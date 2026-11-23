//! Pruebas portadas de `~/Documents/Open RGB/test_ambilight.py` (clases `ColorDePantalla`,
//! `ObjetivoDelFoco`, `Calibracion`, `Diferencia` y `Suavizador`).
//!
//! Mismos nombres (sin el prefijo `test_`), mismas imágenes y mismas tolerancias. Donde Python
//! usa `assertAlmostEqual` sin `delta` (7 decimales) acá se usa [`CASI`]: los colores del
//! contrato (`Hsv`, `ScreenColor`) son `f32` y no llegan a 7 decimales.

#![allow(missing_docs)]

use color_engine::{Calibration, ColorParams, Smoother, difference, light_target, screen_color};
use openlight_types::{ColorMode, Frame, Hsv, NormRect, ScreenColor};

/// Tolerancia de los `assertAlmostEqual` sin `delta` (ver el comentario del módulo).
const CASI: f64 = 1e-6;

const AZUL: [u8; 3] = [30, 80, 220]; // tono 224°, sat 0,86
const NARANJA: [u8; 3] = [240, 130, 20]; // tono 30°, sat 0,92
const ROJO: [u8; 3] = [220, 30, 30]; // tono 0°, sat 0,86
const VERDE: [u8; 3] = [30, 200, 60]; // tono 131°
const ROJO_PURO: [u8; 3] = [230, 40, 30]; // tono 3°
const AMBAR: [u8; 3] = [200, 120, 40]; // tono 30°, brillo 0,78
const BLANCO: [u8; 3] = [245, 245, 245];
const NEGRO: [u8; 3] = [0, 0, 0];

/// Miniatura BGRA como la que entrega mss, de un color RGB (`imagen` de las pruebas).
struct Imagen {
    ancho: usize,
    alto: usize,
    bgra: Vec<u8>,
}

fn imagen(color: [u8; 3]) -> Imagen {
    let (alto, ancho) = (36, 64);
    let mut img = Imagen {
        ancho,
        alto,
        bgra: vec![0; ancho * alto * 4],
    };
    img.pintar(color, 0..alto, 0..ancho);
    img
}

impl Imagen {
    /// `pintar`: filas y columnas como los `slice` de numpy (rangos semiabiertos).
    fn pintar(
        &mut self,
        color: [u8; 3],
        filas: std::ops::Range<usize>,
        columnas: std::ops::Range<usize>,
    ) -> &mut Self {
        for y in filas {
            for x in columnas.clone() {
                let i = (y * self.ancho + x) * 4;
                self.bgra[i] = color[2];
                self.bgra[i + 1] = color[1];
                self.bgra[i + 2] = color[0];
            }
        }
        self
    }

    fn columnas(&mut self, color: [u8; 3], columnas: std::ops::Range<usize>) -> &mut Self {
        let alto = self.alto;
        self.pintar(color, 0..alto, columnas)
    }

    fn filas(&mut self, color: [u8; 3], filas: std::ops::Range<usize>) -> &mut Self {
        let ancho = self.ancho;
        self.pintar(color, filas, 0..ancho)
    }

    fn frame(&self) -> Frame {
        Frame::new(self.ancho as u32, self.alto as u32, self.bgra.clone()).unwrap()
    }
}

fn separacion(grados_a: f64, grados_b: f64) -> f64 {
    ((grados_a - grados_b + 180.0).rem_euclid(360.0) - 180.0).abs()
}

fn casi(a: f64, b: f64, delta: f64) {
    assert!((a - b).abs() <= delta, "{a} no está a {delta} de {b}");
}

mod color_de_pantalla {
    use super::*;

    fn color(img: &Imagen, modo: ColorMode, tono_previo: Option<f32>) -> ScreenColor {
        let params = ColorParams::default().with_mode(modo);
        screen_color(&img.frame(), NormRect::FULL, tono_previo, &params)
    }

    fn dominante(img: &Imagen) -> ScreenColor {
        color(img, ColorMode::Dominant, None)
    }

    #[track_caller]
    fn assert_tono(h: Option<f32>, grados: f64) {
        let h = h.expect("tiene que tener tono");
        let sep = separacion(f64::from(h) * 360.0, grados);
        assert!(
            sep < 2.0,
            "tono {}°, se esperaba {grados}°",
            f64::from(h) * 360.0
        );
    }

