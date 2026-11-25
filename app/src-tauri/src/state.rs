//! El estado de la app: todo lo que los comandos IPC comparten.
//!
//! [`Backend`] no conoce a Tauri. Recibe sus dependencias en [`BackendParts`] (carpeta de datos,
//! llavero, destino de los eventos, capturas, nube, búsqueda en la red), así la app real arma
//! las de verdad (`lib.rs`) y las pruebas arman las falsas (`tests/`). Los comandos de `ipc.rs`
//! solo llaman a los métodos de `Backend`, que están repartidos por tema en `cmd/`.

use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, AtomicU8};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use openlight_core::{Config, DeviceLocks, DeviceRecord, PowerEvents, SourceFactory, Store};
use openlight_types::ipc::{ConfiguredDevice, DiscoveredDevice};
use openlight_types::{
    Capabilities, CloudCredentials, CloudDevice, CloudError, ColourFormat, DeviceCreds, DeviceId,
    DpMapping, Profile, ProfileId, Secret, SessionState, Settings, SettingsOverride, TuyaCloud,
    UserFacingError,
};
use secrets::{SecretKey, SecretStore};
use tokio::sync::Mutex as AsyncMutex;
use tokio::task::JoinHandle;
use tuya_device::{
    CapabilitiesExt, DeviceConfig, DeviceHandle, IpMode, Locator, StateStore, Timings, spawn,
};

use crate::bridge::lan::LanScanner;
use crate::bridge::state_store::CoreStateStore;
use crate::errors::{CmdResult, device_error, unknown_device};
use crate::events::{AppEvent, EventSink};

/// Cómo se crea el cliente de la nube a partir de las credenciales.
pub type CloudFactory =
    Arc<dyn Fn(CloudCredentials) -> Result<Arc<dyn TuyaCloud>, CloudError> + Send + Sync>;

/// Último retoque de la configuración de un foco antes de lanzar su actor (las pruebas lo usan
/// para apuntar al puerto del simulador).
pub type DeviceTweak = Arc<dyn Fn(&DeviceId, DeviceConfig) -> DeviceConfig + Send + Sync>;

/// Tiempos y tamaños del backend. Los valores por defecto son los de la app real.
#[derive(Clone)]
pub struct BackendOptions {
    /// Cuánto dura cada color del patrón de prueba (el script usaba 1.5 s).
    pub test_step: Duration,
    /// Sin usarse durante este tiempo, la conexión del control manual se suelta.
    pub manual_idle: Duration,
    /// Cuánto se escucha la red al listar los dispositivos de la nube.
    pub list_scan: Duration,
    /// Cuánto se escucha la red buscando un foco puntual (al agregarlo).
    pub lookup_scan: Duration,
    /// Cuánto se espera a que cada foco vuelva a su estado al terminar la sesión.
    pub restore_timeout: Duration,
    /// Cuánto se espera a que un foco aparezca en el control manual y en las pruebas.
    pub connect_timeout: Duration,
    /// Ancho de la vista previa de un monitor, en píxeles.
    pub preview_width: u32,
    /// Retoque de la configuración de cada foco.
    pub device_tweak: Option<DeviceTweak>,
}

impl Default for BackendOptions {
    fn default() -> Self {
        Self {
            test_step: Duration::from_millis(1500),
            manual_idle: Duration::from_secs(30),
            list_scan: Duration::from_secs(3),
            lookup_scan: Duration::from_secs(5),
            restore_timeout: Duration::from_secs(8),
            connect_timeout: Duration::from_secs(15),
            preview_width: 480,
            device_tweak: None,
        }
    }
}

/// Lo que el backend necesita de afuera.
pub struct BackendParts {
    /// Carpeta de datos de la app.
    pub data_dir: std::path::PathBuf,
    /// Llavero (el del sistema en la app real, en memoria en las pruebas).
    pub secrets: Arc<dyn SecretStore>,
    /// Destino de los eventos.
    pub events: Arc<dyn EventSink>,
    /// Fábrica de capturas de pantalla.
    pub sources: SourceFactory,
    /// Fábrica del cliente de la nube.
    pub cloud: CloudFactory,
    /// Búsqueda de focos en la red.
    pub scanner: Arc<dyn LanScanner>,
    /// Quién encuentra la IP de un foco cuando cambia.
    pub locator: Arc<dyn Locator>,
    /// Tiempos y tamaños.
    pub options: BackendOptions,
}

