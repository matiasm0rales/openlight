//! 24 horas simuladas con el reloj de tokio pausado: cortes periódicos del foco, focos
//! apagados, focos mudos y una clave rechazada, mientras llegan colores sin parar.
//!
//! Criterio de aceptación de la tarea F: sin fugas (sockets ni tareas) y sin bloqueos (el actor
//! siempre contesta y siempre recupera la conexión).
#![allow(missing_docs)]

mod common;

use std::time::Duration;

use common::*;
use openlight_types::{DeviceSessionState, DisconnectReason, Hsv, ProtocolVersion};
use serde_json::json;
use tuya_sim::Faults;

const HOUR: u64 = 3600;
const DAY: u64 = 24 * HOUR;

/// Qué le pasa al foco y cuándo (segundos desde el arranque).
#[derive(Clone, Copy, Debug)]
enum Event {
    /// El foco no deja entrar a nadie (apagado del interruptor, o la app Smart Life lo ocupa).
    Occupied(bool),
    /// El foco lee y no contesta nada.
    Mute(bool),
    /// El foco tiene otra clave.
    RejectKey(bool),
}

fn timeline() -> Vec<(u64, Event)> {
    let mut events = Vec::new();
    // Cada 3 horas, el foco está apagado 2 minutos.
    for k in 1..=7 {
        events.push((k * 3 * HOUR, Event::Occupied(true)));
        events.push((k * 3 * HOUR + 120, Event::Occupied(false)));
    }
    // Cada 5 horas y 17 minutos, se cuelga 90 s.
    for k in 1..=4 {
        let t = k * 5 * HOUR + 17 * 60;
        events.push((t, Event::Mute(true)));
        events.push((t + 90, Event::Mute(false)));
    }
    // A las 20 horas, 5 minutos con otra clave.
    events.push((20 * HOUR + 7, Event::RejectKey(true)));
    events.push((20 * HOUR + 307, Event::RejectKey(false)));
    events.sort_by_key(|(t, _)| *t);
    events
}

#[test]
fn veinticuatro_horas_sin_fugas_ni_bloqueos() {
    run(async {
        // El foco corta cada conexión a los 10 minutos, como los 272 cortes del log.
        let sim = sim_with(
            ProtocolVersion::V35,
            Faults {
                drop_every: Some(Duration::from_secs(600)),
                ..Faults::default()
            },
        )
        .await;
        let rig = Rig::with(sim, |c| c);
        let caps = rig.start().await.capabilities;
        rig.activate().await;
        let tasks_at_start = tokio::runtime::Handle::current()
            .metrics()
            .num_alive_tasks();

        let events = timeline();
        let mut next_event = 0;
        let mut down_since: Option<u64> = None;
        let mut longest_down = 0;
        let mut max_sockets = 0;
        let mut probes = 0;
        let mut last_colour = Hsv::new(0.0, 1.0, 1.0);

        for second in 0..DAY {
            while next_event < events.len() && events[next_event].0 == second {
                match events[next_event].1 {
                    Event::Occupied(on) => {
                        rig.sim.update_faults(|f| f.occupied = on);
                        if on {
                            rig.sim.disconnect_all();
                        }
                    }
                    Event::Mute(on) => rig.sim.update_faults(|f| f.mute = on),
                    Event::RejectKey(on) => rig.sim.update_faults(|f| f.reject_key = on),
                }
                next_event += 1;
            }

            // Un color por segundo, como un video.
            last_colour = Hsv::new((second % 360) as f32 / 360.0, 1.0, 1.0);
            rig.device.send_colour(last_colour);
            tokio::time::advance(Duration::from_secs(1)).await;
            // 86.400 vueltas: con pocas cesiones alcanza para un viaje de ida y vuelta.
            settle_n(8).await;

            // Cada minuto: ¿cuánto lleva caído? ¿hay sockets de más?
            if second % 60 == 0 {
                max_sockets = max_sockets.max(rig.sim.active_connections());
                if rig.device.health().connected {
                    if let Some(since) = down_since.take() {
                        longest_down = longest_down.max(second - since);
                    }
                } else {
                    down_since.get_or_insert(second);
                }
            }

            // Cada 10 minutos, el actor tiene que contestar (conectado o no) sin colgarse.
            if second % 600 == 300 {
                probes += 1;
                let answer = drive(tokio::time::timeout(
                    Duration::from_secs(30),
                    rig.device.status(),
                ))
                .await;
                assert!(answer.is_ok(), "el actor se colgó en el segundo {second}");
            }
        }

        // Cada corte se recuperó solo, y rápido: el peor (clave rechazada 5 min) dura menos que 6.
        pump_slow(Duration::from_secs(120)).await;
        let health = rig.device.health();
        assert!(health.connected, "{health:?}");
        assert!(
            longest_down <= 6 * 60,
            "estuvo {longest_down} s sin conexión de corrido"
        );
        assert_eq!(rig.device.snapshot().state, DeviceSessionState::Active);

        // Sin fugas: nunca más de un socket, y las tareas vivas son las mismas.
        assert!(
            max_sockets <= 1,
            "llegó a haber {max_sockets} sockets abiertos"
        );
        let tasks_at_end = tokio::runtime::Handle::current()
            .metrics()
            .num_alive_tasks();
        assert!(
            tasks_at_end <= tasks_at_start + 1,
            "tareas vivas: {tasks_at_start} al empezar, {tasks_at_end} al terminar"
        );
        // La cuenta de cortes de la última hora no crece sin límite (se purga).
        assert!(health.disconnects_last_hour <= 12, "{health:?}");
        // Pero el total de reconexiones sí refleja las 24 horas: 144 cortes de 10 minutos más
        // los otros.
        assert!(
            health.reconnects >= 144,
            "reconexiones: {}",
            health.reconnects
        );
        assert!(probes >= 140);
        assert!(rig.sim.stats().heartbeats() > 5000, "{:?}", rig.sim.stats());
        assert!(
            matches!(
                health.last_disconnect,
                Some(
                    DisconnectReason::PeerClosed
                        | DisconnectReason::KeyRejected
                        | DisconnectReason::NoResponse
                )
            ),
            "{:?}",
            health.last_disconnect
        );

        eprintln!(
            "24 h: {} reconexiones, {} cortes en la última hora, caída más larga {} s, \
             máximo {} socket(s), {} → {} tareas, {:?}",
            health.reconnects,
            health.disconnects_last_hour,
            longest_down,
            max_sockets,
            tasks_at_start,
            tasks_at_end,
            rig.sim.stats(),
        );

        // El último color llegó (sin esperar un corte más) y se puede restaurar.
        rig.device.send_colour(last_colour);
        pump(Duration::from_secs(2)).await;
        let esperado = {
            let h = (last_colour.h * 360.0).round() as u32;
            format!("{h:04x}03e803e8")
        };
        assert_eq!(rig.sim.dp("24"), Some(json!(esperado)));
        assert_eq!(caps.dp_colour.as_ref().map(|c| c.0.as_str()), Some("24"));
        drive(rig.device.restore())
            .await
            .expect("restaura al final");
        pump(Duration::from_secs(1)).await;
        assert_eq!(rig.sim.active_connections(), 0);
    });
}