    // port: test_ambilight.py::ColorDePantalla::test_colores_opuestos_no_se_cancelan
    #[test]
    fn colores_opuestos_no_se_cancelan() {
        let mut img = imagen(AZUL);
        img.columnas(NARANJA, 32..64);
        let c = dominante(&img);
        assert_tono(c.h, 30.0);
        casi(f64::from(c.s), 0.917, 0.01);
    }

    // port: test_ambilight.py::ColorDePantalla::test_no_inventa_un_color_que_no_esta_en_pantalla
    #[test]
    fn no_inventa_un_color_que_no_esta_en_pantalla() {
        let mut img = imagen(ROJO_PURO);
        img.columnas(VERDE, 32..64);
        assert_tono(dominante(&img).h, 3.0);
    }

    // port: test_ambilight.py::ColorDePantalla::test_el_modo_promedio_mezcla_todo
    #[test]
    fn el_modo_promedio_mezcla_todo() {
        let mut img = imagen(AZUL);
        img.columnas(NARANJA, 32..64);
        let c = color(&img, ColorMode::Average, None);
        assert_tono(c.h, 359.0);
        casi(f64::from(c.s), 0.266, 0.01);
    }

    // port: test_ambilight.py::ColorDePantalla::test_pagina_blanca_con_foto_chica_queda_palida
    #[test]
    fn pagina_blanca_con_foto_chica_queda_palida() {
        let mut img = imagen(BLANCO);
        img.pintar(ROJO, 14..22, 26..38);
        let c = dominante(&img);
        assert_tono(c.h, 0.0);
        casi(f64::from(c.s), 0.207, 0.02);
    }

    // port: test_ambilight.py::ColorDePantalla::test_color_pastel_conserva_su_tono
    #[test]
    fn color_pastel_conserva_su_tono() {
        let c = dominante(&imagen([230, 220, 200]));
        assert_tono(c.h, 40.0);
        casi(f64::from(c.s), 0.13, 0.01);
    }

    // port: test_ambilight.py::ColorDePantalla::test_negro_no_tiene_tono_ni_luz
    #[test]
    fn negro_no_tiene_tono_ni_luz() {
        assert_eq!(
            dominante(&imagen(NEGRO)),
            ScreenColor {
                h: None,
                s: 0.0,
                luz: 0.0
            }
        );
    }

    // port: test_ambilight.py::ColorDePantalla::test_gris_no_tiene_tono_pero_si_luz
    #[test]
    fn gris_no_tiene_tono_pero_si_luz() {
        let c = dominante(&imagen([120, 120, 120]));
        assert_eq!(c.h, None);
        assert_eq!(c.s, 0.0);
        casi(f64::from(c.luz), 0.471, 0.001);
    }

    // port: test_ambilight.py::ColorDePantalla::test_gris_casi_puro_tampoco_tiene_tono
    #[test]
    fn gris_casi_puro_tampoco_tiene_tono() {
        let c = dominante(&imagen([200, 200, 202]));
        assert_eq!(c.h, None);
        assert_eq!(c.s, 0.0);
    }

    // port: test_ambilight.py::ColorDePantalla::test_bandas_negras_no_bajan_el_brillo
    #[test]
    fn bandas_negras_no_bajan_el_brillo() {
        let mut img = imagen(AMBAR);
        img.filas([10, 10, 10], 0..6).filas([10, 10, 10], 30..36);
        casi(f64::from(dominante(&img).luz), 0.784, 0.001);
    }

    // port: test_ambilight.py::ColorDePantalla::test_bandas_laterales_tampoco
    #[test]
    fn bandas_laterales_tampoco() {
        let mut img = imagen(AMBAR);
        img.columnas(NEGRO, 0..8).columnas(NEGRO, 56..64);
        casi(f64::from(dominante(&img).luz), 0.784, 0.001);
    }

    // port: test_ambilight.py::ColorDePantalla::test_sin_tono_previo_gana_el_color_mas_grande
    #[test]
    fn sin_tono_previo_gana_el_color_mas_grande() {
        let mut img = imagen(ROJO);
        img.columnas([30, 30, 220], 30..64);
        assert_tono(dominante(&img).h, 240.0);
    }

