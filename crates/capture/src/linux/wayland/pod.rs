//! POD de SPA (el formato binario de parámetros de PipeWire), en Rust seguro y sin IO.
//!
//! Solo lo necesario para la captura: armar el `EnumFormat` que se ofrece y leer el `Format`
//! que se negoció. Se hace a mano y no con el crate `libspa` porque ese crate no compila contra
//! los headers de PipeWire 0.3.48 (Ubuntu 22.04) desde la versión 0.8, y la 0.7 no compila
//! contra PipeWire 1.x. Los valores de las constantes se comparan con `libspa-sys` en las
//! pruebas de `stream.rs`.
//!
//! Formato: cada POD es `u32 tamaño del cuerpo`, `u32 tipo`, cuerpo, relleno hasta 8 bytes.
//! Todo en el orden de bytes nativo.

/// Tipos de POD (`spa/utils/type.h`).
pub(crate) mod ty {
    pub const ID: u32 = 3;
    pub const RECTANGLE: u32 = 10;
    pub const FRACTION: u32 = 11;
    pub const OBJECT: u32 = 15;
    pub const CHOICE: u32 = 19;
    /// `SPA_TYPE_OBJECT_Format`.
    pub const OBJECT_FORMAT: u32 = 0x40003;
}

/// Ids de parámetro (`spa/param/param.h`).
pub(crate) mod param {
    pub const ENUM_FORMAT: u32 = 3;
    pub const FORMAT: u32 = 4;
}

/// Claves del objeto `Format` (`spa/param/format.h`).
pub(crate) mod key {
    pub const MEDIA_TYPE: u32 = 1;
    pub const MEDIA_SUBTYPE: u32 = 2;
    pub const VIDEO_FORMAT: u32 = 0x20001;
    pub const VIDEO_SIZE: u32 = 0x20003;
    pub const VIDEO_FRAMERATE: u32 = 0x20004;
    pub const VIDEO_MAX_FRAMERATE: u32 = 0x20005;
}

/// `SPA_MEDIA_TYPE_video` y `SPA_MEDIA_SUBTYPE_raw`.
pub(crate) const MEDIA_TYPE_VIDEO: u32 = 2;
pub(crate) const MEDIA_SUBTYPE_RAW: u32 = 1;

/// Tipos de choice (`spa/pod/pod.h`).
pub(crate) mod choice {
    pub const NONE: u32 = 0;
    pub const RANGE: u32 = 1;
    pub const ENUM: u32 = 3;
}

/// Formatos de video de 32 bits (`spa/param/video/raw.h`).
pub(crate) mod video {
    pub const RGBX: u32 = 7;
    pub const BGRX: u32 = 8;
    pub const XRGB: u32 = 9;
    pub const XBGR: u32 = 10;
    pub const RGBA: u32 = 11;
    pub const BGRA: u32 = 12;
    pub const ARGB: u32 = 13;
    pub const ABGR: u32 = 14;
}

fn push_u32(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_ne_bytes());
}

fn pad8(buf: &mut Vec<u8>) {
    while !buf.len().is_multiple_of(8) {
        buf.push(0);
    }
}

/// Un POD completo: encabezado, cuerpo y relleno.
fn pod(type_: u32, body: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(8 + body.len() + 7);
    push_u32(&mut buf, body.len() as u32);
    push_u32(&mut buf, type_);
    buf.extend_from_slice(body);
    pad8(&mut buf);
    buf
}

fn id(value: u32) -> Vec<u8> {
    pod(ty::ID, &value.to_ne_bytes())
}

fn pair(a: u32, b: u32) -> [u8; 8] {
    let mut out = [0u8; 8];
    out[..4].copy_from_slice(&a.to_ne_bytes());
    out[4..].copy_from_slice(&b.to_ne_bytes());
    out
}

/// Un `Choice` con valores del mismo tipo y tamaño. El primero es el preferido.
fn choice(kind: u32, child_type: u32, values: &[&[u8]]) -> Vec<u8> {
    let child_size = values.first().map_or(0, |v| v.len()) as u32;
    let mut body = Vec::new();
    push_u32(&mut body, kind);
    push_u32(&mut body, 0); // flags
    push_u32(&mut body, child_size);
    push_u32(&mut body, child_type);
    for v in values {
        body.extend_from_slice(v);
    }
    pod(ty::CHOICE, &body)
}