impl BackendParts {
    /// Las dependencias de la app real: llavero del sistema, capturas del SO, nube y red de verdad.
    ///
    /// `OPENLIGHT_SECRETS=memory` cambia el llavero por uno en memoria: sirve para desarrollar
    /// (`pnpm tauri dev`) sin tocar el llavero real, y los secretos no sobreviven al proceso.
    pub fn production(data_dir: std::path::PathBuf, events: Arc<dyn EventSink>) -> Self {
        let secrets: Arc<dyn SecretStore> =
            if std::env::var("OPENLIGHT_SECRETS").is_ok_and(|v| v == "memory") {
                tracing::warn!("llavero en memoria: los secretos no se guardan");
                Arc::new(secrets::MemoryStore::new())
            } else {
                Arc::new(secrets::KeyringStore::new())
            };
        Self {
            data_dir,
            secrets,
            events,
            sources: crate::bridge::sources::platform_sources(),
            cloud: Arc::new(|creds| {
                tuya_cloud::TuyaCloudClient::new(creds).map(|c| Arc::new(c) as Arc<dyn TuyaCloud>)
            }),
            scanner: Arc::new(crate::bridge::lan::UdpScanner::default()),
            locator: Arc::new(tuya_device::UdpLocator::new()),
            options: BackendOptions::default(),
        }
    }
}

/// La carpeta de datos de la app: `OPENLIGHT_DATA_DIR` si está, y si no la de Tauri.
pub fn data_dir<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    use tauri::Manager;
    match std::env::var_os("OPENLIGHT_DATA_DIR") {
        Some(dir) => Ok(dir.into()),
        None => Ok(app.path().app_data_dir()?),
    }
}

/// Un dispositivo de la nube con su mapeo, tal como se lo vio en el último listado.
#[derive(Clone)]
pub(crate) struct CloudEntry {
    pub device: CloudDevice,
    pub mapping: Option<DpMapping>,
}

/// Lo que se recuerda entre comandos sobre la nube y la red.
#[derive(Default)]
pub(crate) struct Cache {
    pub cloud: Option<Arc<dyn TuyaCloud>>,
    pub cloud_devices: HashMap<DeviceId, CloudEntry>,
    pub lan: HashMap<DeviceId, DiscoveredDevice>,
}

/// Una sesión en marcha con los actores de sus focos.
pub(crate) struct ActiveSession {
    pub session: openlight_core::Session,
    pub handles: Vec<DeviceHandle>,
    /// Reenvía los eventos de la sesión a la UI; devuelve el último estado que vio.
    pub forwarder: JoinHandle<SessionState>,
}

/// Conexión de control manual con un foco, que se suelta sola tras un rato sin usarla.
pub(crate) struct ManualSlot {
    pub handle: DeviceHandle,
    /// `Some(capacidades)` mientras el foco está conectado (ya se llamó a `start`).
    pub link: AsyncMutex<Option<Capabilities>>,
    pub last_used: Mutex<tokio::time::Instant>,
    /// Si ya hay una tarea esperando para soltar la conexión.
    pub reaper: AtomicBool,
}

/// El estado compartido de la app.
pub struct Backend {
    pub(crate) store: Store,
    pub(crate) secrets: Arc<dyn SecretStore>,
    pub(crate) events: Arc<dyn EventSink>,
    pub(crate) sources: SourceFactory,
    pub(crate) cloud_factory: CloudFactory,
    pub(crate) scanner: Arc<dyn LanScanner>,
    pub(crate) locator: Arc<dyn Locator>,
    pub(crate) options: BackendOptions,
    pub(crate) locks: DeviceLocks,
    pub(crate) power: PowerEvents,
    /// Serializa las lecturas-modificaciones-escrituras de `config.json` y `devices.json`: el
    /// `Store` ordena cada escritura, pero no la transacción entera (dos cambios seguidos de un
    /// slider se pisarían).
    pub(crate) edit: Mutex<()>,
    /// El log de la app (no hay nada en las pruebas que no lo instalan).
    pub(crate) logs: Mutex<Option<crate::logging::LogHandle>>,
    /// Efectos sobre las ventanas y el tray (no hay nada en las pruebas).
    pub(crate) shell: Mutex<Option<Arc<dyn crate::shell::Shell>>>,
    /// Focos que el usuario sacó del efecto desde el tray.
    pub(crate) excluded: Mutex<HashSet<DeviceId>>,
    /// 0 = en marcha, 1 = cerrando, 2 = ya cerró (`shutdown`).
    pub(crate) shutdown_state: AtomicU8,
    /// Alguien pidió salir (ver `begin_shutdown`).
    pub(crate) quit_requested: AtomicBool,
    pub(crate) cache: Mutex<Cache>,
    pub(crate) session: AsyncMutex<Option<ActiveSession>>,
    pub(crate) session_state: Mutex<SessionState>,
    pub(crate) manual: Mutex<HashMap<DeviceId, Arc<ManualSlot>>>,
    pub(crate) calibration: AsyncMutex<Option<crate::cmd::calibration::CalibrationRun>>,
}

