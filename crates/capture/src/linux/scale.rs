//! Conversión de píxeles y reducción a la miniatura BGRA. Código puro, sin IO.
//!
//! Lo usan los dos backends: X11 reduce primero en el servidor (XRender) a 4× el tamaño pedido y
//! acá promedia bloques de 4×4; PipeWire entrega el cuadro entero y acá se promedia una grilla
//! rala de puntos por celda, sin recorrer todos los píxeles.

/// Un canal de color dentro de una palabra de 32 bits: posición y cantidad de bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Channel {
    shift: u8,
    bits: u8,
}

impl Channel {
    /// Arma el canal desde una máscara contigua (por ejemplo `0x00ff_0000`).
    pub(crate) fn from_mask(mask: u32) -> Option<Self> {
        if mask == 0 {
            return None;
        }
        let shift = mask.trailing_zeros();
        let bits = (mask >> shift).trailing_ones();
        if (mask >> shift) >> bits != 0 || bits > 16 {
            return None; // máscara con huecos o rara
        }
        Some(Self {
            shift: shift as u8,
            bits: bits as u8,
        })
    }

    #[inline]
    fn extract(self, word: u32) -> u8 {
        let max = (1u32 << self.bits) - 1;
        let value = (word >> self.shift) & max;
        if self.bits == 8 {
            value as u8
        } else {
            ((value * 255 + max / 2) / max) as u8
        }
    }
}

/// Cómo están guardados los canales de un píxel de 4 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PixelLayout {
    /// Bytes B, G, R, X/A en memoria (BGRx/BGRA de PipeWire, X11 de 24 bits en little endian).
    Bgrx,
    /// Bytes R, G, B, X/A.
    Rgbx,
    /// Bytes X/A, R, G, B.
    Xrgb,
    /// Bytes X/A, B, G, R.
    Xbgr,
    /// Palabra de 32 bits con máscaras arbitrarias (X11 con otro orden de bytes o 30 bits).
    Masked {
        /// Canal rojo.
        red: Channel,
        /// Canal verde.
        green: Channel,
        /// Canal azul.
        blue: Channel,
        /// La palabra está en big endian (`image-byte-order` MSBFirst).
        big_endian: bool,
    },
}

impl PixelLayout {
    /// Elige el formato desde las máscaras de X11. Reconoce los casos comunes para ir más rápido.
    pub(crate) fn from_masks(red: u32, green: u32, blue: u32, big_endian: bool) -> Option<Self> {
        let common = (red, green, blue) == (0x00ff_0000, 0x0000_ff00, 0x0000_00ff);
        let swapped = (red, green, blue) == (0x0000_00ff, 0x0000_ff00, 0x00ff_0000);
        Some(match (common, swapped, big_endian) {
            (true, _, false) => Self::Bgrx,
            (true, _, true) => Self::Xrgb,
            (_, true, false) => Self::Rgbx,
            (_, true, true) => Self::Xbgr,
            _ => Self::Masked {
                red: Channel::from_mask(red)?,
                green: Channel::from_mask(green)?,
                blue: Channel::from_mask(blue)?,
                big_endian,
            },
        })
    }

    /// `(r, g, b)` de un píxel.
    #[inline]
    fn rgb(self, px: &[u8]) -> (u8, u8, u8) {
        match self {
            Self::Bgrx => (px[2], px[1], px[0]),
            Self::Rgbx => (px[0], px[1], px[2]),
            Self::Xrgb => (px[1], px[2], px[3]),
            Self::Xbgr => (px[3], px[2], px[1]),
            Self::Masked {
                red,
                green,
                blue,
                big_endian,
            } => {
                let bytes = [px[0], px[1], px[2], px[3]];
                let word = if big_endian {
                    u32::from_be_bytes(bytes)
                } else {
                    u32::from_le_bytes(bytes)
                };
                (red.extract(word), green.extract(word), blue.extract(word))
            }
        }
    }
}

/// Una imagen de origen de 4 bytes por píxel, con filas de `stride` bytes.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SourceImage<'a> {
    /// Píxeles.
    pub data: &'a [u8],
    /// Ancho en píxeles.
    pub width: u32,
    /// Alto en píxeles.
    pub height: u32,
    /// Bytes por fila (≥ `width * 4`).
    pub stride: usize,
    /// Formato de cada píxel.
    pub layout: PixelLayout,
}