    // port: test_ambilight.py::ColorDePantalla::test_el_tono_previo_evita_saltar_entre_colores_parecidos
    #[test]
    fn el_tono_previo_evita_saltar_entre_colores_parecidos() {
        let mut img = imagen(ROJO);
        img.columnas([30, 30, 220], 30..64);
        assert_tono(color(&img, ColorMode::Dominant, Some(0.0)).h, 0.0);
    }

    // port: test_ambilight.py::ColorDePantalla::test_el_tono_previo_no_retiene_un_color_que_ya_casi_no_esta
    #[test]
    fn el_tono_previo_no_retiene_un_color_que_ya_casi_no_esta() {
        let mut img = imagen(ROJO);
        img.columnas([30, 30, 220], 10..64);
        assert_tono(color(&img, ColorMode::Dominant, Some(0.0)).h, 240.0);
    }
}

mod objetivo_del_foco {
    use super::*;

    /// `AJUSTES = dict(saturacion=1.3, brillo_min=0.05, brillo_max=1.0)`.
    fn ajustes() -> ColorParams {
        ColorParams {
            saturation: 1.3,
            brightness_min: 0.05,
            brightness_max: 1.0,
            ..ColorParams::default()
        }
    }

    fn objetivo(
        h: Option<f32>,
        s: f32,
        luz: f32,
        actual: Option<Hsv>,
        calibracion: Option<Calibration>,
    ) -> Hsv {
        let cal = calibracion.unwrap_or_default();
        light_target(ScreenColor { h, s, luz }, actual, &cal, &ajustes())
    }

    // port: test_ambilight.py::ObjetivoDelFoco::test_color_aplica_realce_y_brillo
    #[test]
    fn color_aplica_realce_y_brillo() {
        let c = objetivo(Some(0.25), 0.5, 1.0, None, None);
        casi(f64::from(c.h), 0.25, CASI);
        casi(f64::from(c.s), 0.65, CASI);
        casi(f64::from(c.v), 1.0, CASI);
    }

    // port: test_ambilight.py::ObjetivoDelFoco::test_color_pasa_por_la_calibracion
    #[test]
    fn color_pasa_por_la_calibracion() {
        let cal =
            Calibration::from_table([(0.0, 0.0, 1.0), (120.0, 100.0, 1.0), (240.0, 240.0, 1.0)]);
        let c = objetivo(Some(120.0 / 360.0), 0.5, 1.0, None, Some(cal));
        casi(f64::from(c.h) * 360.0, 100.0, 1e-4);
    }

    // port: test_ambilight.py::ObjetivoDelFoco::test_el_brillo_crece_con_la_luz_de_la_pantalla
    #[test]
    fn el_brillo_crece_con_la_luz_de_la_pantalla() {
        // 0,05 + 0,95 · (0,4 · 1,5) = 0,62
        casi(
            f64::from(objetivo(Some(0.25), 0.5, 0.4, None, None).v),
            0.62,
            CASI,
        );
    }

    // port: test_ambilight.py::ObjetivoDelFoco::test_pantalla_negra_conserva_el_color_y_lo_atenua
    #[test]
    fn pantalla_negra_conserva_el_color_y_lo_atenua() {
        let actual = Some(Hsv::new(0.3, 0.8, 0.5));
        assert_eq!(
            objetivo(None, 0.0, 0.0, actual, None),
            Hsv::new(0.3, 0.8, 0.05)
        );
    }

    // port: test_ambilight.py::ObjetivoDelFoco::test_pantalla_muy_oscura_tampoco_se_vuelve_blanca
    #[test]
    fn pantalla_muy_oscura_tampoco_se_vuelve_blanca() {
        let c = objetivo(None, 0.0, 0.08, Some(Hsv::new(0.3, 0.8, 0.5)), None);
        assert_eq!((c.h, c.s), (0.3, 0.8));
    }