fn object(object_type: u32, object_id: u32, props: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut body = Vec::new();
    push_u32(&mut body, object_type);
    push_u32(&mut body, object_id);
    for (key, value) in props {
        push_u32(&mut body, *key);
        push_u32(&mut body, 0); // flags de la propiedad
        body.extend_from_slice(value);
    }
    pod(ty::OBJECT, &body)
}

/// Los formatos que se aceptan, en orden de preferencia.
pub(crate) const FORMATS: [u32; 8] = [
    video::BGRX,
    video::BGRA,
    video::RGBX,
    video::RGBA,
    video::XRGB,
    video::ARGB,
    video::XBGR,
    video::ABGR,
];

/// `EnumFormat` que se ofrece: video crudo de 32 bits, cualquier tamaño, hasta `fps`.
pub(crate) fn enum_format(fps: u32) -> Vec<u8> {
    let fps = fps.clamp(1, 1000);
    let formats: Vec<[u8; 4]> = std::iter::once(FORMATS[0])
        .chain(FORMATS)
        .map(u32::to_ne_bytes)
        .collect();
    let formats: Vec<&[u8]> = formats.iter().map(|f| f.as_slice()).collect();
    let rect = |w, h| pair(w, h);
    let frac = |n, d| pair(n, d);
    object(
        ty::OBJECT_FORMAT,
        param::ENUM_FORMAT,
        &[
            (key::MEDIA_TYPE, id(MEDIA_TYPE_VIDEO)),
            (key::MEDIA_SUBTYPE, id(MEDIA_SUBTYPE_RAW)),
            (key::VIDEO_FORMAT, choice(choice::ENUM, ty::ID, &formats)),
            (
                key::VIDEO_SIZE,
                choice(
                    choice::RANGE,
                    ty::RECTANGLE,
                    &[&rect(1920, 1080), &rect(1, 1), &rect(8192, 8192)],
                ),
            ),
            (
                key::VIDEO_FRAMERATE,
                choice(
                    choice::RANGE,
                    ty::FRACTION,
                    &[&frac(fps, 1), &frac(0, 1), &frac(1000, 1)],
                ),
            ),
            (
                key::VIDEO_MAX_FRAMERATE,
                choice(
                    choice::RANGE,
                    ty::FRACTION,
                    &[&frac(fps, 1), &frac(1, 1), &frac(1000, 1)],
                ),
            ),
        ],
    )
}

/// Lo que importa del formato negociado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VideoFormat {
    /// `SPA_VIDEO_FORMAT_*`.
    pub format: u32,
    /// Ancho en píxeles.
    pub width: u32,
    /// Alto en píxeles.
    pub height: u32,
}

fn read_u32(bytes: &[u8], at: usize) -> Option<u32> {
    let raw = bytes.get(at..at + 4)?;
    Some(u32::from_ne_bytes(raw.try_into().ok()?))
}

/// Valor de una propiedad: `(tipo, bytes)`. Si es un `Choice`, el valor preferido.
fn value_of(type_: u32, body: &[u8]) -> Option<(u32, &[u8])> {
    if type_ != ty::CHOICE {
        return Some((type_, body));
    }
    let size = read_u32(body, 8)? as usize;
    let child_type = read_u32(body, 12)?;
    Some((child_type, body.get(16..16 + size)?))
}