impl SourceImage<'_> {
    fn validate(&self) -> Result<(), String> {
        if self.width == 0 || self.height == 0 {
            return Err("imagen vacía".into());
        }
        let row = self.width as usize * 4;
        if self.stride < row {
            return Err(format!("stride {} menor que la fila ({row})", self.stride));
        }
        let needed = self.stride * (self.height as usize - 1) + row;
        if self.data.len() < needed {
            return Err(format!(
                "búfer de {} bytes, hacen falta {needed}",
                self.data.len()
            ));
        }
        Ok(())
    }
}

/// Reduce `src` a `target` (ancho, alto) en BGRA con alfa 255.
///
/// Cada píxel de salida promedia hasta `samples × samples` puntos repartidos de forma pareja en
/// su celda. Si la celda es más chica que la grilla, se usan todos sus píxeles.
pub(crate) fn downscale(
    src: &SourceImage<'_>,
    target: (u32, u32),
    samples: u32,
) -> Result<Vec<u8>, String> {
    src.validate()?;
    let (tw, th) = target;
    if tw == 0 || th == 0 {
        return Err("tamaño de salida vacío".into());
    }
    let samples = samples.max(1);
    let xs = sample_positions(src.width, tw, samples);
    let ys = sample_positions(src.height, th, samples);

    let mut out = Vec::with_capacity(tw as usize * th as usize * 4);
    for rows in &ys {
        for cols in &xs {
            let (mut r, mut g, mut b) = (0u32, 0u32, 0u32);
            for &y in rows {
                let base = y as usize * src.stride;
                for &x in cols {
                    let at = base + x as usize * 4;
                    let (pr, pg, pb) = src.layout.rgb(&src.data[at..at + 4]);
                    r += u32::from(pr);
                    g += u32::from(pg);
                    b += u32::from(pb);
                }
            }
            let n = (rows.len() * cols.len()) as u32;
            let avg = |sum: u32| ((sum + n / 2) / n) as u8;
            out.extend_from_slice(&[avg(b), avg(g), avg(r), 255]);
        }
    }
    Ok(out)
}

