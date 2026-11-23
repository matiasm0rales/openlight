use color_engine::Calibration;
use openlight_types::{Settings, Zone, ZonePreset};

use super::*;
use crate::pipeline::tests::{AZUL, NEGRO, ROJO, VERDE, display, frames, solid};

pub(crate) const CONNECTED: Link = Link {
    connected: true,
    reconnects: 0,
};

/// Sin suavizado ni realce y con los tiempos pedidos, como los `correr()` del script.
pub(crate) fn engine(timing: Timing) -> DeviceEngine {
    let settings = Settings {
        smoothing_ms: 0.0,
        saturation: 1.0,
        ..Settings::default()
    };
    let pipeline = ColorPipeline::new(
        vec![Zone::from_preset(display("1"), ZonePreset::Full)],
        &settings,
        Calibration::identity(),
    );
    DeviceEngine::new(pipeline, timing)
}

/// Sin límite de envíos ni de cuadros (`fps=100000, envios=100000`).
pub(crate) fn fast() -> Timing {
    Timing {
        frame_period_s: 1e-5,
        send_period_s: 1e-5,
        send_threshold: 0.01,
        no_screen_wait_s: 0.0,
    }
}

pub(crate) fn sends(steps: &[Step]) -> Vec<Hsv> {
    steps
        .iter()
        .flat_map(|s| &s.actions)
        .filter_map(|a| match a {
            Action::Send(c) => Some(*c),
            _ => None,
        })
        .collect()
}

#[test]
fn arrancar_prepara_el_foco() {
    let mut e = engine(fast());
    assert_eq!(e.start(CONNECTED), vec![Action::Prepare]);
}

#[test]
fn no_repite_un_color_que_no_cambio() {
    // port: test_ambilight.py::Bucle::test_no_repite_un_color_que_no_cambio
    let mut e = engine(fast());
    let steps: Vec<_> = (0..5)
        .map(|i| e.tick(f64::from(i), &frames("1", solid(ROJO)), CONNECTED))
        .collect();
    assert_eq!(sends(&steps).len(), 1);
}

#[test]
fn respeta_el_limite_de_envios() {
    // port: test_ambilight.py::Bucle::test_respeta_el_limite_de_envios
    let mut e = engine(Timing {
        send_period_s: 1.0,
        ..fast()
    });
    let steps = [
        e.tick(0.00, &frames("1", solid(ROJO)), CONNECTED),
        e.tick(0.01, &frames("1", solid(AZUL)), CONNECTED),
        e.tick(0.02, &frames("1", solid(VERDE)), CONNECTED),
    ];
    assert_eq!(sends(&steps).len(), 1);
}

#[test]
fn pasado_el_periodo_vuelve_a_enviar_si_cambio() {
    let mut e = engine(Timing {
        send_period_s: 0.125, // 8 envíos por segundo
        ..fast()
    });
    let steps = [
        e.tick(0.00, &frames("1", solid(ROJO)), CONNECTED),
        e.tick(0.10, &frames("1", solid(AZUL)), CONNECTED), // muy pronto
        e.tick(0.13, &frames("1", solid(AZUL)), CONNECTED), // ya toca
    ];
    assert_eq!(sends(&steps).len(), 2);
}

#[test]
fn un_cambio_menor_al_umbral_no_se_envia() {
    let mut e = engine(Timing {
        send_threshold: 0.5,
        ..fast()
    });
    let steps = [
        e.tick(0.0, &frames("1", solid(ROJO)), CONNECTED),
        // Rojo a naranja: cambio de tono chico
        e.tick(1.0, &frames("1", solid((220, 80, 30))), CONNECTED),
    ];
    assert_eq!(sends(&steps).len(), 1);
}

#[test]
fn tras_reconectar_reenvia_el_color_aunque_no_cambie() {
    // port: test_ambilight.py::Bucle::test_tras_reconectar_reenvia_el_color_aunque_no_cambie
    let mut e = engine(fast());
    let reconnected = Link {
        connected: true,
        reconnects: 1,
    };
    let steps = [
        e.tick(0.0, &frames("1", solid(ROJO)), CONNECTED),
        e.tick(1.0, &frames("1", solid(ROJO)), CONNECTED),
        e.tick(2.0, &frames("1", solid(ROJO)), reconnected),
        e.tick(3.0, &frames("1", solid(ROJO)), reconnected),
    ];
    assert_eq!(sends(&steps).len(), 2);
}