impl Backend {
    /// Abre la carpeta de datos y arma el backend. Siembra los perfiles de fábrica si faltan.
    pub fn new(parts: BackendParts) -> CmdResult<Arc<Self>> {
        let store = Store::open(&parts.data_dir)?;
        let backend = Arc::new(Self {
            store,
            secrets: parts.secrets,
            events: parts.events,
            sources: parts.sources,
            cloud_factory: parts.cloud,
            scanner: parts.scanner,
            locator: parts.locator,
            options: parts.options,
            locks: DeviceLocks::new(),
            power: PowerEvents::new(),
            edit: Mutex::new(()),
            logs: Mutex::new(None),
            shell: Mutex::new(None),
            excluded: Mutex::new(HashSet::new()),
            shutdown_state: AtomicU8::new(0),
            quit_requested: AtomicBool::new(false),
            cache: Mutex::default(),
            session: AsyncMutex::new(None),
            session_state: Mutex::new(SessionState::Idle),
            manual: Mutex::default(),
            calibration: AsyncMutex::new(None),
        });
        backend.seed_profiles()?;
        *backend
            .excluded
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = backend.load_excluded();
        Ok(backend)
    }

    /// El bus de energía: la integración nativa con el SO publica acá `Suspend` y `Resume`.
    pub fn power(&self) -> PowerEvents {
        self.power.clone()
    }

    /// La carpeta de datos.
    pub fn data_dir(&self) -> &std::path::Path {
        self.store.dir()
    }

    pub(crate) fn emit(&self, event: AppEvent) {
        self.events.emit(event);
    }

    /// Avisa un error que no vino de un comando (el usuario lo ve como aviso).
    pub(crate) fn raise(&self, error: UserFacingError) {
        self.emit(AppEvent::ErrorRaised(error));
    }

    /// Toma el candado de edición de la configuración. No se mantiene a través de un `await`.
    pub(crate) fn edit_lock(&self) -> MutexGuard<'_, ()> {
        self.edit.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn cache(&self) -> MutexGuard<'_, Cache> {
        self.cache.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn set_session_state(&self, state: SessionState) {
        let changed = std::mem::replace(
            &mut *self
                .session_state
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
            state,
        ) != state;
        if changed {
            self.notify_changed();
        }
    }

    /// Conecta la shell de la app (ventanas y tray).
    pub fn set_shell(&self, shell: Arc<dyn crate::shell::Shell>) {
        *self.shell.lock().unwrap_or_else(PoisonError::into_inner) = Some(shell);
    }