/// Lee un `Format` de video crudo. `None` si no es video crudo o le falta algo.
pub(crate) fn parse_video_format(bytes: &[u8]) -> Option<VideoFormat> {
    let size = read_u32(bytes, 0)? as usize;
    if read_u32(bytes, 4)? != ty::OBJECT || bytes.len() < 8 + size {
        return None;
    }
    let end = 8 + size;
    let (mut media_type, mut media_subtype, mut format, mut dims) = (None, None, None, None);
    let mut at = 16; // encabezado + tipo e id del objeto
    while at + 16 <= end {
        let key = read_u32(bytes, at)?;
        let value_size = read_u32(bytes, at + 8)? as usize;
        let value_type = read_u32(bytes, at + 12)?;
        let body = bytes.get(at + 16..at + 16 + value_size)?;
        let (vt, value) = value_of(value_type, body)?;
        match (key, vt) {
            (key::MEDIA_TYPE, ty::ID) => media_type = read_u32(value, 0),
            (key::MEDIA_SUBTYPE, ty::ID) => media_subtype = read_u32(value, 0),
            (key::VIDEO_FORMAT, ty::ID) => format = read_u32(value, 0),
            (key::VIDEO_SIZE, ty::RECTANGLE) => {
                dims = Some((read_u32(value, 0)?, read_u32(value, 4)?))
            }
            _ => {}
        }
        at += 16 + value_size.next_multiple_of(8);
    }
    if media_type? != MEDIA_TYPE_VIDEO || media_subtype? != MEDIA_SUBTYPE_RAW {
        return None;
    }
    let (width, height) = dims?;
    Some(VideoFormat {
        format: format?,
        width,
        height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_id_ocupa_16_bytes() {
        let p = id(2);
        assert_eq!(p.len(), 16);
        assert_eq!(read_u32(&p, 0), Some(4));
        assert_eq!(read_u32(&p, 4), Some(ty::ID));
        assert_eq!(read_u32(&p, 8), Some(2));
    }

    #[test]
    fn choice_de_rectangulos() {
        let p = choice(choice::RANGE, ty::RECTANGLE, &[&pair(1, 2), &pair(3, 4)]);
        // 8 encabezado + 16 (tipo, flags, encabezado del hijo) + 2 × 8 valores.
        assert_eq!(p.len(), 40);
        assert_eq!(read_u32(&p, 0), Some(32));
        assert_eq!(read_u32(&p, 4), Some(ty::CHOICE));
        assert_eq!(read_u32(&p, 8), Some(choice::RANGE));
        assert_eq!(read_u32(&p, 16), Some(8));
        assert_eq!(read_u32(&p, 20), Some(ty::RECTANGLE));
    }

    #[test]
    fn el_enum_format_se_vuelve_a_leer() {
        let bytes = enum_format(20);
        assert!(bytes.len().is_multiple_of(8));
        assert_eq!(read_u32(&bytes, 0).unwrap() as usize + 8, bytes.len());
        assert_eq!(read_u32(&bytes, 8), Some(ty::OBJECT_FORMAT));
        assert_eq!(read_u32(&bytes, 12), Some(param::ENUM_FORMAT));
        // Con choices, el parser toma el valor preferido.
        assert_eq!(
            parse_video_format(&bytes),
            Some(VideoFormat {
                format: video::BGRX,
                width: 1920,
                height: 1080
            })
        );
    }

    #[test]
    fn lee_un_format_fijo() {
        let bytes = object(
            ty::OBJECT_FORMAT,
            param::FORMAT,
            &[
                (key::MEDIA_TYPE, id(MEDIA_TYPE_VIDEO)),
                (key::MEDIA_SUBTYPE, id(MEDIA_SUBTYPE_RAW)),
                (key::VIDEO_FORMAT, id(video::RGBA)),
                (key::VIDEO_SIZE, pod(ty::RECTANGLE, &pair(2560, 1440))),
                (key::VIDEO_FRAMERATE, pod(ty::FRACTION, &pair(0, 1))),
            ],
        );
        assert_eq!(
            parse_video_format(&bytes),
            Some(VideoFormat {
                format: video::RGBA,
                width: 2560,
                height: 1440
            })
        );
    }

    #[test]
    fn rechaza_lo_que_no_es_video_crudo() {
        let audio = object(
            ty::OBJECT_FORMAT,
            param::FORMAT,
            &[
                (key::MEDIA_TYPE, id(1)),
                (key::MEDIA_SUBTYPE, id(MEDIA_SUBTYPE_RAW)),
            ],
        );
        assert_eq!(parse_video_format(&audio), None);
        assert_eq!(parse_video_format(&[0u8; 4]), None);
        let mut cortado = enum_format(20);
        cortado.truncate(40);
        assert_eq!(parse_video_format(&cortado), None);
    }
}
