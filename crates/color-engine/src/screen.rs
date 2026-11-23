//! `color_de_pantalla` y `zona_dominante`: el color representativo de una zona de la pantalla.
//!
//! Port de `ambilight.py`. El script trabaja con arreglos de numpy en `float32`; acá cada paso
//! repite el mismo tipo y el mismo orden de operaciones (ver [`crate::pymath`]):
//!
//! | numpy | acá |
//! |---|---|
//! | `img.astype(np.float32) / 255.0` | tabla de 256 `f32` con la misma división |
//! | `peso.sum()` (1D contiguo) | suma por pares de numpy en `f32` |
//! | `(rgb * peso[:, None]).sum(axis=0)` | suma secuencial en `f32` por canal |
//! | `np.histogram(..., weights=)` | `bincount`: acumula cada bin en `f64` y pasa a `f32` |
//! | `brillos[...].mean()` | suma por pares en `f32`, dividida en `f64` y pasada a `f32` |
//! | `separacion_tonos(tono, centro)` | en `f64` (el centro es un `np.float64`) |

use openlight_types::{ColorMode, Frame, NormRect, ScreenColor};

use crate::ColorParams;
use crate::pymath::{pairwise_sum_f32, py_mod, py_mod_f32, rgb_to_hsv};

/// Bins del histograma de tonos (de 10° cada uno).
pub const HUE_BINS: usize = 36;
/// Peso mínimo de un píxel gris en el promedio (`0.1 + sat` en `color_de_pantalla`).
const GREY_WEIGHT: f32 = 0.1;
/// Píxeles a menos de estos grados del tono ganador forman parte de la zona dominante.
const DOMINANT_SPREAD_DEG: f64 = 35.0;
/// Bins a menos de estos grados del tono previo reciben la ventaja (`<= 30`).
const PREVIOUS_HUE_RANGE_DEG: f64 = 30.0;
/// Píxeles con menos saturación que esto (neutros) siempre entran en la zona dominante.
const NEUTRAL_SAT: f32 = 0.25;
/// Por debajo de este peso total la zona es negra (`peso.sum() < 1e-3`).
const MIN_TOTAL_WEIGHT: f32 = 1e-3;
/// Piso de los divisores (`np.maximum(..., 1e-6)`).
const EPS: f32 = 1e-6;

/// `u8 as f32 / 255.0_f32`, igual que `astype(np.float32) / 255.0`.
static UNIT: [f32; 256] = {
    let mut t = [0.0_f32; 256];
    let mut i = 0;
    while i < 256 {
        t[i] = i as f32 / 255.0;
        i += 1;
    }
    t
};

/// Color representativo de una zona de un cuadro BGRA (`color_de_pantalla` del script).
///
/// - `zone` se recorta sobre el cuadro sin copiar los píxeles ([`NormRect::to_pixels`]).
/// - `prev_hue` es el `h` del cuadro anterior (`tono_previo`): en modo dominante le da ventaja
///   a ese tono para no saltar entre dos colores que ocupan casi lo mismo.
/// - Devuelve `h = None` (y `s = 0`) si la zona es negra o gris. `luz` es el brillo medio sin
///   contar las bandas negras.
///
/// Un cuadro vacío o con un búfer más chico que `width × height × 4` da negro.
#[doc(alias = "color_de_pantalla")]
pub fn screen_color(
    frame: &Frame,
    zone: NormRect,
    prev_hue: Option<f32>,
    params: &ColorParams,
) -> ScreenColor {
    let (width, height) = (frame.width as usize, frame.height as usize);
    if width == 0 || height == 0 || frame.bgra.len() < width * height * 4 {
        return ScreenColor {
            h: None,
            s: 0.0,
            luz: 0.0,
        };
    }
    let (x0, y0, x1, y1) = zone.to_pixels(frame.width, frame.height);
    let view = View {
        data: &frame.bgra,
        stride: width * 4,
        x0: x0 as usize,
        y0: y0 as usize,
        width: (x1 - x0) as usize,
        height: (y1 - y0) as usize,
    };
    let (h, s, luz) = screen_color_view(&view, prev_hue.map(f64::from), params);
    ScreenColor {
        h: h.map(|h| h as f32),
        s: s as f32,
        luz,
    }
}

/// Un rectángulo de un búfer BGRA, sin copiar.
struct View<'a> {
    data: &'a [u8],
    stride: usize,
    x0: usize,
    y0: usize,
    width: usize,
    height: usize,
}

impl View<'_> {
    /// Las filas del rectángulo, cada una con `width * 4` bytes.
    fn rows(&self) -> impl Iterator<Item = &[u8]> {
        (self.y0..self.y0 + self.height).map(move |y| {
            let start = y * self.stride + self.x0 * 4;
            &self.data[start..start + self.width * 4]
        })
    }
}