#[test]
fn sin_conexion_el_color_queda_pendiente() {
    // port: test_ambilight.py::Bucle::test_sin_conexion_el_color_queda_pendiente
    let mut e = engine(fast());
    let down = Link {
        connected: false,
        reconnects: 0,
    };
    let steps = [
        e.tick(0.0, &frames("1", solid(ROJO)), down),
        e.tick(1.0, &frames("1", solid(ROJO)), down),
        e.tick(2.0, &frames("1", solid(ROJO)), CONNECTED),
        e.tick(3.0, &frames("1", solid(ROJO)), CONNECTED),
    ];
    assert_eq!(sends(&steps).len(), 1);
    assert!(steps[0].actions.is_empty() && steps[1].actions.is_empty());
}

#[test]
fn los_pasos_informan_el_color_de_pantalla() {
    let mut e = engine(fast());
    let step = e.tick(0.0, &frames("1", solid(NEGRO)), CONNECTED);
    let processed = step.processed.unwrap();
    assert_eq!(processed.screen.h, None);
    assert_eq!(processed.screen.luz, 0.0);
}

fn steps_with(e: &mut DeviceEngine, screens: &[Option<Frames>]) -> Vec<Step> {
    screens
        .iter()
        .enumerate()
        .map(|(i, f)| e.tick(i as f64, f.as_ref().unwrap_or(&Frames::new()), CONNECTED))
        .collect()
}

fn actions(e: &mut DeviceEngine, screens: &[Option<Frames>]) -> Vec<Action> {
    let mut all = e.start(CONNECTED);
    all.extend(steps_with(e, screens).into_iter().flat_map(|s| s.actions));
    all
}

fn red() -> Option<Frames> {
    Some(frames("1", solid(ROJO)))
}

#[test]
fn sin_pantalla_devuelve_el_estado_y_al_volver_retoma() {
    // port: test_ambilight.py::Bucle::test_sin_pantalla_devuelve_el_estado_y_al_volver_retoma
    let mut e = engine(fast());
    let all = actions(&mut e, &[None, None, red()]);
    assert_eq!(
        all[..3],
        [Action::Prepare, Action::Restore, Action::Prepare]
    );
    assert_eq!(
        all.iter().filter(|a| matches!(a, Action::Send(_))).count(),
        1
    );
}

#[test]
fn sin_pantalla_devuelve_el_estado_una_sola_vez() {
    // port: test_ambilight.py::Bucle::test_sin_pantalla_devuelve_el_estado_una_sola_vez
    let mut e = engine(fast());
    let all = actions(&mut e, &[None, None, None, red()]);
    assert_eq!(all.iter().filter(|a| **a == Action::Restore).count(), 1);
}

#[test]
fn espera_no_screen_wait_antes_de_devolver_el_foco() {
    let mut e = engine(Timing {
        no_screen_wait_s: 20.0,
        ..fast()
    });
    e.start(CONNECTED);
    let none = Frames::new();
    assert!(e.tick(100.0, &none, CONNECTED).actions.is_empty());
    assert!(
        e.tick(119.9, &none, CONNECTED).actions.is_empty(),
        "todavía no pasaron 20 s"
    );
    assert_eq!(
        e.tick(120.0, &none, CONNECTED).actions,
        vec![Action::Restore]
    );
    assert!(e.is_restored());
}

#[test]
fn si_la_pantalla_vuelve_antes_el_temporizador_se_reinicia() {
    let mut e = engine(Timing {
        no_screen_wait_s: 20.0,
        ..fast()
    });
    e.start(CONNECTED);
    let none = Frames::new();
    e.tick(0.0, &none, CONNECTED);
    e.tick(15.0, &frames("1", solid(ROJO)), CONNECTED);
    // Otra falta de pantalla cuenta desde cero
    assert!(e.tick(16.0, &none, CONNECTED).actions.is_empty());
    assert!(e.tick(35.0, &none, CONNECTED).actions.is_empty());
    assert_eq!(
        e.tick(36.0, &none, CONNECTED).actions,
        vec![Action::Restore]
    );
}

