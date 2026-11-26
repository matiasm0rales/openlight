//! DTOs del contrato IPC (Tauri). Los comandos están en `app/src-tauri/src/ipc.rs`; los tipos de
//! TypeScript se generan en `app/ui/src/lib/bindings.ts`. Esta lista es cerrada: agregar o
//! cambiar algo pasa por el integrador (I-0).

use std::fmt;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{
    Capabilities, DeviceId, DeviceSessionState, DisplayId, Hsv, LinkHealth, PermissionState,
    ProfileId, ProtocolVersion, Region, ScreenColor, SessionState, SettingDef, Settings,
    SettingsOverride, SystemPane, Zone,
};

/// Nombres de los eventos que el backend emite hacia la UI.
pub mod events {
    /// Payload [`super::SessionStateEvent`].
    pub const SESSION_STATE: &str = "session://state";
    /// Payload [`super::DeviceHealthEvent`].
    pub const DEVICE_HEALTH: &str = "device://health";
    /// Payload [`super::LiveColorEvent`], como mucho 10 por segundo por foco.
    pub const COLOR_LIVE: &str = "color://live";
    /// Payload [`crate::UserFacingError`].
    pub const ERROR_RAISED: &str = "error://raised";
    /// Payload [`super::DiscoveredDevice`].
    pub const DISCOVERY_FOUND: &str = "discovery://found";
    /// Payload [`super::PipelineStatsEvent`], una vez por segundo mientras hay sesión.
    pub const PIPELINE_STATS: &str = "stats://pipeline";
    /// Payload [`super::EffectDevicesEvent`], cuando un foco sale del efecto o vuelve (desde la
    /// ventana o desde el tray).
    pub const EFFECT_DEVICES: &str = "effect://devices";
}

// ---------- Asistente ----------

/// Pasos del asistente inicial (sección 3.9 A).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OnboardingStep {
    /// 1. Bienvenida.
    Welcome,
    /// 2. Permisos del sistema.
    Permissions,
    /// 3. Cuenta en Tuya Developer.
    TuyaAccount,
    /// 4. Proyecto Cloud.
    CloudProject,
    /// 5. Vincular Smart Life.
    LinkApp,
    /// 6. Access ID y Access Secret.
    Credentials,
    /// 7. Elegir focos.
    ChooseDevices,
    /// 8. Probar cada foco.
    TestDevices,
    /// 9. Monitor y zonas.
    Zones,
    /// 10. Calibrar.
    Calibrate,
    /// 11. Listo.
    Done,
}

/// Progreso guardado del asistente (se puede pausar y retomar).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct OnboardingState {
    /// Paso actual.
    pub step: OnboardingStep,
    /// Pasos completados.
    pub completed: Vec<OnboardingStep>,
    /// Si ya hay credenciales de la nube guardadas.
    pub has_credentials: bool,
    /// Focos ya configurados.
    pub devices: Vec<DeviceId>,
}

/// Qué permiso del sistema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum PermissionKind {
    /// Grabación de pantalla (macOS).
    ScreenRecording,
    /// Red local (macOS).
    LocalNetwork,
    /// Firewall (Windows).
    Firewall,
    /// Portal ScreenCast (Wayland).
    ScreenCastPortal,
}

/// Estado de un permiso y dónde arreglarlo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct PermissionStatus {
    /// Permiso.
    pub kind: PermissionKind,
    /// Estado.
    pub state: PermissionState,
    /// Panel de ajustes para arreglarlo.
    pub pane: Option<SystemPane>,
    /// Si hay que reabrir la app después de concederlo (macOS, Grabación de pantalla).
    pub needs_restart: bool,
}

// ---------- Nube y dispositivos ----------

/// Credenciales tal como las escribe el usuario en el paso 6. `Debug` no muestra ninguna.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct CloudCredentialsInput {
    /// Access ID.
    pub access_id: String,
    /// Access Secret.
    pub access_secret: String,
    /// Región elegida (si está mal, el backend sugiere la correcta).
    pub region: Region,
}