/// `separacion_tonos` en `f64`: distancia circular entre dos tonos en grados (0..=180).
#[doc(alias = "separacion_tonos")]
pub fn hue_separation(a_deg: f64, b_deg: f64) -> f64 {
    (py_mod(a_deg - b_deg + 180.0, 360.0) - 180.0).abs()
}

/// El núcleo de `color_de_pantalla`: `(h, s, luz)` con `h` y `s` en `f64` como `colorsys`.
fn screen_color_view(
    view: &View<'_>,
    prev_hue: Option<f64>,
    params: &ColorParams,
) -> (Option<f64>, f64, f32) {
    let n = view.width * view.height;
    let dominant = params.mode == ColorMode::Dominant;

    // rgb, mx (= brillos), sat y, en modo dominante, el tono de cada píxel
    let mut rgb = Vec::with_capacity(n);
    let mut mx = Vec::with_capacity(n);
    let mut sat = Vec::with_capacity(n);
    let mut hue = Vec::with_capacity(if dominant { n } else { 0 });
    let mut row_max = vec![0.0_f32; view.height];
    let mut col_max = vec![0.0_f32; view.width];

    for (y, row) in view.rows().enumerate() {
        for (x, px) in row.as_chunks::<4>().0.iter().enumerate() {
            let (r, g, b) = (
                UNIT[px[2] as usize],
                UNIT[px[1] as usize],
                UNIT[px[0] as usize],
            );
            let hi = r.max(g).max(b);
            let lo = r.min(g).min(b);
            rgb.push([r, g, b]);
            mx.push(hi);
            sat.push((hi - lo) / hi.max(EPS));
            row_max[y] = row_max[y].max(hi);
            col_max[x] = col_max[x].max(hi);
            if dominant {
                hue.push(pixel_hue(r, g, b, hi, lo));
            }
        }
    }

    let luz = light(&mx, &row_max, &col_max, view.width, params.black_bar);

    // Los píxeles oscuros y grises (barras negras, UI) pesan menos
    let mut weight: Vec<f32> = mx
        .iter()
        .zip(&sat)
        .map(|(&m, &s)| m * m * (GREY_WEIGHT + s))
        .collect();
    if dominant {
        let center = dominant_hue_center(&hue, &mx, &sat, prev_hue, params.hue_stickiness);
        for ((w, &t), &s) in weight.iter_mut().zip(&hue).zip(&sat) {
            let inside =
                hue_separation(f64::from(t), center) < DOMINANT_SPREAD_DEG || s < NEUTRAL_SAT;
            *w *= if inside { 1.0 } else { 0.0 };
        }
    }

    let total = pairwise_sum_f32(&weight);
    if total < MIN_TOTAL_WEIGHT {
        return (None, 0.0, luz);
    }
    let mut acc = [0.0_f32; 3];
    for (c, &w) in rgb.iter().zip(&weight) {
        acc[0] += c[0] * w;
        acc[1] += c[1] * w;
        acc[2] += c[2] * w;
    }
    let color = acc.map(|a| f64::from(a / total));
    let (h, s, _) = rgb_to_hsv(color[0], color[1], color[2]);
    if s >= params.grey_threshold {
        (Some(h), s, luz)
    } else {
        (None, 0.0, luz)
    }
}

/// Tono de un píxel en grados `0..=360`, en `f32` (el `np.where` de `zona_dominante`).
fn pixel_hue(r: f32, g: f32, b: f32, hi: f32, lo: f32) -> f32 {
    let range = (hi - lo).max(EPS);
    let sextant = if hi == r {
        py_mod_f32((g - b) / range, 6.0)
    } else if hi == g {
        (b - r) / range + 2.0
    } else {
        (r - g) / range + 4.0
    };
    sextant * 60.0
}

/// Luz media sin las bandas negras: `brillos[filas][:, columnas].mean()`.
fn light(mx: &[f32], row_max: &[f32], col_max: &[f32], width: usize, black_bar: f32) -> f32 {
    if !row_max.iter().any(|&m| m > black_bar) {
        return 0.0;
    }
    let cols: Vec<usize> = (0..width).filter(|&x| col_max[x] > black_bar).collect();
    let mut kept = Vec::with_capacity(mx.len());
    for (y, _) in row_max.iter().enumerate().filter(|(_, m)| **m > black_bar) {
        let row = &mx[y * width..(y + 1) * width];
        kept.extend(cols.iter().map(|&x| row[x]));
    }
    // numpy: suma en float32 y divide por un np.intp, o sea en float64
    (f64::from(pairwise_sum_f32(&kept)) / kept.len() as f64) as f32
}

