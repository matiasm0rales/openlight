use openlight_types::{NormRect, ZonePreset};

use super::*;

pub(crate) const ROJO: (u8, u8, u8) = (220, 30, 30);
pub(crate) const VERDE: (u8, u8, u8) = (30, 200, 60);
pub(crate) const AZUL: (u8, u8, u8) = (30, 80, 220);
pub(crate) const NEGRO: (u8, u8, u8) = (0, 0, 0);

pub(crate) fn display(id: &str) -> DisplayId {
    DisplayId(id.to_owned())
}

/// Cuadro de 64×36 de un solo color (RGB), como `imagen()` de `test_ambilight.py`.
pub(crate) fn solid(color: (u8, u8, u8)) -> Frame {
    halves(color, color)
}

/// Cuadro de 64×36 con la mitad izquierda y la derecha de distinto color.
pub(crate) fn halves(left: (u8, u8, u8), right: (u8, u8, u8)) -> Frame {
    let (w, h) = (64u32, 36u32);
    let mut bgra = Vec::with_capacity((w * h * 4) as usize);
    for _ in 0..h {
        for x in 0..w {
            let (r, g, b) = if x < w / 2 { left } else { right };
            bgra.extend_from_slice(&[b, g, r, 255]);
        }
    }
    Frame::new(w, h, bgra).unwrap()
}

pub(crate) fn frames(id: &str, frame: Frame) -> Frames {
    HashMap::from([(display(id), frame)])
}

fn settings() -> Settings {
    // Como los `correr()` de los tests del script: sin suavizado y sin realce
    Settings {
        smoothing_ms: 0.0,
        saturation: 1.0,
        ..Settings::default()
    }
}

fn pipeline(preset: ZonePreset) -> ColorPipeline {
    ColorPipeline::new(
        vec![Zone::from_preset(display("1"), preset)],
        &settings(),
        Calibration::identity(),
    )
}

fn hue_deg(color: Hsv) -> f32 {
    color.h * 360.0
}

#[test]
fn una_zona_sigue_su_mitad_de_la_pantalla() {
    let f = frames("1", halves(ROJO, AZUL));
    let left = pipeline(ZonePreset::Left).process(0.0, &f).unwrap();
    let right = pipeline(ZonePreset::Right).process(0.0, &f).unwrap();
    assert!(
        hue_deg(left.color).min(360.0 - hue_deg(left.color)) < 2.0,
        "rojo: {left:?}"
    );
    assert!(
        (hue_deg(right.color) - 224.0).abs() < 2.0,
        "azul: {right:?}"
    );
}

#[test]
fn una_zona_es_igual_al_motor_de_color() {
    // Con una sola zona el pipeline no agrega nada propio al script
    let f = frames("1", solid(ROJO));
    let out = pipeline(ZonePreset::Full).process(0.0, &f).unwrap();
    let direct = screen_color(
        &f[&display("1")],
        NormRect::FULL,
        None,
        &ColorParams::from_settings(&settings()),
    );
    assert_eq!(out.screen, direct);
}

#[test]
fn zona_de_un_monitor_sin_cuadro_se_ignora_y_sin_ninguno_no_hay_pantalla() {
    let zones = vec![
        Zone::from_preset(display("1"), ZonePreset::Full),
        Zone::from_preset(display("2"), ZonePreset::Full),
    ];
    let mut p = ColorPipeline::new(zones, &settings(), Calibration::identity());
    // Solo el monitor 2 tiene cuadro: manda su color
    let solo2 = frames("2", solid(VERDE));
    let out = p.process(0.0, &solo2).unwrap();
    assert!((hue_deg(out.color) - 131.0).abs() < 2.0);
    // Ninguno tiene cuadro
    assert!(p.process(1.0, &Frames::new()).is_none());
}

#[test]
fn varias_zonas_de_varios_monitores_se_mezclan() {
    let zones = vec![
        Zone::from_preset(display("1"), ZonePreset::Full),
        Zone::from_preset(display("2"), ZonePreset::Full),
    ];
    let mut p = ColorPipeline::new(zones, &settings(), Calibration::identity());
    let mut both = frames("1", solid(ROJO));
    both.insert(display("2"), solid(VERDE));
    let out = p.process(0.0, &both).unwrap();
    // Rojo (0°) y verde (131°) pesan parecido: el tono queda entre los dos
    let h = hue_deg(out.color);
    assert!(h > 40.0 && h < 100.0, "{h}");
}

#[test]
fn tonos_opuestos_no_dan_un_tono_inventado() {
    let near = |a: f32, b: f32| (a - b).abs() < 1e-3;
    let a = ScreenColor {
        h: Some(0.0),
        s: 0.8,
        luz: 0.5,
    };
    let b = ScreenColor {
        h: Some(0.5),
        s: 0.8,
        luz: 0.5,
    };
    let merged = merge_zones(&[a, b]);
    // Se cancelan: gana una de las dos, no un tono arbitrario
    assert!(
        near(merged.h.unwrap(), 0.0) || near(merged.h.unwrap(), 0.5),
        "{merged:?}"
    );
}

#[test]
fn zonas_sin_color_dan_luz_sin_tono() {
    let grey = ScreenColor {
        h: None,
        s: 0.0,
        luz: 0.4,
    };
    let dark = ScreenColor {
        h: None,
        s: 0.0,
        luz: 0.0,
    };
    let merged = merge_zones(&[grey, dark]);
    assert_eq!(merged.h, None);
    assert!((merged.luz - 0.2).abs() < 1e-6);
}

#[test]
fn una_zona_oscura_pesa_poco_en_el_tono() {
    let bright_red = ScreenColor {
        h: Some(0.0),
        s: 0.9,
        luz: 0.8,
    };
    let dim_blue = ScreenColor {
        h: Some(0.66),
        s: 0.9,
        luz: 0.02,
    };
    let merged = merge_zones(&[bright_red, dim_blue]);
    let h = merged.h.unwrap();
    assert!(h < 0.03 || h > 0.97, "casi rojo: {h}");
}

#[test]
fn los_comandos_salen_calibrados() {
    // port: test_ambilight.py::Bucle::test_los_comandos_salen_calibrados
    let table = [(0.0, 0.0, 1.0), (120.0, 100.0, 1.0), (240.0, 240.0, 1.0)];
    let mut p = ColorPipeline::new(
        vec![Zone::from_preset(display("1"), ZonePreset::Full)],
        &settings(),
        Calibration::from_table(table),
    );
    let out = p.process(0.0, &frames("1", solid((0, 255, 0)))).unwrap();
    assert!(
        (hue_deg(out.color) - 100.0).abs() < 0.5,
        "{}",
        hue_deg(out.color)
    );
}

#[test]
fn pantalla_negra_conserva_el_ultimo_color_atenuado() {
    // port: test_ambilight.py::Bucle::test_pantalla_negra_conserva_el_ultimo_color_atenuado
    let mut p = pipeline(ZonePreset::Full);
    p.process(0.0, &frames("1", solid(ROJO))).unwrap();
    let out = p.process(0.01, &frames("1", solid(NEGRO))).unwrap();
    let h = hue_deg(out.color).min(360.0 - hue_deg(out.color));
    assert!(h < 2.0, "tono {h}");
    assert!((out.color.s - 0.864).abs() < 0.01, "sat {}", out.color.s);
    assert!((out.color.v - 0.05).abs() < 1e-6, "brillo {}", out.color.v);
}
