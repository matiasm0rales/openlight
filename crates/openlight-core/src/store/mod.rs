//! Persistencia en la carpeta de datos de la app.
//!
//! ```text
//! <datos>/
//! ├─ config.json              ajustes globales, perfiles y región de la nube
//! ├─ devices.json             focos configurados (sin claves: esas van al llavero)
//! ├─ calibration/<foco>.json  tabla de calibración de cada foco
//! └─ state.json               última IP y estado original de cada foco
//! ```
//!
//! Todos los archivos llevan `"version"`. Al leerlos se migran hasta la versión actual; uno
//! de una versión más nueva se rechaza en vez de pisarlo. Las escrituras son atómicas
//! (archivo temporal + renombrar), así que un corte de luz no deja un JSON a medias.
//!
//! Los secretos **no** pasan por acá: `Secret` ni siquiera implementa `Serialize`.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use openlight_types::ipc::CalibrationPoint;
use openlight_types::{
    Capabilities, DeviceId, DpMapping, Dps, Profile, ProfileId, ProtocolVersion, Region, Settings,
    SettingsOverride, Zone,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::error::{CoreError, Result};

/// Versión actual de `config.json`.
pub const CONFIG_VERSION: u32 = 1;
/// Versión actual de `devices.json`.
pub const DEVICES_VERSION: u32 = 1;
/// Versión actual de `calibration/<foco>.json`.
pub const CALIBRATION_VERSION: u32 = 1;
/// Versión actual de `state.json`.
pub const STATE_VERSION: u32 = 1;
/// Máximo de zonas por foco.
pub const MAX_ZONES: usize = 8;

/// Contenido de `config.json`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Config {
    /// Ajustes globales que pisan los valores por defecto del catálogo.
    #[serde(default)]
    pub global: SettingsOverride,
    /// Perfil activo.
    #[serde(default)]
    pub active_profile: Option<ProfileId>,
    /// Perfiles (de fábrica y propios).
    #[serde(default)]
    pub profiles: Vec<Profile>,
    /// Región de la cuenta de Tuya (el Access ID y el Access Secret van en el llavero).
    #[serde(default)]
    pub cloud_region: Option<Region>,
}

impl Config {
    /// Ajustes efectivos: catálogo ← globales ← perfil activo ← (opcional) lo propio de un foco.
    pub fn effective_settings(&self, device: Option<&SettingsOverride>) -> Result<Settings> {
        let invalid = |e: openlight_types::SettingsError| CoreError::Invalid(e.to_string());
        let mut settings = Settings::default()
            .with_overrides(&self.global)
            .map_err(invalid)?;
        if let Some(profile) = self
            .active_profile
            .as_ref()
            .and_then(|id| self.profiles.iter().find(|p| &p.id == id))
        {
            settings = settings.with_overrides(&profile.values).map_err(invalid)?;
        }
        if let Some(overrides) = device {
            settings = settings.with_overrides(overrides).map_err(invalid)?;
        }
        // Cada capa puede ser válida y la combinación no: manda el máximo, que es el tope
        settings.brightness_min = settings.brightness_min.min(settings.brightness_max);
        Ok(settings)
    }
}

/// Un foco configurado, tal como se guarda en `devices.json` (sin la clave local).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceRecord {
    /// Id de Tuya.
    pub id: DeviceId,
    /// Nombre para mostrar.
    pub name: String,
    /// IP fija o última conocida.
    #[serde(default)]
    pub ip: Option<String>,
    /// Si la IP se busca automáticamente.
    #[serde(default = "default_true")]
    pub auto_ip: bool,
    /// Protocolo.
    pub version: ProtocolVersion,
    /// Mapeo de DPs de la nube, si se conoce (de ahí salen las capacidades).
    #[serde(default)]
    pub mapping: Option<DpMapping>,
    /// Capacidades ya resueltas, si se conocen.
    #[serde(default)]
    pub capabilities: Option<Capabilities>,
    /// Zonas que sigue.
    #[serde(default)]
    pub zones: Vec<Zone>,
    /// Ajustes propios del foco.
    #[serde(default)]
    pub overrides: SettingsOverride,
    /// Lugar en la lista de focos (el menor va primero). Los archivos de antes no lo traen:
    /// quedan en 0 y desempata el nombre.
    #[serde(default)]
    pub order: u32,
}