    pub(crate) fn shell(&self) -> Option<Arc<dyn crate::shell::Shell>> {
        self.shell
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Avisa a la shell que cambió algo que el tray muestra.
    pub(crate) fn notify_changed(&self) {
        if let Some(shell) = self.shell() {
            shell.changed();
        }
    }

    // ---------- lecturas comunes ----------

    pub(crate) fn config(&self) -> CmdResult<Config> {
        Ok(self.store.load_config()?)
    }

    pub(crate) fn records(&self) -> CmdResult<Vec<DeviceRecord>> {
        Ok(self.store.load_devices()?)
    }

    pub(crate) fn record(&self, id: &DeviceId) -> CmdResult<DeviceRecord> {
        self.records()?
            .into_iter()
            .find(|r| &r.id == id)
            .ok_or_else(|| unknown_device(id))
    }

    /// Ajustes efectivos de un foco: globales ← perfil activo ← lo propio del foco.
    pub(crate) fn settings_for(&self, overrides: &SettingsOverride) -> CmdResult<Settings> {
        Ok(self.config()?.effective_settings(Some(overrides))?)
    }

    /// La clave local de un foco, del llavero. Sin clave, lo que corresponde es renovarla.
    pub(crate) fn local_key(&self, id: &DeviceId) -> CmdResult<Secret<String>> {
        match self.secrets.get(&SecretKey::DeviceLocalKey(id.clone()))? {
            Some(key) => Ok(key),
            None => Err(device_error(id, &openlight_types::DeviceError::KeyRejected)),
        }
    }

    /// Lanza el actor de un foco (todavía sin conectar), con su clave del llavero.
    pub(crate) fn spawn_device(&self, record: &DeviceRecord) -> CmdResult<DeviceHandle> {
        let settings = self.settings_for(&record.overrides)?;
        let creds = DeviceCreds {
            id: record.id.clone(),
            local_key: self.local_key(&record.id)?,
            version: record.version,
            ip: record
                .ip
                .as_deref()
                .and_then(|ip| ip.parse::<IpAddr>().ok()),
        };
        let mut config = DeviceConfig::new(creds)
            .channel(settings.send_channel)
            .timings(Timings::from_settings(&settings))
            .ip_mode(if record.auto_ip {
                IpMode::Auto
            } else {
                IpMode::Fixed
            });
        if let Some(caps) = resolved_capabilities(record) {
            config = config.capabilities(caps);
        }
        if let Some(tweak) = &self.options.device_tweak {
            config = tweak(&record.id, config);
        }
        let store: Arc<dyn StateStore> = Arc::new(CoreStateStore::new(self.store.clone()));
        Ok(spawn(config, self.locator.clone(), store))
    }

    /// Un foco tal como lo ve la UI.
    pub(crate) fn configured(&self, record: &DeviceRecord) -> ConfiguredDevice {
        ConfiguredDevice {
            id: record.id.clone(),
            name: record.name.clone(),
            ip: record.ip.clone(),
            auto_ip: record.auto_ip,
            version: record.version,
            capabilities: resolved_capabilities(record).unwrap_or_else(fallback_capabilities),
            zones: record.zones.clone(),
            overrides: record.overrides.clone(),
            calibrated: matches!(self.store.load_calibration(&record.id), Ok(Some(_))),
        }
    }

    // ---------- perfiles de fábrica ----------

    fn seed_profiles(&self) -> CmdResult<()> {
        let _edit = self.edit_lock();
        let mut config = self.config()?;
        let mut changed = false;
        for profile in factory_profiles() {
            if !config.profiles.iter().any(|p| p.id == profile.id) {
                config.profiles.push(profile);
                changed = true;
            }
        }
        if changed {
            self.store.save_config(&config)?;
        }
        Ok(())
    }
}

/// Capacidades de un foco: las guardadas o las que salen de su mapeo de la nube.
pub(crate) fn resolved_capabilities(record: &DeviceRecord) -> Option<Capabilities> {
    record
        .capabilities
        .clone()
        .or_else(|| record.mapping.as_ref().and_then(Capabilities::from_mapping))
}

/// Lo que se muestra de un foco sin capacidades conocidas (importado sin mapeo): un foco nuevo
/// de los habituales (DP 20–28). El actor deduce las de verdad al conectar.
fn fallback_capabilities() -> Capabilities {
    Capabilities {
        dp_switch: "20".into(),
        dp_mode: Some("21".into()),
        dp_bright: None,
        dp_temp: None,
        dp_colour: Some(("24".into(), ColourFormat::V2Hsv12)),
        dp_control: None,
        modes: vec!["white".into(), "colour".into()],
    }
}

/// Los perfiles de fábrica (Películas, Juegos, Trabajo), con los valores que muestra la UI.
pub fn factory_profiles() -> Vec<Profile> {
    use openlight_types::{SettingId, SettingValue};
    let values = |items: &[(SettingId, f64)]| {
        SettingsOverride(
            items
                .iter()
                .map(|(id, v)| (*id, SettingValue::Number(*v)))
                .collect(),
        )
    };
    vec![
        Profile {
            id: ProfileId("movies".into()),
            name: "profile.movies".into(),
            values: values(&[
                (SettingId::SmoothingMs, 500.0),
                (SettingId::Saturation, 1.4),
                (SettingId::BrightnessMax, 0.8),
            ]),
            shortcut: Some("CmdOrCtrl+Alt+1".into()),
            builtin: true,
        },
        Profile {
            id: ProfileId("games".into()),
            name: "profile.games".into(),
            values: values(&[
                (SettingId::SmoothingMs, 120.0),
                (SettingId::CaptureFps, 30.0),
                (SettingId::MaxSendsPerS, 10.0),
            ]),
            shortcut: Some("CmdOrCtrl+Alt+2".into()),
            builtin: true,
        },
        Profile {
            id: ProfileId("work".into()),
            name: "profile.work".into(),
            values: values(&[
                (SettingId::Saturation, 1.0),
                (SettingId::BrightnessMax, 0.5),
                (SettingId::SmoothingMs, 800.0),
            ]),
            shortcut: None,
            builtin: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn los_perfiles_de_fabrica_son_validos() {
        for profile in factory_profiles() {
            profile.values.validate().unwrap();
            assert!(profile.builtin);
            assert!(profile.name.starts_with("profile."));
        }
    }
}
