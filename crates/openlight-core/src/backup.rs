//! Respaldo sin claves: exportar lo configurado para llevarlo a otra compu e importarlo.
//!
//! **El archivo no tiene dónde poner un secreto.** [`Backup`] no tiene campos para claves
//! locales, credenciales de la nube ni IPs, así que no se pueden serializar; las claves se vuelven
//! a bajar de la nube de Tuya después de importar. Tampoco lleva el mapeo de DPs (`values` es
//! JSON libre): se recupera junto con la clave.

use std::collections::HashSet;
use std::hash::Hash;

use openlight_types::ipc::{
    BackupDeviceSummary, BackupSummary, CalibrationPoint, DuplicatePolicy, ImportReport,
};
use openlight_types::{
    Capabilities, DeviceId, DisplayId, Profile, ProfileId, ProtocolVersion, SettingsOverride, Zone,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{CoreError, Result};
use crate::import::error_place;
use crate::store::{Config, DeviceRecord, MAX_ZONES, Store, check_device_id};

/// Identifica el archivo.
pub const FORMAT: &str = "openlight-backup";
/// Versión actual del formato.
pub const VERSION: u32 = 1;
/// Tamaño máximo de un respaldo que se acepta leer.
pub const MAX_BYTES: usize = 2 * 1024 * 1024;
/// Alguna zona apuntaba a un monitor que no está en esta compu.
pub const WARN_ZONES_MOVED: &str = "import.warning.backup_zones_moved";

// Topes de lo que se acepta: muy por encima de cualquier setup real
const MAX_DEVICES: usize = 256;
const MAX_PROFILES: usize = 64;
const MAX_POINTS: usize = 360;
const MAX_TEXT: usize = 200;

/// El archivo de respaldo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Backup {
    /// Siempre [`FORMAT`].
    pub format: String,
    /// Versión del formato.
    pub version: u32,
    /// Cuándo se exportó (UTC, `2026-11-24T18:30:05Z`).
    pub created_at: String,
    /// Versión de OpenLight que lo exportó.
    pub app_version: String,
    /// Ajustes globales.
    #[serde(default)]
    pub settings: BackupSettings,
    /// Perfiles (de fábrica y propios).
    #[serde(default)]
    pub profiles: Vec<Profile>,
    /// Focos.
    #[serde(default)]
    pub devices: Vec<BackupDevice>,
    /// Focos que el usuario sacó del efecto.
    #[serde(default)]
    pub excluded_devices: Vec<DeviceId>,
}

/// Los ajustes globales de `config.json` (la región de la nube va con las credenciales).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct BackupSettings {
    /// Ajustes globales que pisan los valores por defecto.
    #[serde(default)]
    pub global: SettingsOverride,
    /// Perfil activo.
    #[serde(default)]
    pub active_profile: Option<ProfileId>,
}

/// Un foco en el respaldo: sin clave y sin IP.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackupDevice {
    /// Id de Tuya.
    pub id: DeviceId,
    /// Nombre para mostrar.
    pub name: String,
    /// Protocolo.
    pub version: ProtocolVersion,
    /// Capacidades, si se conocen.
    #[serde(default)]
    pub capabilities: Option<Capabilities>,
    /// Zonas que sigue.
    #[serde(default)]
    pub zones: Vec<Zone>,
    /// Ajustes propios del foco.
    #[serde(default)]
    pub overrides: SettingsOverride,
    /// Tabla de calibración, si tiene.
    #[serde(default)]
    pub calibration: Option<Vec<CalibrationPoint>>,
}

/// Lo que hay en la compu donde se importa y no sale de la carpeta de datos.
pub struct Destination<'a> {
    /// Los perfiles de fábrica: uno que está sin tocar no cuenta como "ya lo tenés".
    pub factory_profiles: &'a [Profile],
    /// Monitores de esta compu (vacío si no se pudieron listar).
    pub displays: &'a [DisplayId],
    /// A dónde pasar las zonas de un monitor que acá no está.
    pub fallback_display: Option<&'a DisplayId>,
}

/// Lo que se aplicó.
#[derive(Debug, Clone, PartialEq)]
pub struct Applied {
    /// Para la UI.
    pub report: ImportReport,
    /// Focos que no estaban.
    pub added: Vec<DeviceId>,
    /// Focos que estaban y se pisaron.
    pub replaced: Vec<DeviceId>,
    /// De los importados, los que quedan fuera del efecto (el resto participa).
    pub excluded: Vec<DeviceId>,
}