/// Para cada celda de salida, las coordenadas de origen que se promedian.
fn sample_positions(src_len: u32, dst_len: u32, samples: u32) -> Vec<Vec<u32>> {
    let (src_len, dst_len, samples) = (u64::from(src_len), u64::from(dst_len), u64::from(samples));
    (0..dst_len)
        .map(|i| {
            let start = i * src_len / dst_len;
            let end = ((i + 1) * src_len / dst_len).max(start + 1).min(src_len);
            let cell = end - start;
            let n = samples.min(cell);
            // Centros de `n` franjas iguales dentro de la celda.
            (0..n)
                .map(|k| (start + (2 * k + 1) * cell / (2 * n)) as u32)
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(width: u32, height: u32, pixels: &[[u8; 4]]) -> Vec<u8> {
        assert_eq!(pixels.len(), (width * height) as usize);
        pixels.iter().flatten().copied().collect()
    }

    fn src(data: &[u8], width: u32, height: u32, layout: PixelLayout) -> SourceImage<'_> {
        SourceImage {
            data,
            width,
            height,
            stride: width as usize * 4,
            layout,
        }
    }

    #[test]
    fn un_color_liso_queda_igual() {
        let data = image(4, 2, &[[10, 20, 30, 0]; 8]);
        let out = downscale(&src(&data, 4, 2, PixelLayout::Bgrx), (2, 1), 4).unwrap();
        assert_eq!(out, vec![10, 20, 30, 255, 10, 20, 30, 255]);
    }

    #[test]
    fn cuadrantes_van_a_su_celda() {
        let (r, g, b, w) = ([0, 0, 255, 0], [0, 255, 0, 0], [255, 0, 0, 0], [255; 4]);
        #[rustfmt::skip]
        let data = image(4, 4, &[
            r, r, g, g,
            r, r, g, g,
            b, b, w, w,
            b, b, w, w,
        ]);
        let out = downscale(&src(&data, 4, 4, PixelLayout::Bgrx), (2, 2), 4).unwrap();
        assert_eq!(
            out,
            vec![
                0, 0, 255, 255, 0, 255, 0, 255, 255, 0, 0, 255, 255, 255, 255, 255
            ]
        );
    }

    #[test]
    fn promedia_la_celda_entera() {
        let data = image(4, 1, &[[0; 4], [0; 4], [255; 4], [255; 4]]);
        let out = downscale(&src(&data, 4, 1, PixelLayout::Bgrx), (1, 1), 4).unwrap();
        assert_eq!(out, vec![128, 128, 128, 255]);
    }

    #[test]
    fn muestreo_ralo_no_lee_todo() {
        // 100 columnas en una celda con 4 muestras: centros de franjas de 25.
        assert_eq!(sample_positions(100, 1, 4), vec![vec![12, 37, 62, 87]]);
        // Celdas más chicas que la grilla: se usan todos los píxeles.
        assert_eq!(sample_positions(4, 2, 4), vec![vec![0, 1], vec![2, 3]]);
        // Agrandar: cada celda toma al menos un píxel.
        assert_eq!(
            sample_positions(2, 4, 4),
            vec![vec![0], vec![0], vec![1], vec![1]]
        );
    }

    #[test]
    fn respeta_el_stride() {
        // 1×2 con 4 bytes de relleno por fila.
        let data = [1, 2, 3, 0, 9, 9, 9, 9, 4, 5, 6, 0, 9, 9, 9, 9];
        let img = SourceImage {
            data: &data,
            width: 1,
            height: 2,
            stride: 8,
            layout: PixelLayout::Bgrx,
        };
        let out = downscale(&img, (1, 2), 1).unwrap();
        assert_eq!(out, vec![1, 2, 3, 255, 4, 5, 6, 255]);
    }

    #[test]
    fn convierte_cada_formato_a_bgra() {
        let cases = [
            (PixelLayout::Bgrx, [3, 2, 1, 0]),
            (PixelLayout::Rgbx, [1, 2, 3, 0]),
            (PixelLayout::Xrgb, [0, 1, 2, 3]),
            (PixelLayout::Xbgr, [0, 3, 2, 1]),
        ];
        for (layout, px) in cases {
            let out = downscale(&src(&px, 1, 1, layout), (1, 1), 1).unwrap();
            assert_eq!(out, vec![3, 2, 1, 255], "{layout:?}");
        }
    }

    #[test]
    fn mascaras_de_x11() {
        assert_eq!(
            PixelLayout::from_masks(0xff0000, 0xff00, 0xff, false),
            Some(PixelLayout::Bgrx)
        );
        assert_eq!(
            PixelLayout::from_masks(0xff0000, 0xff00, 0xff, true),
            Some(PixelLayout::Xrgb)
        );
        assert_eq!(
            PixelLayout::from_masks(0xff, 0xff00, 0xff0000, false),
            Some(PixelLayout::Rgbx)
        );
        assert_eq!(PixelLayout::from_masks(0xf0f, 0xff00, 0xff, false), None);

        // 30 bits (10 por canal): blanco y un rojo a media intensidad.
        let deep = PixelLayout::from_masks(0x3ff0_0000, 0x000f_fc00, 0x0000_03ff, false).unwrap();
        let white = 0x3fff_ffffu32.to_le_bytes();
        let half_red = (512u32 << 20).to_le_bytes();
        let data = [white, half_red].concat();
        let out = downscale(&src(&data, 2, 1, deep), (2, 1), 1).unwrap();
        assert_eq!(out, vec![255, 255, 255, 255, 0, 0, 128, 255]);
    }

    #[test]
    fn canal_desde_mascara() {
        assert_eq!(
            Channel::from_mask(0x00ff_0000),
            Some(Channel { shift: 16, bits: 8 })
        );
        assert_eq!(Channel::from_mask(0), None);
        assert_eq!(Channel::from_mask(0b1011), None);
    }

    #[test]
    fn rechaza_buferes_cortos_o_vacios() {
        let data = [0u8; 7];
        assert!(downscale(&src(&data, 2, 1, PixelLayout::Bgrx), (1, 1), 1).is_err());
        let data = [0u8; 8];
        assert!(downscale(&src(&data, 2, 1, PixelLayout::Bgrx), (0, 1), 1).is_err());
        let img = SourceImage {
            data: &data,
            width: 2,
            height: 1,
            stride: 4,
            layout: PixelLayout::Bgrx,
        };
        assert!(downscale(&img, (1, 1), 1).is_err());
    }
}