    // port: test_ambilight.py::ObjetivoDelFoco::test_gris_da_luz_sin_color
    #[test]
    fn gris_da_luz_sin_color() {
        let c = objetivo(None, 0.0, 0.47, Some(Hsv::new(0.3, 0.8, 0.5)), None);
        assert_eq!((c.h, c.s), (0.3, 0.0));
        casi(f64::from(c.v), 0.72, 0.01);
    }

    // port: test_ambilight.py::ObjetivoDelFoco::test_primer_cuadro_negro_sin_color_previo
    #[test]
    fn primer_cuadro_negro_sin_color_previo() {
        assert_eq!(
            objetivo(None, 0.0, 0.0, None, None),
            Hsv::new(0.0, 0.0, 0.05)
        );
    }
}

mod calibracion {
    use super::*;

    /// `TABLA = {0: {"tono": -10, "sat": 1.0}, 120: {"tono": 100, "sat": 0.8}, 240: {"tono": 240, "sat": 1.0}}`.
    fn tabla() -> Calibration {
        Calibration::from_table([(0.0, -10.0, 1.0), (120.0, 100.0, 0.8), (240.0, 240.0, 1.0)])
    }

    // port: test_ambilight.py::Calibracion::test_interpola_tono_y_saturacion
    #[test]
    fn interpola_tono_y_saturacion() {
        let cal = tabla();
        for (pantalla, tono, sat) in [
            (0.0, 350.0, 1.0),
            (60.0, 45.0, 0.9),
            (120.0, 100.0, 0.8),
            (180.0, 170.0, 0.9),
            (300.0, 295.0, 1.0),
        ] {
            let (h, s) = cal.apply(pantalla / 360.0, 1.0);
            assert!(
                (h * 360.0 - tono).abs() <= 0.01,
                "pantalla {pantalla}°: tono {}",
                h * 360.0
            );
            assert!((s - sat).abs() <= 0.01, "pantalla {pantalla}°: sat {s}");
        }
    }

    // port: test_ambilight.py::Calibracion::test_cierra_el_circulo_cerca_de_360
    #[test]
    fn cierra_el_circulo_cerca_de_360() {
        let (h, _) = tabla().apply(350.0 / 360.0, 1.0);
        casi(h * 360.0, 340.83, 0.01);
    }

    // port: test_ambilight.py::Calibracion::test_la_saturacion_no_pasa_de_uno
    #[test]
    fn la_saturacion_no_pasa_de_uno() {
        let cal = Calibration::from_table([(0.0, 0.0, 1.3)]);
        assert_eq!(cal.apply(0.5, 0.9).1, 1.0);
    }

    // port: test_ambilight.py::Calibracion::test_sin_tabla_no_cambia_nada
    #[test]
    fn sin_tabla_no_cambia_nada() {
        let (h, s) = Calibration::identity().apply(0.25, 0.5);
        casi(h, 0.25, 1e-12);
        casi(s, 0.5, 1e-12);
    }
}

mod diferencia {
    use super::*;

    fn dif(a: (f32, f32, f32), b: (f32, f32, f32)) -> f64 {
        difference(Hsv::new(a.0, a.1, a.2), Hsv::new(b.0, b.1, b.2))
    }

    // port: test_ambilight.py::Diferencia::test_tono_opuesto_con_color_pleno_es_el_maximo
    #[test]
    fn tono_opuesto_con_color_pleno_es_el_maximo() {
        casi(dif((0.0, 1.0, 1.0), (0.5, 1.0, 1.0)), 1.0, CASI);
    }

    // port: test_ambilight.py::Diferencia::test_el_tono_no_cuenta_si_no_hay_color
    #[test]
    fn el_tono_no_cuenta_si_no_hay_color() {
        casi(dif((0.0, 0.0, 1.0), (0.5, 0.0, 1.0)), 0.0, CASI);
    }

    // port: test_ambilight.py::Diferencia::test_el_tono_es_circular
    #[test]
    fn el_tono_es_circular() {
        // 350° y 10°: 20° de distancia, no 340°
        casi(
            dif((350.0 / 360.0, 1.0, 1.0), (10.0 / 360.0, 1.0, 1.0)),
            20.0 / 180.0,
            CASI,
        );
    }

