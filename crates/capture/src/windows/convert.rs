//! Conversiones puras del backend de Windows: sin FFI, así que también se prueban en macOS y Linux.
//!
//! - Elegir el nivel de mipmap que la GPU deja apenas por encima del tamaño pedido.
//! - Pasar un píxel HDR (scRGB lineal en half float) a SDR (sRGB de 8 bits).
//! - Reducir por promedio de áreas (box filter) el nivel de mipmap al tamaño exacto pedido.

/// Nivel de mipmap más chico cuyo tamaño todavía alcanza o supera `target` en las dos dimensiones.
///
/// D3D11 calcula cada nivel como `max(1, lado >> nivel)`. Si la pantalla ya es más chica que
/// `target`, el nivel es 0 (se usa la imagen entera y la reducción en CPU agranda).
pub(crate) fn mip_level_for(src: (u32, u32), target: (u32, u32)) -> u32 {
    let (tw, th) = (target.0.max(1), target.1.max(1));
    let mut level = 0;
    // El último nivel de la cadena es el que queda en 1×1: más allá no hay niveles.
    while level < 31
        && (src.0 >> (level + 1) > 0 || src.1 >> (level + 1) > 0)
        && mip_size(src, level + 1).0 >= tw
        && mip_size(src, level + 1).1 >= th
    {
        level += 1;
    }
    level
}

/// Tamaño del nivel `level` de una textura de `src` píxeles.
pub(crate) fn mip_size(src: (u32, u32), level: u32) -> (u32, u32) {
    ((src.0 >> level).max(1), (src.1 >> level).max(1))
}

/// Decodifica un half float IEEE 754 (binary16).
pub(crate) fn f16_to_f32(bits: u16) -> f32 {
    let sign = if bits & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = i32::from((bits >> 10) & 0x1f);
    let mant = f32::from(bits & 0x3ff);
    match exp {
        0 => sign * mant * 2f32.powi(-24),
        0x1f if mant == 0.0 => sign * f32::INFINITY,
        0x1f => f32::NAN,
        _ => sign * (1.0 + mant / 1024.0) * 2f32.powi(exp - 15),
    }
}

/// Curva de codificación sRGB (lineal `0..1` → codificado `0..1`).
pub(crate) fn srgb_encode(c: f32) -> f32 {
    if c <= 0.003_130_8 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// Pasa un color scRGB lineal a sRGB de 8 bits.
///
/// En scRGB, `1.0` son 80 nits. Con HDR activo, Windows compone el escritorio SDR para que su
/// blanco quede en `white_scale` (= `SDRWhiteLevel / 1000` de DisplayConfig). Por eso:
/// 1. se divide por `white_scale`: el contenido SDR vuelve exacto a `0..1`;
/// 2. lo negativo (colores fuera de la gama Rec.709) y los NaN se llevan a 0;
/// 3. lo que pasa del blanco SDR (brillos HDR) se escala para que el canal más alto quede en 1.
///    Así se conservan el tono y la saturación, que es lo que importa para el foco: recortar
///    canal por canal cambiaría el tono;
/// 4. se aplica la curva sRGB.
pub(crate) fn scrgb_to_srgb8(rgb: [f32; 3], white_scale: f32) -> [u8; 3] {
    let scale = if white_scale.is_finite() && white_scale > 0.0 {
        white_scale
    } else {
        1.0
    };
    let mut c = rgb.map(|v| {
        let v = v / scale;
        if v.is_nan() { 0.0 } else { v.max(0.0) }
    });
    let max = c[0].max(c[1]).max(c[2]);
    if max > 1.0 {
        // Un canal infinito deja a los demás en 0 y a ese en 1: sigue siendo el tono correcto.
        c = c.map(|v| {
            if max.is_finite() {
                v / max
            } else if v.is_infinite() {
                1.0
            } else {
                0.0
            }
        });
    }
    c.map(|v| (srgb_encode(v.clamp(0.0, 1.0)) * 255.0).round() as u8)
}

/// Convierte una imagen RGBA half float (scRGB), con `pitch` bytes por fila, a BGRA de 8 bits
/// sin relleno.
pub(crate) fn rgba16f_to_bgra8(
    src: &[u8],
    width: u32,
    height: u32,
    pitch: usize,
    white_scale: f32,
) -> Vec<u8> {
    let (w, h) = (width as usize, height as usize);
    let mut out = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        let row = &src[y * pitch..y * pitch + w * 8];
        for px in row.as_chunks::<8>().0 {
            let ch = |i: usize| f16_to_f32(u16::from_le_bytes([px[i * 2], px[i * 2 + 1]]));
            let [r, g, b] = scrgb_to_srgb8([ch(0), ch(1), ch(2)], white_scale);
            out.extend_from_slice(&[b, g, r, 255]);
        }
    }
    out
}