impl fmt::Debug for CloudCredentialsInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CloudCredentialsInput")
            .field("region", &self.region)
            .finish_non_exhaustive()
    }
}

/// Un foco encontrado en la red local (evento `discovery://found`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct DiscoveredDevice {
    /// Id.
    pub id: DeviceId,
    /// IP en la red local.
    pub ip: String,
    /// Versión del protocolo que anuncia.
    pub version: ProtocolVersion,
    /// `productKey` del broadcast.
    pub product_key: Option<String>,
}

/// Un dispositivo de la nube para la lista del paso 7 (sin la clave local).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct CloudDeviceInfo {
    /// Id.
    pub id: DeviceId,
    /// Nombre en Smart Life.
    pub name: String,
    /// Categoría de Tuya.
    pub category: String,
    /// Product id.
    pub product_id: String,
    /// Si es una luz.
    pub is_light: bool,
    /// Si la nube lo ve conectado.
    pub online: bool,
    /// Capacidades, si se pudo leer el mapeo.
    pub capabilities: Option<Capabilities>,
    /// Si se lo encontró en la red local.
    pub lan: Option<DiscoveredDevice>,
}

/// Pedido para agregar un foco (la clave sale de la nube o de la importación).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct DeviceAddRequest {
    /// Id.
    pub id: DeviceId,
    /// Nombre para mostrar.
    pub name: String,
    /// IP fija, o `None` para buscarla automáticamente.
    pub ip: Option<String>,
}

/// Un foco configurado en la app.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct ConfiguredDevice {
    /// Id.
    pub id: DeviceId,
    /// Nombre.
    pub name: String,
    /// IP fija o última conocida.
    pub ip: Option<String>,
    /// Si la IP se busca automáticamente.
    pub auto_ip: bool,
    /// Protocolo.
    pub version: ProtocolVersion,
    /// Capacidades.
    pub capabilities: Capabilities,
    /// Zonas que sigue.
    pub zones: Vec<Zone>,
    /// Ajustes propios del foco.
    pub overrides: SettingsOverride,
    /// Si tiene calibración guardada.
    pub calibrated: bool,
    /// Si participa del efecto: el usuario no lo sacó. Sin zonas tampoco sigue la pantalla.
    pub in_effect: bool,
}

/// Patrón de prueba (paso 8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TestPattern {
    /// Rojo, verde y azul, 1.5 s cada uno (como `--probar`).
    RedGreenBlue,
    /// Parpadeo para identificar cuál es cuál.
    Blink,
}

/// Control manual (pantalla E).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ManualSet {
    /// Prender o apagar.
    Power(bool),
    /// Un color.
    Colour(Hsv),
    /// Blanco con brillo y temperatura en `0..1`.
    White {
        /// Brillo.
        brightness: f32,
        /// Temperatura (0 = cálido, 1 = frío).
        temperature: f32,
    },
    /// Una escena del foco, por su valor de DP.
    Scene(String),
}

// ---------- Pantallas ----------

/// Vista previa de un monitor para el paso 9.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct PreviewImage {
    /// Monitor.
    pub display: DisplayId,
    /// Ancho.
    pub width: u32,
    /// Alto.
    pub height: u32,
    /// PNG en base64.
    pub png_base64: String,
}

// ---------- Ajustes y perfiles ----------

/// Dónde se aplica un cambio de ajuste.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum SettingScope {
    /// Ajustes globales.
    Global,
    /// Un foco.
    Device(DeviceId),
    /// Un perfil.
    Profile(ProfileId),
}

/// Ajustes de un foco.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct DeviceOverrides {
    /// Foco.
    pub device: DeviceId,
    /// Lo que pisa.
    pub overrides: SettingsOverride,
}