/// El `order` de un foco que se agrega a `devices`: va después de todos.
pub fn next_order(devices: &[DeviceRecord]) -> u32 {
    devices
        .iter()
        .map(|d| d.order)
        .max()
        .map_or(0, |last| last.saturating_add(1))
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize, Deserialize)]
struct DevicesFile {
    version: u32,
    devices: Vec<DeviceRecord>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CalibrationFile {
    version: u32,
    points: Vec<CalibrationPoint>,
}

/// Lo que se recuerda de un foco entre ejecuciones (`foco_estado.json` del script).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct DeviceState {
    /// Última IP en la que respondió.
    #[serde(default)]
    pub last_ip: Option<String>,
    /// Versión del protocolo que anunció el foco cuando difería de la del registro.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<ProtocolVersion>,
    /// Estado al que volver al salir (`DPS_A_RESTAURAR`). Mientras exista, la app no cerró limpio.
    #[serde(default)]
    pub original: Option<Dps>,
}

/// Contenido de `state.json`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct State {
    /// Estado por foco.
    #[serde(default)]
    pub devices: BTreeMap<DeviceId, DeviceState>,
}

/// La carpeta de datos de la app. Se puede clonar y compartir entre tareas.
#[derive(Debug, Clone)]
pub struct Store {
    dir: PathBuf,
    /// Serializa las lecturas-modificaciones-escrituras (`update_state`).
    write_lock: Arc<Mutex<()>>,
}

impl Store {
    /// Abre (y crea si falta) la carpeta de datos.
    pub fn open(dir: impl Into<PathBuf>) -> Result<Self> {
        let dir = dir.into();
        fs::create_dir_all(dir.join("calibration")).map_err(|e| CoreError::io(&dir, &e))?;
        Ok(Self {
            dir,
            write_lock: Arc::default(),
        })
    }

    /// La carpeta de datos.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    // ---------- config.json ----------

    /// Lee `config.json`; si no existe, la configuración por defecto.
    pub fn load_config(&self) -> Result<Config> {
        let mut config: Config = self
            .read("config.json", CONFIG_VERSION, migrate_config)?
            .unwrap_or_default();
        config.global.upgrade();
        for profile in &mut config.profiles {
            profile.values.upgrade();
        }
        config
            .global
            .validate()
            .map_err(|e| CoreError::Invalid(format!("config.json: {e}")))?;
        Ok(config)
    }

    /// Guarda `config.json` después de validar los ajustes.
    pub fn save_config(&self, config: &Config) -> Result<()> {
        config
            .global
            .validate()
            .map_err(|e| CoreError::Invalid(e.to_string()))?;
        for profile in &config.profiles {
            profile
                .values
                .validate()
                .map_err(|e| CoreError::Invalid(format!("perfil {}: {e}", profile.id.0)))?;
        }
        let _guard = self.guard();
        self.write("config.json", CONFIG_VERSION, config)
    }

    // ---------- devices.json ----------

    /// Lee los focos configurados, en el orden en que se muestran.
    pub fn load_devices(&self) -> Result<Vec<DeviceRecord>> {
        let file: Option<DevicesFile> =
            self.read("devices.json", DEVICES_VERSION, migrate_identity)?;
        let mut devices = file.map(|f| f.devices).unwrap_or_default();
        for device in &mut devices {
            device.overrides.upgrade();
            device
                .overrides
                .validate()
                .map_err(|e| CoreError::Invalid(format!("devices.json: {e}")))?;
        }
        devices.sort_by_cached_key(|d| (d.order, d.name.to_lowercase()));
        Ok(devices)
    }

