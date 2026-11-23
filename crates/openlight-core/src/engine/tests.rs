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
