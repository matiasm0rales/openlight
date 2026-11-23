//! Benchmark de `screen_color` por zona a 96×54 (criterio de la tarea D: < 0.2 ms).
//!
//!     cargo bench -p color-engine

#![allow(missing_docs)]

use std::hint::black_box;

use color_engine::{ColorParams, screen_color};
use criterion::{Criterion, criterion_group, criterion_main};
use openlight_types::{ColorMode, Frame, NormRect};

/// Una escena variada de 96×54: degradé, bloques y bandas negras arriba y abajo.
fn scene() -> Frame {
    let (w, h) = (96_u32, 54_u32);
    let mut bgra = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let px = if !(7..h - 7).contains(&y) {
                [0, 0, 0, 255]
            } else if (20..60).contains(&x) && (15..35).contains(&y) {
                [30, 200, 60, 255]
            } else {
                [(x * 2) as u8, (y * 4) as u8, (255 - x * 2) as u8, 255]
            };
            bgra.extend_from_slice(&px);
        }
    }
    Frame::new(w, h, bgra).unwrap()
}

fn bench(c: &mut Criterion) {
    let frame = scene();
    let dominant = ColorParams::default();
    let average = dominant.with_mode(ColorMode::Average);
    c.bench_function("screen_color 96x54 dominante", |b| {
        b.iter(|| {
            screen_color(
                black_box(&frame),
                NormRect::FULL,
                black_box(Some(0.3)),
                &dominant,
            )
        })
    });
    c.bench_function("screen_color 96x54 promedio", |b| {
        b.iter(|| screen_color(black_box(&frame), NormRect::FULL, None, &average))
    });
    c.bench_function("screen_color 96x54 zona izquierda", |b| {
        let left = NormRect::new(0.0, 0.0, 0.5, 1.0);
        b.iter(|| screen_color(black_box(&frame), left, black_box(Some(0.3)), &dominant))
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