    /// Guarda los focos configurados.
    pub fn save_devices(&self, devices: &[DeviceRecord]) -> Result<()> {
        let _guard = self.guard();
        self.write_devices(devices)
    }

    /// Deja los focos en el orden de `ids`, que tiene que nombrar a cada foco configurado
    /// una sola vez; si no, no cambia nada.
    pub fn reorder_devices(&self, ids: &[DeviceId]) -> Result<()> {
        let _guard = self.guard();
        let mut devices = self.load_devices()?;
        let places: BTreeMap<&DeviceId, u32> = ids.iter().zip(0..).collect();
        if places.len() != ids.len()
            || ids.len() != devices.len()
            || !devices.iter().all(|d| places.contains_key(&d.id))
        {
            return Err(CoreError::DeviceOrderMismatch);
        }
        for device in &mut devices {
            device.order = places[&device.id];
        }
        devices.sort_by_key(|d| d.order);
        self.write_devices(&devices)
    }

    /// Valida y escribe `devices.json`. Quien llama tiene tomado el candado de escritura.
    fn write_devices(&self, devices: &[DeviceRecord]) -> Result<()> {
        for device in devices {
            check_device_id(&device.id)?;
            device
                .overrides
                .validate()
                .map_err(|e| CoreError::Invalid(e.to_string()))?;
        }
        let file = json!({ "devices": devices });
        self.write_value("devices.json", DEVICES_VERSION, file)
    }

    // ---------- calibration/<foco>.json ----------

    fn calibration_name(device: &DeviceId) -> Result<String> {
        check_device_id(device)?;
        Ok(format!("calibration/{}.json", device.0))
    }

    /// Lee la calibración de un foco, o `None` si no tiene.
    pub fn load_calibration(&self, device: &DeviceId) -> Result<Option<Vec<CalibrationPoint>>> {
        let name = Self::calibration_name(device)?;
        let file: Option<CalibrationFile> =
            self.read(&name, CALIBRATION_VERSION, migrate_identity)?;
        Ok(file.map(|f| f.points))
    }

    /// Guarda la calibración de un foco.
    pub fn save_calibration(&self, device: &DeviceId, points: &[CalibrationPoint]) -> Result<()> {
        let name = Self::calibration_name(device)?;
        let _guard = self.guard();
        self.write_value(&name, CALIBRATION_VERSION, json!({ "points": points }))
    }