    // port: test_ambilight.py::Diferencia::test_cuenta_brillo_y_saturacion
    #[test]
    fn cuenta_brillo_y_saturacion() {
        casi(dif((0.1, 0.5, 0.2), (0.1, 0.5, 0.4)), 0.2, CASI);
        casi(dif((0.1, 0.5, 0.2), (0.1, 0.8, 0.2)), 0.3, CASI);
    }
}

mod suavizador {
    use super::*;

    fn hsv(h: f32, s: f32, v: f32) -> Hsv {
        Hsv::new(h, s, v)
    }

    // port: test_ambilight.py::Suavizador::test_el_primer_cuadro_llega_directo
    #[test]
    fn el_primer_cuadro_llega_directo() {
        assert_eq!(
            Smoother::new(0.3).update(hsv(0.1, 0.5, 0.7), 10.0),
            hsv(0.1, 0.5, 0.7)
        );
    }

    // port: test_ambilight.py::Suavizador::test_no_depende_de_los_cuadros_por_segundo
    #[test]
    fn no_depende_de_los_cuadros_por_segundo() {
        let mut finales = Vec::new();
        for cps in [10_u32, 40] {
            let mut filtro = Smoother::new(0.3);
            filtro.update(hsv(0.0, 0.0, 0.5), 0.0);
            let mut v = 0.0;
            for i in 1..=cps / 2 {
                v = filtro
                    .update(hsv(0.0, 0.0, 0.6), f64::from(i) / f64::from(cps))
                    .v;
            }
            finales.push(f64::from(v));
        }
        casi(finales[0], finales[1], 0.005);
    }

    // port: test_ambilight.py::Suavizador::test_el_tono_toma_el_camino_corto
    #[test]
    fn el_tono_toma_el_camino_corto() {
        let mut filtro = Smoother::new(0.3);
        filtro.update(hsv(350.0 / 360.0, 1.0, 1.0), 0.0);
        let h = filtro.update(hsv(10.0 / 360.0, 1.0, 1.0), 0.05).h;
        assert!(separacion(f64::from(h) * 360.0, 0.0) < 10.1);
    }

    // port: test_ambilight.py::Suavizador::test_un_corte_de_escena_es_mas_rapido_que_un_cambio_chico
    #[test]
    fn un_corte_de_escena_es_mas_rapido_que_un_cambio_chico() {
        fn progreso(desde: f32, hasta: f32) -> f64 {
            let mut filtro = Smoother::new(0.3);
            filtro.update(hsv(0.0, 0.0, desde), 0.0);
            let v = filtro.update(hsv(0.0, 0.0, hasta), 0.05).v;
            f64::from(v - desde) / f64::from(hasta - desde)
        }
        assert!(progreso(0.2, 0.9) > 2.0 * progreso(0.5, 0.52));
    }

    // port: test_ambilight.py::Suavizador::test_cruzar_colores_opuestos_no_pasa_por_blanco
    #[test]
    fn cruzar_colores_opuestos_no_pasa_por_blanco() {
        let mut filtro = Smoother::new(0.3);
        filtro.update(hsv(0.0, 1.0, 1.0), 0.0);
        assert_eq!(filtro.update(hsv(0.5, 1.0, 1.0), 0.05).s, 1.0);
    }

    // port: test_ambilight.py::Suavizador::test_desde_casi_blanco_el_tono_salta_directo
    #[test]
    fn desde_casi_blanco_el_tono_salta_directo() {
        let mut filtro = Smoother::new(0.3);
        filtro.update(hsv(0.0, 0.02, 1.0), 0.0);
        assert_eq!(filtro.update(hsv(0.5, 1.0, 1.0), 0.05).h, 0.5);
    }

    // port: test_ambilight.py::Suavizador::test_sin_suavizado_llega_de_una
    #[test]
    fn sin_suavizado_llega_de_una() {
        let mut filtro = Smoother::new(0.0);
        filtro.update(hsv(0.0, 1.0, 0.2), 0.0);
        let c = filtro.update(hsv(0.4, 0.6, 0.9), 0.05);
        casi(f64::from(c.h), 0.4, CASI);
        casi(f64::from(c.s), 0.6, CASI);
        casi(f64::from(c.v), 0.9, CASI);
    }
}