/// Reduce (o agranda) una imagen BGRA de 8 bits a `dst` con promedio de áreas.
///
/// `pitch` son los bytes por fila de la fuente (puede tener relleno, como una textura mapeada).
/// Cada píxel de destino promedia el rectángulo de la fuente que le toca; si la fuente es más
/// chica que el destino, toma el píxel más cercano. El alfa de salida siempre es 255.
pub(crate) fn box_resample_bgra8(
    src: &[u8],
    src_size: (u32, u32),
    pitch: usize,
    dst: (u32, u32),
) -> Vec<u8> {
    let (sw, sh) = (src_size.0 as usize, src_size.1 as usize);
    let (dw, dh) = (dst.0.max(1) as usize, dst.1.max(1) as usize);
    let span = |d: usize, s: usize, n: usize| {
        let a = d * s / n;
        let b = ((d + 1) * s / n).max(a + 1).min(s);
        (a.min(s - 1), b)
    };
    let mut out = Vec::with_capacity(dw * dh * 4);
    for dy in 0..dh {
        let (y0, y1) = span(dy, sh, dh);
        for dx in 0..dw {
            let (x0, x1) = span(dx, sw, dw);
            let mut acc = [0u32; 3];
            for y in y0..y1 {
                let row = &src[y * pitch..];
                for x in x0..x1 {
                    let p = &row[x * 4..x * 4 + 3];
                    acc[0] += u32::from(p[0]);
                    acc[1] += u32::from(p[1]);
                    acc[2] += u32::from(p[2]);
                }
            }
            let n = ((y1 - y0) * (x1 - x0)) as u32;
            let avg = acc.map(|v| ((v + n / 2) / n) as u8);
            out.extend_from_slice(&[avg[0], avg[1], avg[2], 255]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nivel_de_mipmap_para_4k_es_5() {
        // 3840×2160 >> 5 = 120×67, que todavía cubre 96×54; el nivel 6 (60×33) ya no.
        assert_eq!(mip_level_for((3840, 2160), (96, 54)), 5);
        assert_eq!(mip_size((3840, 2160), 5), (120, 67));
    }

    #[test]
    fn nivel_de_mipmap_para_otras_pantallas() {
        assert_eq!(mip_level_for((1920, 1080), (96, 54)), 4);
        assert_eq!(mip_level_for((2560, 1440), (96, 54)), 4);
        assert_eq!(mip_level_for((96, 54), (96, 54)), 0);
        // Pantalla más chica que el objetivo: se usa entera.
        assert_eq!(mip_level_for((64, 32), (96, 54)), 0);
        // Ultrawide: manda el alto.
        assert_eq!(mip_level_for((5120, 1440), (96, 54)), 4);
        // Objetivo 0 no cuelga: se trata como 1×1.
        assert_eq!(mip_level_for((8, 8), (0, 0)), 3);
        // Nunca pasa del último nivel real (1×1), aunque el objetivo sea 1×1.
        assert_eq!(mip_level_for((3840, 2160), (1, 1)), 11);
    }

    #[test]
    fn half_float_valores_conocidos() {
        assert_eq!(f16_to_f32(0x0000), 0.0);
        assert_eq!(f16_to_f32(0x3c00), 1.0);
        assert_eq!(f16_to_f32(0xc000), -2.0);
        assert_eq!(f16_to_f32(0x3800), 0.5);
        assert_eq!(f16_to_f32(0x7bff), 65504.0);
        assert_eq!(f16_to_f32(0x0001), 2f32.powi(-24));
        assert_eq!(f16_to_f32(0x7c00), f32::INFINITY);
        assert!(f16_to_f32(0x7e00).is_nan());
    }

    #[test]
    fn srgb_extremos_y_gris_medio() {
        assert_eq!(srgb_encode(0.0), 0.0);
        assert!((srgb_encode(1.0) - 1.0).abs() < 1e-6);
        // 18% lineal ≈ 118/255 en sRGB.
        assert_eq!((srgb_encode(0.18) * 255.0).round() as u8, 118);
    }

    #[test]
    fn hdr_contenido_sdr_vuelve_exacto() {
        // Escritorio con HDR y blanco SDR en 240 nits (SDRWhiteLevel = 3000 → escala 3).
        let escala = 3.0;
        assert_eq!(scrgb_to_srgb8([3.0, 3.0, 3.0], escala), [255, 255, 255]);
        assert_eq!(scrgb_to_srgb8([0.0, 0.0, 0.0], escala), [0, 0, 0]);
        // Un gris sRGB 128 linealizado y llevado al blanco de 240 nits vuelve a 128.
        let lin = ((128.0f32 / 255.0 + 0.055) / 1.055).powf(2.4) * escala;
        assert_eq!(scrgb_to_srgb8([lin, lin, lin], escala), [128, 128, 128]);
    }

    #[test]
    fn hdr_brillos_conservan_el_tono() {
        // Naranja HDR a 4× el blanco SDR: se escala entero, no se recorta canal por canal.
        let [r, g, b] = scrgb_to_srgb8([4.0, 2.0, 0.0], 1.0);
        assert_eq!(r, 255);
        assert_eq!(g, (srgb_encode(0.5) * 255.0).round() as u8);
        assert_eq!(b, 0);
    }

    #[test]
    fn hdr_fuera_de_gama_negativos_y_nan() {
        assert_eq!(scrgb_to_srgb8([-0.5, 1.0, f32::NAN], 1.0), [0, 255, 0]);
        assert_eq!(scrgb_to_srgb8([f32::INFINITY, 0.5, 0.0], 1.0), [255, 0, 0]);
        // Escala inválida: se usa 1.
        assert_eq!(scrgb_to_srgb8([1.0, 1.0, 1.0], 0.0), [255, 255, 255]);
    }

    #[test]
    fn rgba16f_a_bgra8_respeta_pitch_y_orden() {
        // 1×2 píxeles: rojo puro y azul puro, con 8 bytes de relleno por fila.
        let uno = 0x3c00u16.to_le_bytes();
        let cero = 0u16.to_le_bytes();
        let mut src = Vec::new();
        for px in [[uno, cero, cero, uno], [cero, cero, uno, uno]] {
            for c in px {
                src.extend_from_slice(&c);
            }
            src.extend_from_slice(&[0xAA; 8]);
        }
        let out = rgba16f_to_bgra8(&src, 1, 2, 16, 1.0);
        assert_eq!(out, vec![0, 0, 255, 255, 255, 0, 0, 255]);
    }

    #[test]
    fn box_promedia_bloques() {
        // 4×2 → 2×1: el bloque izquierdo promedia 0 y 100; el derecho, 200 y 255.
        let mut src = Vec::new();
        for y in 0..2 {
            for x in 0..4 {
                let v = match (x / 2, y) {
                    (0, 0) => 0,
                    (0, _) => 100,
                    (_, 0) => 200,
                    _ => 255,
                };
                src.extend_from_slice(&[v, v, v, 7]);
            }
        }
        let out = box_resample_bgra8(&src, (4, 2), 16, (2, 1));
        assert_eq!(out, vec![50, 50, 50, 255, 228, 228, 228, 255]);
    }

    #[test]
    fn box_ignora_el_relleno_y_agranda() {
        // 1×1 con relleno → 3×2: todos los píxeles copian el único de la fuente.
        let src = [10, 20, 30, 0, 99, 99, 99, 99];
        let out = box_resample_bgra8(&src, (1, 1), 8, (3, 2));
        assert_eq!(out.len(), 3 * 2 * 4);
        assert!(
            out.as_chunks::<4>()
                .0
                .iter()
                .all(|p| p == &[10, 20, 30, 255])
        );
    }

    #[test]
    fn box_de_120x67_a_96x54() {
        let src = vec![128u8; 120 * 67 * 4];
        let out = box_resample_bgra8(&src, (120, 67), 120 * 4, (96, 54));
        assert_eq!(out.len(), 96 * 54 * 4);
        assert!(
            out.as_chunks::<4>()
                .0
                .iter()
                .all(|p| p == &[128, 128, 128, 255])
        );
    }
}