    /// Borra la calibración de un foco (no es un error si no había).
    pub fn delete_calibration(&self, device: &DeviceId) -> Result<()> {
        let path = self.dir.join(Self::calibration_name(device)?);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(CoreError::io(path, &e)),
        }
    }

    // ---------- state.json ----------

    /// Lee `state.json`.
    pub fn load_state(&self) -> Result<State> {
        Ok(self
            .read("state.json", STATE_VERSION, migrate_identity)?
            .unwrap_or_default())
    }

    /// Lee, modifica y guarda `state.json` sin que dos escrituras se pisen.
    pub fn update_state(&self, change: impl FnOnce(&mut State)) -> Result<State> {
        let _guard = self.guard();
        let mut state = self.load_state()?;
        change(&mut state);
        self.write("state.json", STATE_VERSION, &state)?;
        Ok(state)
    }

    /// Recuerda la última IP en la que respondió un foco.
    pub fn set_last_ip(&self, device: &DeviceId, ip: &str) -> Result<()> {
        self.update_state(|s| {
            s.devices.entry(device.clone()).or_default().last_ip = Some(ip.to_owned())
        })
        .map(drop)
    }

    /// Guarda el estado al que hay que volver (antes de tocar el foco).
    pub fn set_original(&self, device: &DeviceId, original: Dps) -> Result<()> {
        self.update_state(|s| {
            s.devices.entry(device.clone()).or_default().original = Some(original)
        })
        .map(drop)
    }

    /// Olvida el estado original (ya se restauró).
    pub fn clear_original(&self, device: &DeviceId) -> Result<()> {
        self.update_state(|s| {
            if let Some(d) = s.devices.get_mut(device) {
                d.original = None;
            }
        })
        .map(drop)
    }

    // ---------- mecánica ----------

    fn guard(&self) -> std::sync::MutexGuard<'_, ()> {
        self.write_lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Lee un archivo, lo migra y lo deserializa. `None` si no existe.
    fn read<T: DeserializeOwned>(
        &self,
        name: &str,
        current: u32,
        migrate: fn(u32, Value) -> Result<Value>,
    ) -> Result<Option<T>> {
        let path = self.dir.join(name);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(CoreError::io(path, &e)),
        };
        let value: Value = serde_json::from_str(&text).map_err(|e| CoreError::json(&path, &e))?;
        let value = migrate_to(name, value, current, migrate)?;
        serde_json::from_value(value)
            .map(Some)
            .map_err(|e| CoreError::json(path, &e))
    }

    fn write<T: Serialize>(&self, name: &str, version: u32, value: &T) -> Result<()> {
        let value =
            serde_json::to_value(value).map_err(|e| CoreError::json(self.dir.join(name), &e))?;
        self.write_value(name, version, value)
    }

    /// Escribe `value` (un objeto) con su `"version"`, de forma atómica.
    fn write_value(&self, name: &str, version: u32, mut value: Value) -> Result<()> {
        let path = self.dir.join(name);
        if let Value::Object(map) = &mut value {
            map.insert("version".to_owned(), json!(version));
        }
        let text = serde_json::to_string_pretty(&value).map_err(|e| CoreError::json(&path, &e))?;
        let tmp = path.with_extension("json.tmp");
        let write = || -> std::io::Result<()> {
            let mut file = fs::File::create(&tmp)?;
            file.write_all(text.as_bytes())?;
            file.sync_all()?;
            fs::rename(&tmp, &path)
        };
        write().map_err(|e| {
            let _ = fs::remove_file(&tmp);
            CoreError::io(&path, &e)
        })
    }
}

/// Ids de Tuya: letras, números, `_` y `-`. Nada que salga de la carpeta de calibración.
pub(crate) fn check_device_id(id: &DeviceId) -> Result<()> {
    let ok = !id.0.is_empty()
        && id.0.len() <= 64
        && id
            .0
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if ok {
        Ok(())
    } else {
        Err(CoreError::Invalid(format!(
            "id de foco no válido: {:?}",
            id.0
        )))
    }
}

/// Lleva `value` de su versión a `current`, un paso por vez.
fn migrate_to(
    name: &str,
    mut value: Value,
    current: u32,
    step: fn(u32, Value) -> Result<Value>,
) -> Result<Value> {
    let found = value
        .get("version")
        .and_then(Value::as_u64)
        .map_or(0, |v| u32::try_from(v).unwrap_or(u32::MAX));
    if found > current {
        return Err(CoreError::UnsupportedVersion {
            file: name.to_owned(),
            found,
            supported: current,
        });
    }
    for from in found..current {
        value = step(from, value)?;
        if let Value::Object(map) = &mut value {
            map.insert("version".to_owned(), json!(from + 1));
        }
    }
    Ok(value)
}

/// Migraciones de `config.json`.
///
/// - v0 → v1: los prototipos guardaban los ajustes globales bajo `"settings"`; ahora es `"global"`.
fn migrate_config(from: u32, mut value: Value) -> Result<Value> {
    if from == 0
        && let Value::Object(map) = &mut value
        && let Some(old) = map.remove("settings")
    {
        map.entry("global").or_insert(old);
    }
    Ok(value)
}

/// Los demás archivos todavía están en su primera versión: migrar es solo ponerles la versión.
fn migrate_identity(_from: u32, value: Value) -> Result<Value> {
    Ok(value)
}

#[cfg(test)]
mod tests;
