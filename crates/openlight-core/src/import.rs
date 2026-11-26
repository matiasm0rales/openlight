//! Importar el setup del script Python (`~/Documents/Open RGB`).
//!
//! Lee de una carpeta, **sin escribir nunca en ella**:
//!
//! | archivo | qué aporta |
//! |---|---|
//! | `devices.json` | los focos (lo genera `tinytuya wizard`): id, nombre, clave, mapeo de DPs |
//! | `config.json` | el foco que usaba el script: `device_id`, `ip`, `version`, `local_key` |
//! | `calibracion.json` | la tabla de calibración de ese foco |
//! | `foco_estado.json` | última IP y estado original (opcional) |
//!
//! Las claves van al llavero; en `devices.json` de OpenLight no queda ninguna. Las que no son
//! luces (enchufes, sensores) se omiten. Volver a importar es seguro: actualiza lo que ya está
//! y conserva las zonas y ajustes que el usuario haya puesto.

use std::collections::BTreeMap;
use std::fs;
use std::net::IpAddr;
use std::path::Path;

use openlight_types::ipc::{CalibrationPoint, ImportReport};
use openlight_types::{
    DeviceId, DisplayId, DpMapping, Dps, ProtocolVersion, Secret, SettingsOverride, Zone,
    ZonePreset, is_light_category,
};
use secrets::{SecretKey, SecretStore};
use serde::Deserialize;
use serde_json::Value;

use crate::error::{CoreError, Result};
use crate::store::{DeviceRecord, Store, next_order};

/// Faltaba la versión del protocolo y se asumió 3.3.
pub const WARN_VERSION_ASSUMED: &str = "import.warning.version_assumed";
/// Un foco sin clave local: hay que renovarla desde la nube.
pub const WARN_NO_KEY: &str = "import.warning.no_key";
/// El mapeo de DPs de un foco no se pudo leer.
pub const WARN_MAPPING_INVALID: &str = "import.warning.mapping_invalid";
/// Hay calibración pero no se sabe de qué foco es.
pub const WARN_CALIBRATION_UNASSIGNED: &str = "import.warning.calibration_unassigned";
/// `calibracion.json` no se pudo leer y se ignoró.
pub const WARN_CALIBRATION_INVALID: &str = "import.warning.calibration_invalid";
/// Se omitieron dispositivos que no son luces.
pub const WARN_SKIPPED_NON_LIGHT: &str = "import.warning.skipped_non_light";
/// Una IP del setup viejo no es válida y se ignoró.
pub const WARN_IP_INVALID: &str = "import.warning.ip_invalid";

