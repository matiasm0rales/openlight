//! Pruebas golden: el motor reproduce las salidas de `ambilight.py` guardadas en
//! `fixtures/color/*.json` (las genera `tools/golden/gen_color.py`).
//!
//! Tolerancia (criterio de la tarea D): ≤0.5° de tono (circular) y ≤0.005 en s, luz y v.
//! Cada prueba recorre todos los casos y, si falla, lista todos los que no coinciden.

#![allow(missing_docs)]

use std::path::PathBuf;

use base64::Engine;
use color_engine::{
    Calibration, ColorParams, Smoother, difference, hue_separation, light_target, screen_color,
};
use openlight_types::{ColorMode, Frame, Hsv, NormRect, ScreenColor};
use serde_json::Value;

const TOL_TONO_GRADOS: f64 = 0.5;
const TOL: f64 = 0.005;

fn fixture(nombre: &str) -> Value {
    let ruta = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/color")
        .join(format!("{nombre}.json"));
    let texto = std::fs::read_to_string(&ruta)
        .unwrap_or_else(|e| panic!("no pude leer {}: {e}", ruta.display()));
    serde_json::from_str(&texto).expect("fixture con JSON válido")
}

fn num(v: &Value) -> f64 {
    v.as_f64()
        .unwrap_or_else(|| panic!("se esperaba un número: {v}"))
}

fn opt(v: &Value) -> Option<f64> {
    if v.is_null() { None } else { Some(num(v)) }
}

fn trio(v: &Value) -> [f64; 3] {
    let a = v.as_array().expect("arreglo");
    [num(&a[0]), num(&a[1]), num(&a[2])]
}

fn hsv(v: [f64; 3]) -> Hsv {
    Hsv::new(v[0] as f32, v[1] as f32, v[2] as f32)
}

/// Diferencia de tono en grados, circular. Los tonos van en `0..1`.
fn dif_tono(a: f64, b: f64) -> f64 {
    hue_separation(a * 360.0, b * 360.0)
}

/// Junta los errores y falla al final con todos.
#[derive(Default)]
struct Errores(Vec<String>);

impl Errores {
    fn tono(&mut self, caso: &str, obtenido: f64, esperado: f64) {
        let d = dif_tono(obtenido, esperado);
        if d > TOL_TONO_GRADOS {
            self.0
                .push(format!("{caso}: tono {obtenido} vs {esperado} ({d:.3}°)"));
        }
    }

    fn valor(&mut self, caso: &str, que: &str, obtenido: f64, esperado: f64) {
        if (obtenido - esperado).abs() > TOL {
            self.0
                .push(format!("{caso}: {que} {obtenido} vs {esperado}"));
        }
    }

    fn color(&mut self, caso: &str, obtenido: [f64; 3], esperado: [f64; 3]) {
        self.tono(caso, obtenido[0], esperado[0]);
        self.valor(caso, "s", obtenido[1], esperado[1]);
        self.valor(caso, "v", obtenido[2], esperado[2]);
    }

    fn terminar(self, total: usize) {
        assert!(total > 0, "la fixture no tiene casos");
        assert!(
            self.0.is_empty(),
            "{} de {total} comparaciones fuera de tolerancia:\n{}",
            self.0.len(),
            self.0.join("\n")
        );
    }
}

fn a_trio(c: Hsv) -> [f64; 3] {
    [f64::from(c.h), f64::from(c.s), f64::from(c.v)]
}

fn calibraciones(d: &Value) -> std::collections::BTreeMap<String, Calibration> {
    d["calibrations"]
        .as_object()
        .expect("calibrations")
        .iter()
        .map(|(k, v)| {
            (
                k.clone(),
                Calibration::from_json(&v.to_string()).expect("tabla válida"),
            )
        })
        .collect()
}