#[test]
fn al_retomar_se_reenvia_el_color_aunque_sea_el_mismo() {
    let mut e = engine(fast());
    let all = actions(&mut e, &[red(), None, red()]);
    assert_eq!(
        all,
        [
            Action::Prepare,
            all[1], // primer color
            Action::Restore,
            Action::Prepare,
            all[4], // el mismo color, reenviado
        ]
    );
    assert!(matches!(all[1], Action::Send(_)) && all[1] == all[4]);
}

#[test]
fn suspender_restaura_y_congela_hasta_despertar() {
    let mut e = engine(fast());
    e.start(CONNECTED);
    e.tick(0.0, &frames("1", solid(ROJO)), CONNECTED);

    assert_eq!(e.suspend(), vec![Action::Restore]);
    // Dormido: aunque haya pantalla no manda nada
    let step = e.tick(1.0, &frames("1", solid(AZUL)), CONNECTED);
    assert!(step.actions.is_empty() && step.processed.is_none());
    // Suspender dos veces no restaura dos veces
    assert!(e.suspend().is_empty());

    e.resume();
    let step = e.tick(2.0, &frames("1", solid(AZUL)), CONNECTED);
    assert!(matches!(
        step.actions[..],
        [Action::Prepare, Action::Send(_)]
    ));
}

#[test]
fn despertar_sin_pantalla_espera_antes_de_prepararse() {
    let mut e = engine(Timing {
        no_screen_wait_s: 20.0,
        ..fast()
    });
    e.start(CONNECTED);
    e.suspend();
    e.resume();
    // Sin monitor todavía: no hace nada (ya está restaurado)
    assert!(e.tick(5.0, &Frames::new(), CONNECTED).actions.is_empty());
    assert!(e.tick(60.0, &Frames::new(), CONNECTED).actions.is_empty());
}

#[test]
fn detener_restaura_solo_si_hace_falta() {
    let mut e = engine(fast());
    e.start(CONNECTED);
    assert_eq!(e.stop(), vec![Action::Restore]);
    assert!(e.stop().is_empty());

    // Ya devuelto por falta de pantalla: no se restaura otra vez
    let mut e = engine(fast());
    e.start(CONNECTED);
    e.tick(0.0, &Frames::new(), CONNECTED);
    assert!(e.stop().is_empty());
}

#[test]
fn un_envio_fallido_queda_pendiente() {
    let mut e = engine(fast());
    e.start(CONNECTED);
    let step = e.tick(0.0, &frames("1", solid(ROJO)), CONNECTED);
    assert_eq!(sends(&[step]).len(), 1);
    e.send_failed();
    assert_eq!(e.sent(), None);
    let step = e.tick(1.0, &frames("1", solid(ROJO)), CONNECTED);
    assert_eq!(
        sends(&[step]).len(),
        1,
        "se reintenta aunque el color no cambió"
    );
}

#[test]
fn una_preparacion_fallida_se_repite_con_el_proximo_cuadro() {
    let mut e = engine(fast());
    e.start(CONNECTED);
    e.prepare_failed();
    let step = e.tick(0.0, &frames("1", solid(ROJO)), CONNECTED);
    assert!(matches!(
        step.actions[..],
        [Action::Prepare, Action::Send(_)]
    ));
}

#[test]
fn una_restauracion_fallida_queda_pendiente() {
    let mut e = engine(fast());
    e.start(CONNECTED);
    assert_eq!(e.stop(), vec![Action::Restore]);
    e.restore_failed();
    assert!(!e.is_restored());
    assert_eq!(e.stop(), vec![Action::Restore], "se vuelve a pedir");
    assert!(e.stop().is_empty(), "y una vez lograda no se repite");
}

#[test]
fn si_falla_la_restauracion_sin_pantalla_se_reintenta_en_el_paso_siguiente() {
    let mut e = engine(fast());
    e.start(CONNECTED);
    let none = Frames::new();
    assert_eq!(e.tick(0.0, &none, CONNECTED).actions, vec![Action::Restore]);
    e.restore_failed();
    assert_eq!(e.tick(1.0, &none, CONNECTED).actions, vec![Action::Restore]);
    assert!(e.tick(2.0, &none, CONNECTED).actions.is_empty());
}
