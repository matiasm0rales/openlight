//! Estado original del foco: qué se guarda para devolverlo y qué se persiste.
//!
//! Port de `elegir_original()` y de `leer_estado()/guardar_estado()` de `ambilight.py`. Guardar
//! en disco es cosa de `openlight-core` (`state.json`): acá solo está el contrato
//! ([`StateStore`]) y una implementación en memoria para pruebas.

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::sync::{Mutex, MutexGuard};

use openlight_types::{Capabilities, DeviceId, Dps, ProtocolVersion};
use serde_json::Value;

/// Lo que se recuerda de un foco entre ejecuciones (`foco_estado.json` del script).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SavedState {
    /// Última IP que funcionó.
    pub ip: Option<IpAddr>,
    /// Versión del protocolo con la que funcionó cuando difería de la configurada (la que
    /// anunció el foco por discovery). `None` = vale la configurada.
    pub version: Option<ProtocolVersion>,
    /// Estado al que volver. Solo está mientras no se haya restaurado: si queda guardado, el
    /// programa murió sin devolver el foco.
    pub original: Option<Dps>,
}

/// Dónde se guarda el [`SavedState`] de cada foco.
///
/// Las llamadas son sincrónicas y rápidas (un JSON chico): el actor las hace en línea.
pub trait StateStore: Send + Sync {
    /// Lo guardado para `id` (vacío si no hay nada).
    fn load(&self, id: &DeviceId) -> SavedState;
    /// Reemplaza lo guardado para `id`.
    fn save(&self, id: &DeviceId, state: &SavedState);
}

/// [`StateStore`] en memoria.
#[derive(Debug, Default)]
pub struct MemoryStore(Mutex<BTreeMap<DeviceId, SavedState>>);

impl MemoryStore {
    /// Almacén vacío.
    pub fn new() -> Self {
        Self::default()
    }

    fn map(&self) -> MutexGuard<'_, BTreeMap<DeviceId, SavedState>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl StateStore for MemoryStore {
    fn load(&self, id: &DeviceId) -> SavedState {
        self.map().get(id).cloned().unwrap_or_default()
    }

    fn save(&self, id: &DeviceId, state: &SavedState) {
        self.map().insert(id.clone(), state.clone());
    }
}

/// Los DPs que se devuelven al salir (`DPS_A_RESTAURAR`): encendido, modo, color, brillo y
/// temperatura. El DP de control no se restaura: solo sirve para mandar colores.
pub fn restorable_dps(caps: &Capabilities) -> Vec<&str> {
    let mut dps = vec![caps.dp_switch.as_str()];
    dps.extend(caps.dp_mode.as_deref());
    dps.extend(caps.dp_colour.as_ref().map(|(dp, _)| dp.as_str()));
    dps.extend(caps.dp_bright.as_ref().map(|r| r.dp.as_str()));
    dps.extend(caps.dp_temp.as_ref().map(|r| r.dp.as_str()));
    dps
}

/// Estado al que volver al salir (`elegir_original`).
///
/// Si una ejecución anterior murió sin restaurar, el foco quedó en modo color y vale lo que se
/// guardó entonces. Si no, vale lo que tiene ahora, solo con los DPs que se restauran (sin
/// datos ajenos como escenas o temporizadores).
pub fn choose_original(caps: &Capabilities, current: &Dps, saved: Option<&Dps>) -> Dps {
    let in_colour = caps
        .dp_mode
        .as_ref()
        .and_then(|mode| current.get(mode))
        .is_some_and(|v| v == &Value::from("colour"));
    match saved {
        Some(saved) if in_colour && !saved.is_empty() => saved.clone(),
        _ => restorable_dps(caps)
            .into_iter()
            .filter_map(|dp| current.get(dp).map(|v| (dp.to_owned(), v.clone())))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::CapabilitiesExt;

    fn caps() -> Capabilities {
        let m = serde_json::from_str(include_str!("../../tuya-sim/profiles/v2.json")).unwrap();
        Capabilities::from_mapping(&m).unwrap()
    }

    fn dps(value: serde_json::Value) -> Dps {
        serde_json::from_value(value).unwrap()
    }

    /// `ESTADO_FOCO` de `test_ambilight.py`: el foco en modo color, más datos ajenos.
    fn estado_foco() -> Dps {
        dps(
            json!({"20": true, "21": "colour", "22": 1000, "23": 1000, "24": "007c013c02d8",
                   "25": "000e0d0000000000000000c80000", "26": 0, "34": false}),
        )
    }

    /// `ESTADO_BLANCO`: lo que había antes de arrancar.
    fn estado_blanco() -> Dps {
        dps(json!({"20": true, "21": "white", "22": 500, "23": 100, "24": "000000000000"}))
    }

    // port: test_ambilight.py::ElegirOriginal::test_sin_nada_guardado_usa_lo_actual_sin_datos_ajenos
    #[test]
    fn sin_nada_guardado_usa_lo_actual_sin_datos_ajenos() {
        assert_eq!(
            choose_original(&caps(), &estado_foco(), None),
            dps(json!({"20": true, "21": "colour", "24": "007c013c02d8", "22": 1000, "23": 1000}))
        );
    }

    // port: test_ambilight.py::ElegirOriginal::test_tras_un_cierre_sucio_vale_lo_guardado
    #[test]
    fn tras_un_cierre_sucio_vale_lo_guardado() {
        assert_eq!(
            choose_original(&caps(), &estado_foco(), Some(&estado_blanco())),
            estado_blanco()
        );
    }

    // port: test_ambilight.py::ElegirOriginal::test_si_el_usuario_cambio_el_foco_vale_lo_actual
    #[test]
    fn si_el_usuario_cambio_el_foco_vale_lo_actual() {
        let actual =
            dps(json!({"20": true, "21": "white", "22": 500, "23": 100, "24": "000000000000"}));
        assert_eq!(
            choose_original(&caps(), &actual, Some(&estado_blanco())),
            actual
        );
    }

    #[test]
    fn un_guardado_vacio_no_cuenta() {
        // `if guardado and ...`: un dict vacío es falso en Python.
        assert_eq!(
            choose_original(&caps(), &estado_foco(), Some(&Dps::new())).len(),
            5
        );
    }

    #[test]
    fn foco_viejo_restaura_sus_dps() {
        let m = serde_json::from_str(include_str!("../../tuya-sim/profiles/v1.json")).unwrap();
        let caps = Capabilities::from_mapping(&m).unwrap();
        let actual = dps(
            json!({"1": true, "2": "white", "3": 255, "4": 255, "5": "ff00000000ffff", "9": 1}),
        );
        let original = choose_original(&caps, &actual, None);
        assert_eq!(original.len(), 5, "sin el DP 9, que no es de la luz");
        assert!(!original.contains_key("9"));
    }

    #[test]
    fn almacen_en_memoria() {
        let store = MemoryStore::new();
        let id = DeviceId("foco".into());
        assert_eq!(store.load(&id), SavedState::default());
        let st = SavedState {
            ip: Some("192.0.2.9".parse().unwrap()),
            version: Some(ProtocolVersion::V35),
            original: Some(estado_blanco()),
        };
        store.save(&id, &st);
        assert_eq!(store.load(&id), st);
        assert_eq!(store.load(&DeviceId("otro".into())), SavedState::default());
    }
}