#[test]
fn golden_color_de_pantalla() {
    let d = fixture("color_de_pantalla");
    // Las constantes del script son los defaults del catálogo
    let k = &d["constants"];
    let base = ColorParams::default();
    assert_eq!(
        f64::from(base.black_bar),
        num(&k["BANDA_NEGRA"]) as f32 as f64
    );
    assert_eq!(base.grey_threshold, num(&k["SIN_COLOR"]));
    assert_eq!(base.dark_screen, num(&k["PANTALLA_OSCURA"]));
    assert_eq!(base.hue_stickiness, num(&k["VENTAJA_TONO_PREVIO"]));
    assert_eq!(Smoother::BIG_CHANGE, num(&k["CAMBIO_GRANDE"]));

    let mut errores = Errores::default();
    let mut total = 0;
    for caso in d["cases"].as_array().expect("cases") {
        let nombre = caso["name"].as_str().unwrap();
        let bgra = base64::engine::general_purpose::STANDARD
            .decode(caso["bgra_b64"].as_str().unwrap())
            .expect("base64");
        let frame = Frame::new(
            caso["width"].as_u64().unwrap() as u32,
            caso["height"].as_u64().unwrap() as u32,
            bgra,
        )
        .expect("tamaño de la imagen");
        for r in caso["results"].as_array().unwrap() {
            total += 1;
            let modo = ColorMode::parse(r["mode"].as_str().unwrap()).expect("modo");
            let previo = opt(&r["tono_previo"]);
            let etiqueta = format!("{nombre} {modo:?} previo={previo:?}");
            let c: ScreenColor = screen_color(
                &frame,
                NormRect::FULL,
                previo.map(|p| p as f32),
                &base.with_mode(modo),
            );
            match (c.h, opt(&r["h"])) {
                (Some(h), Some(esperado)) => errores.tono(&etiqueta, f64::from(h), esperado),
                (None, None) => {}
                (h, esperado) => errores
                    .0
                    .push(format!("{etiqueta}: h {h:?} vs {esperado:?}")),
            }
            errores.valor(&etiqueta, "s", f64::from(c.s), num(&r["s"]));
            errores.valor(&etiqueta, "luz", f64::from(c.luz), num(&r["luz"]));
        }
    }
    errores.terminar(total);
}

#[test]
fn golden_objetivo_del_foco() {
    let d = fixture("objetivo_del_foco");
    let cals = calibraciones(&d);
    let mut errores = Errores::default();
    let casos = d["cases"].as_array().unwrap();
    for (i, c) in casos.iter().enumerate() {
        let params = ColorParams {
            saturation: num(&c["saturacion"]),
            brightness_min: num(&c["brillo_min"]),
            brightness_max: num(&c["brillo_max"]),
            ..ColorParams::default()
        };
        let pantalla = ScreenColor {
            h: opt(&c["h"]).map(|h| h as f32),
            s: num(&c["s"]) as f32,
            luz: num(&c["luz"]) as f32,
        };
        let actual = (!c["actual"].is_null()).then(|| hsv(trio(&c["actual"])));
        let cal = &cals[c["calibration"].as_str().unwrap()];
        let r = light_target(pantalla, actual, cal, &params);
        errores.color(&format!("caso {i}"), a_trio(r), trio(&c["result"]));
    }
    errores.terminar(casos.len());
}

#[test]
fn golden_calibracion() {
    let d = fixture("calibracion");
    let cals = calibraciones(&d);
    let mut errores = Errores::default();
    let casos = d["cases"].as_array().unwrap();
    for c in casos {
        let cal = &cals[c["calibration"].as_str().unwrap()];
        let (h, s) = (num(&c["h"]), num(&c["s"]));
        let (rh, rs) = cal.apply(h, s);
        let esperado = c["result"].as_array().unwrap();
        let etiqueta = format!("{} h={h} s={s}", c["calibration"]);
        errores.tono(&etiqueta, rh, num(&esperado[0]));
        errores.valor(&etiqueta, "s", rs, num(&esperado[1]));
    }
    errores.terminar(casos.len());
}

#[test]
fn golden_diferencia_y_separacion_tonos() {
    let d = fixture("diferencia");
    let mut errores = Errores::default();
    let casos = d["cases"].as_array().unwrap();
    for (i, c) in casos.iter().enumerate() {
        let r = difference(hsv(trio(&c["a"])), hsv(trio(&c["b"])));
        errores.valor(
            &format!("diferencia {i}"),
            "resultado",
            r,
            num(&c["result"]),
        );
    }
    let sep = d["separacion_tonos"].as_array().unwrap();
    for (i, c) in sep.iter().enumerate() {
        let r = hue_separation(num(&c["a"]), num(&c["b"]));
        // Está en grados: la tolerancia es la del tono
        if (r - num(&c["result"])).abs() > TOL_TONO_GRADOS {
            errores
                .0
                .push(format!("separacion {i}: {r} vs {}", c["result"]));
        }
    }
    errores.terminar(casos.len() + sep.len());
}

#[test]
fn golden_suavizador() {
    let d = fixture("suavizador");
    assert_eq!(Smoother::BIG_CHANGE, num(&d["cambio_grande"]));
    let mut errores = Errores::default();
    let mut total = 0;
    for sec in d["sequences"].as_array().unwrap() {
        let lento = num(&sec["lento_s"]);
        let mut filtro = Smoother::new(lento);
        for (i, paso) in sec["steps"].as_array().unwrap().iter().enumerate() {
            total += 1;
            let r = filtro.update(hsv(trio(&paso["objetivo"])), num(&paso["ahora"]));
            let etiqueta = format!("{} lento={lento} paso {i}", sec["name"]);
            errores.color(&etiqueta, a_trio(r), trio(&paso["result"]));
        }
    }
    errores.terminar(total);
}