/// Junta lo configurado en `store`. `capabilities` resuelve las de cada foco y `excluded` son los
/// focos que están fuera del efecto.
pub fn collect(
    store: &Store,
    excluded: &[DeviceId],
    capabilities: &dyn Fn(&DeviceRecord) -> Option<Capabilities>,
    created_at: String,
    app_version: String,
) -> Result<Backup> {
    let config = store.load_config()?;
    let state = store.load_state()?;
    let records = store.load_devices()?;
    let mut devices = Vec::with_capacity(records.len());
    for record in &records {
        devices.push(BackupDevice {
            id: record.id.clone(),
            name: record.name.clone(),
            // Si el foco anunció otra versión que la guardada, vale la que anunció
            version: state
                .devices
                .get(&record.id)
                .and_then(|s| s.version)
                .unwrap_or(record.version),
            capabilities: capabilities(record),
            zones: record.zones.clone(),
            overrides: record.overrides.clone(),
            calibration: store.load_calibration(&record.id)?,
        });
    }
    let mut excluded: Vec<DeviceId> = excluded
        .iter()
        .filter(|id| records.iter().any(|r| &r.id == *id))
        .cloned()
        .collect();
    excluded.sort();
    Ok(Backup {
        format: FORMAT.to_owned(),
        version: VERSION,
        created_at,
        app_version,
        settings: BackupSettings {
            global: config.global,
            active_profile: config.active_profile,
        },
        profiles: config.profiles,
        devices,
        excluded_devices: excluded,
    })
}

/// El texto del archivo.
pub fn render(backup: &Backup) -> Result<String> {
    serde_json::to_string_pretty(backup)
        .map_err(|_| CoreError::Backup("no se pudo armar el archivo".into()))
}

/// Lee y valida un respaldo. Los errores dicen dónde falla, nunca qué decía: el archivo elegido
/// puede ser cualquiera (por ejemplo, el `devices.json` del script, que tiene claves).
pub fn parse(text: &str) -> Result<Backup> {
    if text.len() > MAX_BYTES {
        return Err(invalid("el archivo es demasiado grande"));
    }
    // Algunos editores de Windows guardan con BOM
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let not_json =
        |e: serde_json::Error| invalid(format!("no es JSON válido ({})", error_place(&e)));
    // Primero qué es y de qué versión: un formato nuevo puede no parecerse a este
    let value: Value = serde_json::from_str(text).map_err(not_json)?;
    if value.get("format").and_then(Value::as_str) != Some(FORMAT) {
        return Err(invalid("no es un respaldo de OpenLight"));
    }
    match value.get("version").and_then(Value::as_u64) {
        None => return Err(invalid("falta la versión del formato")),
        Some(found) if found != u64::from(VERSION) => {
            return Err(CoreError::BackupVersion {
                found,
                supported: VERSION,
            });
        }
        Some(_) => {}
    }
    let mut backup: Backup = serde_json::from_str(text).map_err(|e| {
        invalid(format!(
            "no tiene el formato esperado ({})",
            error_place(&e)
        ))
    })?;
    validate(&mut backup)?;
    Ok(backup)
}

fn invalid(detail: impl Into<String>) -> CoreError {
    CoreError::Backup(detail.into())
}

/// Que todo lo del archivo sea algo que la app podría haber guardado. Los mensajes nombran el
/// campo, nunca su valor.
fn validate(backup: &mut Backup) -> Result<()> {
    let short = |text: &str| text.chars().count() <= MAX_TEXT;
    let named = |text: &str| !text.trim().is_empty() && short(text);
    require(
        short(&backup.created_at) && short(&backup.app_version),
        "encabezado demasiado largo",
    )?;
    require(backup.devices.len() <= MAX_DEVICES, "demasiados focos")?;
    require(backup.profiles.len() <= MAX_PROFILES, "demasiados perfiles")?;
    check_values(&mut backup.settings.global, "ajustes globales")?;

    require(
        unique(backup.profiles.iter().map(|p| &p.id)),
        "hay un perfil repetido",
    )?;
    for profile in &mut backup.profiles {
        require(
            named(&profile.id.0) && named(&profile.name),
            "hay un perfil sin id o sin nombre",
        )?;
        require(
            profile.shortcut.as_deref().is_none_or(short),
            "hay un atajo de teclado demasiado largo",
        )?;
        check_values(&mut profile.values, "ajustes de un perfil")?;
    }

    require(
        unique(backup.devices.iter().map(|d| &d.id)),
        "hay un foco repetido",
    )?;
    for device in &mut backup.devices {
        require(
            check_device_id(&device.id).is_ok(),
            "hay un id de foco no válido",
        )?;
        require(named(&device.name), "hay un foco sin nombre")?;
        require(
            device.zones.len() <= MAX_ZONES && device.zones.iter().all(|z| z.rect.is_valid()),
            "hay un foco con zonas no válidas",
        )?;
        check_values(&mut device.overrides, "ajustes de un foco")?;
        if let Some(points) = &device.calibration {
            let finite = points.iter().all(|p| {
                [p.screen_hue_deg, p.tono_deg, p.sat]
                    .iter()
                    .all(|v| v.is_finite())
            });
            require(
                points.len() <= MAX_POINTS && finite,
                "hay una calibración no válida",
            )?;
        }
    }
    Ok(())
}

fn require(ok: bool, detail: &str) -> Result<()> {
    if ok { Ok(()) } else { Err(invalid(detail)) }
}

fn unique<T: Eq + Hash>(items: impl IntoIterator<Item = T>) -> bool {
    let mut seen = HashSet::new();
    items.into_iter().all(|item| seen.insert(item))
}