#[derive(Debug, Deserialize)]
struct LegacyDevice {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    ip: Option<String>,
    #[serde(default)]
    version: Option<Value>,
    #[serde(default)]
    mapping: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct LegacyConfig {
    #[serde(default)]
    device_id: Option<String>,
    #[serde(default)]
    ip: Option<String>,
    #[serde(default)]
    version: Option<Value>,
    #[serde(default)]
    local_key: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LegacyCalibrationPoint {
    tono: f64,
    #[serde(default = "one")]
    sat: f64,
}

fn one() -> f64 {
    1.0
}

#[derive(Debug, Deserialize)]
struct LegacyState {
    #[serde(default)]
    ip: Option<String>,
    #[serde(default)]
    original: Option<Dps>,
}

/// Un foco a medio importar.
struct Candidate {
    record: DeviceRecord,
    key: Option<String>,
    /// Si el setup traía la versión del protocolo (si no, se asume 3.3 y se avisa).
    version_known: bool,
}

/// Importa el setup de `folder` al `store` y al llavero.
///
/// `default_display`: monitor para la zona "todo el monitor" de los focos nuevos, que es lo que
/// hacía el script. Con `None` los focos nuevos quedan sin zonas hasta que se elijan en la app.
pub fn import_legacy(
    folder: &Path,
    store: &Store,
    secrets: &dyn SecretStore,
    default_display: Option<&DisplayId>,
) -> Result<ImportReport> {
    let devices_file: Option<Vec<LegacyDevice>> = read_optional(folder, "devices.json")?;
    let config_file: Option<LegacyConfig> = read_optional(folder, "config.json")?;
    if devices_file.is_none() && config_file.is_none() {
        return Err(CoreError::Import(
            "la carpeta no tiene devices.json ni config.json".into(),
        ));
    }
    let mut warnings: Vec<String> = Vec::new();

    // 1. Los focos: devices.json, y el de config.json encima
    let mut candidates: BTreeMap<String, Candidate> = BTreeMap::new();
    let mut skipped = 0;
    for legacy in devices_file.unwrap_or_default() {
        if legacy
            .category
            .as_deref()
            .is_some_and(|c| !is_light_category(c))
        {
            skipped += 1;
            continue;
        }
        let record = DeviceRecord {
            id: DeviceId(legacy.id.clone()),
            name: legacy
                .name
                .clone()
                .filter(|n| !n.trim().is_empty())
                .unwrap_or(legacy.id.clone()),
            ip: valid_ip(legacy.ip.as_deref(), &mut warnings),
            auto_ip: legacy.ip.is_none(),
            version: version_of(legacy.version.as_ref()).unwrap_or(ProtocolVersion::V33),
            mapping: parse_mapping(legacy.mapping, &mut warnings),
            capabilities: None,
            zones: Vec::new(),
            overrides: SettingsOverride::default(),
            order: 0,
        };
        let version_known = version_of(legacy.version.as_ref()).is_some();
        candidates.insert(
            legacy.id,
            Candidate {
                record,
                key: legacy.key.filter(|k| !k.is_empty()),
                version_known,
            },
        );
    }
    let config_device = config_file.as_ref().and_then(|c| c.device_id.clone());
    if let Some(config) = &config_file
        && let Some(id) = &config.device_id
    {
        let candidate = candidates.entry(id.clone()).or_insert_with(|| Candidate {
            record: DeviceRecord {
                id: DeviceId(id.clone()),
                name: id.clone(),
                ip: None,
                auto_ip: true,
                version: ProtocolVersion::V33,
                mapping: None,
                capabilities: None,
                zones: Vec::new(),
                overrides: SettingsOverride::default(),
                order: 0,
            },
            key: None,
            version_known: false,
        });
        // Como `cargar_config`: lo de config.json manda sobre lo de devices.json
        if let Some(key) = config.local_key.clone().filter(|k| !k.is_empty()) {
            candidate.key = Some(key);
        }
        if let Some(ip) = valid_ip(config.ip.as_deref(), &mut warnings) {
            candidate.record.ip = Some(ip);
            candidate.record.auto_ip = false;
        }
        if let Some(version) = version_of(config.version.as_ref()) {
            candidate.record.version = version;
            candidate.version_known = true;
        }
    }
    if skipped > 0 {
        warnings.push(WARN_SKIPPED_NON_LIGHT.to_owned());
    }
    if candidates.is_empty() {
        return Err(CoreError::Import("no hay ningún foco para importar".into()));
    }

    // 2. La calibración y el estado, antes de escribir nada (si están rotos no se pierde nada)
    let calibration = read_calibration(folder, &mut warnings);
    let calibration_owner: Option<String> = match (&config_device, candidates.len()) {
        (Some(id), _) => Some(id.clone()),
        (None, 1) => candidates.keys().next().cloned(),
        _ => None,
    };
    if calibration.is_some() && calibration_owner.is_none() {
        warnings.push(WARN_CALIBRATION_UNASSIGNED.to_owned());
    }
    let legacy_state: Option<LegacyState> =
        read_optional(folder, "foco_estado.json").unwrap_or(None);

    // 3. Escribir: el llavero primero, así un fallo no deja focos sin clave
    let mut existing = store.load_devices()?;
    let mut imported = Vec::new();
    for (id, candidate) in candidates {
        let Candidate {
            mut record,
            key,
            version_known,
        } = candidate;
        match key {
            Some(key) => secrets.set(
                &SecretKey::DeviceLocalKey(record.id.clone()),
                &Secret::new(key),
            )?,
            None => warnings.push(WARN_NO_KEY.to_owned()),
        }
        if !version_known {
            warnings.push(WARN_VERSION_ASSUMED.to_owned());
        }
        match existing.iter_mut().find(|d| d.id == record.id) {
            // Ya estaba: se actualiza lo técnico y se conserva lo que puso el usuario
            Some(current) => {
                current.ip = record.ip;
                current.auto_ip = record.auto_ip;
                current.version = record.version;
                current.mapping = record.mapping.or(current.mapping.take());
            }
            None => {
                if let Some(display) = default_display {
                    record.zones = vec![Zone::from_preset(display.clone(), ZonePreset::Full)];
                }
                record.order = next_order(&existing);
                existing.push(record);
            }
        }
        imported.push(DeviceId(id));
    }
    store.save_devices(&existing)?;

    let mut calibrations = 0;
    if let (Some(points), Some(owner)) = (calibration, calibration_owner) {
        store.save_calibration(&DeviceId(owner), &points)?;
        calibrations = 1;
    }
    if let (Some(state), Some(owner)) = (legacy_state, config_device) {
        let owner = DeviceId(owner);
        if let Some(ip) = state.ip {
            store.set_last_ip(&owner, &ip)?;
        }
        if let Some(original) = state.original {
            store.set_original(&owner, original)?;
        }
    }

    warnings.sort();
    warnings.dedup();
    Ok(ImportReport {
        devices: imported,
        calibrations,
        // El config.json del script solo tiene el foco; los ajustes eran argumentos de línea de comandos
        settings_applied: false,
        warnings,
    })
}

/// Lee `folder/name` como JSON. `None` si no existe.
fn read_optional<T: for<'de> Deserialize<'de>>(folder: &Path, name: &str) -> Result<Option<T>> {
    let path = folder.join(name);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(CoreError::Import(format!("no se pudo leer {name}: {e}"))),
    };
    // El mensaje de serde repite el valor que no esperaba (que puede ser una clave local), y
    // este error llega a la UI: solo se dice dónde falla
    serde_json::from_str(&text).map(Some).map_err(|e| {
        CoreError::Import(format!(
            "{name} no tiene el formato esperado (línea {}, columna {})",
            e.line(),
            e.column()
        ))
    })
}

fn read_calibration(folder: &Path, warnings: &mut Vec<String>) -> Option<Vec<CalibrationPoint>> {
    let raw: BTreeMap<String, LegacyCalibrationPoint> =
        match read_optional(folder, "calibracion.json") {
            Ok(raw) => raw?,
            Err(_) => {
                warnings.push(WARN_CALIBRATION_INVALID.to_owned());
                return None;
            }
        };
    let mut points = Vec::with_capacity(raw.len());
    for (hue, p) in raw {
        let hue: Option<f64> = hue.trim().parse().ok().filter(|h: &f64| h.is_finite());
        match hue {
            Some(hue) if p.tono.is_finite() && p.sat.is_finite() => points.push(CalibrationPoint {
                screen_hue_deg: hue as f32,
                tono_deg: p.tono as f32,
                sat: p.sat as f32,
            }),
            _ => {
                warnings.push(WARN_CALIBRATION_INVALID.to_owned());
                return None;
            }
        }
    }
    points.sort_by(|a, b| a.screen_hue_deg.total_cmp(&b.screen_hue_deg));
    (!points.is_empty()).then_some(points)
}

fn valid_ip(ip: Option<&str>, warnings: &mut Vec<String>) -> Option<String> {
    let ip = ip?.trim();
    if ip.is_empty() {
        return None;
    }
    if ip.parse::<IpAddr>().is_ok() {
        Some(ip.to_owned())
    } else {
        warnings.push(WARN_IP_INVALID.to_owned());
        None
    }
}

/// La versión del script puede venir como número (`3.5`) o como texto (`"3.5"`).
fn version_of(value: Option<&Value>) -> Option<ProtocolVersion> {
    match value? {
        Value::Number(n) => ProtocolVersion::from_f32(n.as_f64()? as f32),
        Value::String(s) => ProtocolVersion::parse(s),
        _ => None,
    }
}

fn parse_mapping(value: Option<Value>, warnings: &mut Vec<String>) -> Option<DpMapping> {
    let value = value?;
    match serde_json::from_value::<DpMapping>(value) {
        Ok(mapping) => Some(mapping),
        Err(_) => {
            warnings.push(WARN_MAPPING_INVALID.to_owned());
            None
        }
    }
}