/// Bin de `np.histogram(..., bins=36, range=(0, 360))` para un tono, o `None` si queda afuera.
///
/// Repite el cálculo de numpy (índice aproximado y corrección contra los bordes exactos), así
/// un tono justo en un borde cae en el mismo bin.
fn hue_bin(t: f32) -> Option<usize> {
    if !(0.0..=360.0).contains(&t) {
        return None;
    }
    let edge = |i: usize| i as f32 * 10.0;
    let mut i = ((t / 360.0) * HUE_BINS as f32) as usize;
    if i == HUE_BINS {
        i -= 1;
    }
    if t < edge(i) {
        i -= 1;
    }
    if t >= edge(i + 1) && i != HUE_BINS - 1 {
        i += 1;
    }
    Some(i)
}

/// `zona_dominante` hasta elegir el tono: centro (en grados, `f64`) del bin que gana.
fn dominant_hue_center(
    hue: &[f32],
    mx: &[f32],
    sat: &[f32],
    prev_hue: Option<f64>,
    stickiness: f64,
) -> f64 {
    // np.histogram con pesos float32: np.bincount acumula en float64 y se vuelve a float32
    let mut acc = [0.0_f64; HUE_BINS];
    for ((&t, &m), &s) in hue.iter().zip(mx).zip(sat) {
        if let Some(i) = hue_bin(t) {
            acc[i] += f64::from(m * m * s);
        }
    }
    let hist: [f32; HUE_BINS] = acc.map(|v| v as f32);
    // Suavizado circular: cada bin suma la mitad de sus vecinos
    let smooth: [f32; HUE_BINS] = std::array::from_fn(|k| {
        hist[k] + (hist[(k + HUE_BINS - 1) % HUE_BINS] + hist[(k + 1) % HUE_BINS]) / 2.0
    });
    let center = |k: usize| k as f64 * 10.0 + 5.0;
    // Con tono previo, numpy multiplica por un arreglo float64 y el histograma pasa a float64
    let scores: [f64; HUE_BINS] = std::array::from_fn(|k| {
        let v = f64::from(smooth[k]);
        match prev_hue {
            Some(p) if hue_separation(center(k), p * 360.0) <= PREVIOUS_HUE_RANGE_DEG => {
                v * stickiness
            }
            _ => v,
        }
    });
    // argmax: el primero de los máximos
    let mut best = 0;
    for k in 1..HUE_BINS {
        if scores[k] > scores[best] {
            best = k;
        }
    }
    center(best)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bins_en_los_bordes() {
        assert_eq!(hue_bin(0.0), Some(0));
        assert_eq!(hue_bin(9.999_999), Some(0));
        assert_eq!(hue_bin(10.0), Some(1));
        assert_eq!(hue_bin(30.0), Some(3));
        assert_eq!(hue_bin(359.9), Some(35));
        assert_eq!(hue_bin(360.0), Some(35));
        assert_eq!(hue_bin(-0.1), None);
        assert_eq!(hue_bin(360.1), None);
    }

    #[test]
    fn separacion_circular() {
        assert_eq!(hue_separation(0.0, 359.0), 1.0);
        assert_eq!(hue_separation(10.0, 350.0), 20.0);
        assert_eq!(hue_separation(90.0, 270.0), 180.0);
    }

    #[test]
    fn cuadro_vacio_es_negro() {
        let frame = Frame {
            width: 0,
            height: 0,
            bgra: Vec::new().into(),
        };
        let c = screen_color(&frame, NormRect::FULL, None, &ColorParams::default());
        assert_eq!(
            c,
            ScreenColor {
                h: None,
                s: 0.0,
                luz: 0.0
            }
        );
    }

    #[test]
    fn la_zona_recorta_el_cuadro() {
        // Mitad izquierda roja, mitad derecha azul
        let (w, h) = (8_u32, 2_u32);
        let mut bgra = Vec::new();
        for _ in 0..h {
            for x in 0..w {
                bgra.extend_from_slice(if x < 4 {
                    &[0, 0, 255, 255]
                } else {
                    &[255, 0, 0, 255]
                });
            }
        }
        let frame = Frame::new(w, h, bgra).unwrap();
        let p = ColorParams::default();
        let izq = screen_color(&frame, NormRect::new(0.0, 0.0, 0.5, 1.0), None, &p);
        let der = screen_color(&frame, NormRect::new(0.5, 0.0, 0.5, 1.0), None, &p);
        assert_eq!(izq.h, Some(0.0));
        assert!((der.h.unwrap() - 2.0 / 3.0).abs() < 1e-6);
    }
}