fn check_values(values: &mut SettingsOverride, place: &str) -> Result<()> {
    values.upgrade();
    values
        .validate()
        .map_err(|_| invalid(format!("hay un valor fuera de rango en: {place}")))
}

/// Qué trae el respaldo y qué de eso ya está configurado.
pub fn summarize(backup: &Backup, existing: &[DeviceRecord]) -> BackupSummary {
    let (existing_devices, new_devices) = backup
        .devices
        .iter()
        .map(|d| BackupDeviceSummary {
            id: d.id.clone(),
            name: d.name.clone(),
        })
        .partition(|d| existing.iter().any(|e| e.id == d.id));
    BackupSummary {
        created_at: backup.created_at.clone(),
        app_version: backup.app_version.clone(),
        new_devices,
        existing_devices,
    }
}

/// Importa un respaldo ya validado por [`parse`]. Nunca borra un foco, un perfil ni una
/// calibración que no estén en el archivo.
pub fn apply(
    backup: Backup,
    store: &Store,
    policy: DuplicatePolicy,
    here: &Destination<'_>,
) -> Result<Applied> {
    let replace = policy == DuplicatePolicy::Replace;
    let mut config = store.load_config()?;
    let mut records = store.load_devices()?;
    let settings_applied =
        merge_settings(&mut config, backup.settings, backup.profiles, replace, here);

    let mut imported = Vec::new();
    let mut added = Vec::new();
    let mut replaced = Vec::new();
    let mut calibrations = Vec::new();
    let mut zones_moved = false;
    for mut device in backup.devices {
        let current = records.iter().position(|r| r.id == device.id);
        if current.is_some() && !replace {
            continue;
        }
        zones_moved |= relocate(&mut device.zones, here);
        let id = device.id.clone();
        if let Some(points) = device.calibration {
            calibrations.push((id.clone(), points));
        }
        match current.map(|i| &mut records[i]) {
            // Se pisa lo que trae el respaldo. La IP y el mapeo son de esta compu
            Some(record) => {
                record.name = device.name;
                record.version = device.version;
                record.zones = device.zones;
                record.overrides = device.overrides;
                if device.capabilities.is_some() {
                    record.capabilities = device.capabilities;
                }
                replaced.push(id.clone());
            }
            None => {
                records.push(DeviceRecord {
                    id: id.clone(),
                    name: device.name,
                    ip: None,
                    auto_ip: true,
                    version: device.version,
                    mapping: None,
                    capabilities: device.capabilities,
                    zones: device.zones,
                    overrides: device.overrides,
                });
                added.push(id.clone());
            }
        }
        imported.push(id);
    }

    store.save_config(&config)?;
    store.save_devices(&records)?;
    for (id, points) in &calibrations {
        store.save_calibration(id, points)?;
    }
    let excluded = imported
        .iter()
        .filter(|id| backup.excluded_devices.contains(id))
        .cloned()
        .collect();
    Ok(Applied {
        report: ImportReport {
            devices: imported,
            calibrations: u32::try_from(calibrations.len()).unwrap_or(u32::MAX),
            settings_applied,
            warnings: zones_moved
                .then(|| WARN_ZONES_MOVED.to_owned())
                .into_iter()
                .collect(),
        },
        added,
        replaced,
        excluded,
    })
}

/// Suma los perfiles y, si corresponde, los ajustes globales. Con "saltear", lo que el usuario ya
/// cambió acá no se toca. Devuelve si se aplicaron los ajustes globales del respaldo.
fn merge_settings(
    config: &mut Config,
    settings: BackupSettings,
    profiles: Vec<Profile>,
    replace: bool,
    here: &Destination<'_>,
) -> bool {
    let untouched = config.global.0.is_empty() && config.active_profile.is_none();
    for profile in profiles {
        match config.profiles.iter_mut().find(|p| p.id == profile.id) {
            Some(local) => {
                // Un perfil de fábrica sin tocar no es "del usuario": se puede completar
                if replace || here.factory_profiles.contains(local) {
                    local.values = profile.values;
                    local.shortcut = profile.shortcut;
                    // El nombre de uno de fábrica es una clave de traducción: no se cambia
                    if !local.builtin {
                        local.name = profile.name;
                    }
                }
            }
            // Un archivo no puede crear perfiles de fábrica (no se podrían borrar)
            None => config.profiles.push(Profile {
                builtin: false,
                ..profile
            }),
        }
    }
    let apply = replace || untouched;
    if apply {
        config.global = settings.global;
        config.active_profile = settings
            .active_profile
            .filter(|id| config.profiles.iter().any(|p| &p.id == id));
    }
    apply
}

/// Pasa al monitor de esta compu las zonas de uno que no está. `true` si movió alguna.
fn relocate(zones: &mut [Zone], here: &Destination<'_>) -> bool {
    let Some(fallback) = here.fallback_display else {
        return false;
    };
    let mut moved = false;
    for zone in zones {
        if !here.displays.contains(&zone.display) {
            zone.display = fallback.clone();
            moved = true;
        }
    }
    moved
}