/// Todo lo que necesita la pantalla de Ajustes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct SettingsSnapshot {
    /// Catálogo (para generar los controles).
    pub catalog: Vec<SettingDef>,
    /// Valores globales con el perfil activo aplicado.
    pub effective: Settings,
    /// Valores globales sin perfil.
    pub global: Settings,
    /// Lo que pisa cada foco.
    pub devices: Vec<DeviceOverrides>,
    /// Perfil activo.
    pub active_profile: Option<ProfileId>,
}

// ---------- Calibración ----------

/// Un punto de la tabla de calibración (`calibracion.json`: `{"30": {"tono": 26, "sat": 1.3}}`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
pub struct CalibrationPoint {
    /// Tono de pantalla, en grados (0, 30, ... 330).
    pub screen_hue_deg: f32,
    /// Tono que hay que pedirle al foco, en grados (puede ser negativo o pasar de 360).
    pub tono_deg: f32,
    /// Factor de saturación (0.3–1.5).
    pub sat: f32,
}

/// Sesión de calibración en curso.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct CalibrationState {
    /// Foco que se calibra.
    pub device: DeviceId,
    /// Monitor donde se muestran los tonos.
    pub display: DisplayId,
    /// Tabla (12 tonos, o 6 en modo rápido).
    pub points: Vec<CalibrationPoint>,
    /// Índice del tono que se está ajustando.
    pub index: u32,
}

// ---------- Sesión y eventos ----------

/// Evento `session://state`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct SessionStateEvent {
    /// Estado global.
    pub state: SessionState,
}

/// Evento `device://health`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct DeviceHealthEvent {
    /// Foco.
    pub device: DeviceId,
    /// Estado del foco en la sesión.
    pub state: DeviceSessionState,
    /// Salud del enlace.
    pub health: LinkHealth,
}

/// Evento `color://live`: lo que hay en pantalla y lo que se mandó.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct LiveColorEvent {
    /// Foco.
    pub device: DeviceId,
    /// Color de su zona.
    pub screen: ScreenColor,
    /// Último color enviado (después de calibración y suavizado).
    pub sent: Option<Hsv>,
}

/// Evento `effect://devices`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct EffectDevicesEvent {
    /// Los focos que el usuario sacó del efecto.
    pub excluded: Vec<DeviceId>,
}

/// Envíos por segundo de un foco (parte de [`PipelineStatsEvent`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct DeviceRate {
    /// Foco.
    pub device: DeviceId,
    /// Comandos de color mandados en el último segundo.
    pub sends_per_s: f32,
}

/// Evento `stats://pipeline`: lo que alimenta los gráficos del diagnóstico.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct PipelineStatsEvent {
    /// Capturas por segundo que llegaron de verdad (no las pedidas).
    pub capture_fps: f32,
    /// Envíos por segundo de cada foco.
    pub devices: Vec<DeviceRate>,
}

// ---------- Importar y diagnóstico ----------

/// Resultado de importar el setup del script (`devices.json`, `config.json`, `calibracion.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ImportReport {
    /// Focos importados (sus claves ya están en el llavero).
    pub devices: Vec<DeviceId>,
    /// Calibraciones importadas.
    pub calibrations: u32,
    /// Si se aplicaron ajustes de `config.json`.
    pub settings_applied: bool,
    /// Avisos (claves i18n).
    pub warnings: Vec<String>,
}

/// Nivel de una línea del log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    /// Error.
    Error,
    /// Advertencia.
    Warn,
    /// Información.
    Info,
    /// Detalle para depurar.
    Debug,
}

/// Una línea del log para el visor del diagnóstico (ya sin secretos).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct LogLine {
    /// Cuándo (ms desde la época Unix).
    pub at_ms: f64,
    /// Nivel.
    pub level: LogLevel,
    /// Módulo que la escribió (`tuya_device`, `capture`...).
    pub target: String,
    /// Mensaje.
    pub message: String,
}
