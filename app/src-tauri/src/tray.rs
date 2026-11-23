//! Tray (Windows y Linux) y barra de menú (macOS).
//!
//! Menú: estado, encender/apagar, perfil activo, un ítem por foco, "Abrir" y "Salir" (que
//! restaura los focos antes de cerrar).
//!
//! # Cómo se conecta
//!
//! El tray no conoce la sesión real: habla con un [`TrayBackend`]. La sesión real la arma I1a; el
//! integrador implementa el trait sobre ese estado y llama a [`install`]. Mientras tanto,
//! [`FakeTrayBackend`] sirve para probar y para ver el menú andando.
//!
//! El menú es una foto: cuando algo cambia (sesión, perfil, focos), el estado real tiene que
//! llamar a [`refresh`] para que el menú se vuelva a armar. Después de cada clic en el menú se
//! refresca solo.
//!
//! # Textos
//!
//! Las etiquetas no están en el código: salen de los mismos `es.json` / `en.json` que usa la UI
//! (claves `tray.*`, `session.*`, `dashboard.*`, `profile.*`), así el menú nativo y la ventana
//! dicen lo mismo. Una prueba verifica que todas las claves existan en los dos idiomas.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, OnceLock};

use openlight_types::{DeviceId, ProfileId, SessionState};
use serde_json::Value;
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime};

/// Id del ícono del tray (para encontrarlo desde [`refresh`]).
pub const TRAY_ID: &str = "openlight";

/// Nombre de la app en el menú. Es una marca, no un texto traducible.
const APP_NAME: &str = "OpenLight";

/// Ícono para Windows y Linux.
const ICON: &[u8] = include_bytes!("../icons/tray.png");
/// Ícono de la barra de menú de macOS: negro con transparencia, el sistema lo tiñe.
const ICON_TEMPLATE: &[u8] = include_bytes!("../icons/tray-template.png");

// ---------- Contrato con la sesión real ----------

/// Futuro con caja (para no depender de `async-trait` en un trait tan chico).
pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

/// Perfil tal como lo muestra el menú.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayProfile {
    /// Id.
    pub id: ProfileId,
    /// Nombre, o clave i18n (`profile.movies`) en los perfiles de fábrica.
    pub name: String,
}

/// Foco tal como lo muestra el menú.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayDevice {
    /// Id.
    pub id: DeviceId,
    /// Nombre del foco.
    pub name: String,
    /// Si el foco participa del efecto (tilde en el menú).
    pub enabled: bool,
}

/// Lo que el menú necesita leer para armarse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraySnapshot {
    /// Estado de la sesión.
    pub session: SessionState,
    /// Perfiles para elegir.
    pub profiles: Vec<TrayProfile>,
    /// Perfil activo (`None` = solo los ajustes globales).
    pub active_profile: Option<ProfileId>,
    /// Focos configurados.
    pub devices: Vec<TrayDevice>,
}

/// Lo que el tray le pide a la app. Las implementaciones no bloquean: arrancan la tarea y vuelven;
/// si algo falla, lo informan por el canal de errores de la UI, no devuelven nada al menú.
pub trait TrayBackend: Send + Sync + 'static {
    /// Estado actual para armar el menú.
    fn snapshot(&self) -> TraySnapshot;
    /// Arranca el efecto.
    fn start(&self);
    /// Detiene el efecto y devuelve los focos a su estado.
    fn stop(&self);
    /// Activa un perfil (`None` = ninguno).
    fn select_profile(&self, profile: Option<ProfileId>);
    /// Prende o apaga un foco dentro del efecto.
    fn toggle_device(&self, device: &DeviceId);
    /// Restaura los focos y termina. El tray cierra la app cuando el futuro se completa.
    fn restore_and_quit(&self) -> BoxFuture<()>;
}

// ---------- Textos ----------

/// Idioma del menú.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locale {
    /// Español.
    Es,
    /// Inglés.
    En,
}

impl Locale {
    /// Idioma de una etiqueta BCP-47 (`es-AR`, `en_US`): español si empieza con `es`, si no
    /// inglés (la misma regla que la UI).
    pub fn from_tag(tag: &str) -> Self {
        if tag.to_ascii_lowercase().starts_with("es") {
            Self::Es
        } else {
            Self::En
        }
    }

    /// Idioma del sistema.
    pub fn detect() -> Self {
        sys_locale::get_locale().map_or(Self::Es, |tag| Self::from_tag(&tag))
    }
}

/// Textos de un idioma, leídos de los JSON de la UI.
#[derive(Debug, Clone, Copy)]
pub struct Labels {
    locale: Locale,
}

static ES: OnceLock<Value> = OnceLock::new();
static EN: OnceLock<Value> = OnceLock::new();

fn tree(locale: Locale) -> &'static Value {
    let (cell, raw) = match locale {
        Locale::Es => (&ES, include_str!("../../ui/src/lib/i18n/es.json")),
        Locale::En => (&EN, include_str!("../../ui/src/lib/i18n/en.json")),
    };
    cell.get_or_init(|| serde_json::from_str(raw).unwrap_or(Value::Null))
}

fn lookup(locale: Locale, key: &str) -> Option<&'static str> {
    key.split('.')
        .try_fold(tree(locale), |node, part| node.get(part))?
        .as_str()
}

impl Labels {
    /// Textos en `locale`.
    pub fn new(locale: Locale) -> Self {
        Self { locale }
    }

    /// Texto de una clave; si falta en el idioma, el español; si falta en los dos, la clave.
    pub fn get(&self, key: &str) -> String {
        lookup(self.locale, key)
            .or_else(|| lookup(Locale::Es, key))
            .unwrap_or(key)
            .to_owned()
    }

    /// Nombre de un perfil: los de fábrica traen una clave (`profile.movies`), los propios el
    /// nombre tal cual.
    fn profile_name(&self, name: &str) -> String {
        if name.starts_with("profile.") {
            self.get(name)
        } else {
            name.to_owned()
        }
    }
}

/// Claves que usa el menú (las verifica una prueba contra los dos idiomas).
mod key {
    pub const START: &str = "session.start";
    pub const STOP: &str = "session.stop";
    pub const PROFILE: &str = "dashboard.profile";
    pub const NO_PROFILE: &str = "dashboard.no_profile";
    pub const DEVICES: &str = "tray.devices";
    pub const OPEN: &str = "tray.open";
    pub const QUIT: &str = "tray.quit";

    pub fn session_state(state: openlight_types::SessionState) -> &'static str {
        use openlight_types::SessionState::{Active, Connecting, Idle, Paused, Restoring};
        match state {
            Idle => "session.state.idle",
            Connecting => "session.state.connecting",
            Active => "session.state.active",
            Paused => "session.state.paused",
            Restoring => "session.state.restoring",
        }
    }
}

// ---------- Modelo del menú (puro, sin Tauri) ----------

/// Qué pasa cuando se elige un ítem del menú.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuAction {
    /// Encender o apagar, según el estado.
    ToggleSession,
    /// Elegir un perfil (`None` = sin perfil).
    SelectProfile(Option<ProfileId>),
    /// Prender o apagar un foco.
    ToggleDevice(DeviceId),
    /// Mostrar la ventana principal.
    Open,
    /// Restaurar los focos y salir.
    Quit,
}

impl MenuAction {
    /// Id del ítem en el menú nativo.
    pub fn id(&self) -> String {
        match self {
            Self::ToggleSession => "toggle_session".into(),
            Self::SelectProfile(None) => "profile-none".into(),
            Self::SelectProfile(Some(id)) => format!("profile:{}", id.0),
            Self::ToggleDevice(id) => format!("device:{}", id.0),
            Self::Open => "open".into(),
            Self::Quit => "quit".into(),
        }
    }

    /// Acción de un id de ítem (la inversa de [`MenuAction::id`]).
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "toggle_session" => Some(Self::ToggleSession),
            "profile-none" => Some(Self::SelectProfile(None)),
            "open" => Some(Self::Open),
            "quit" => Some(Self::Quit),
            _ => {
                if let Some(p) = id.strip_prefix("profile:") {
                    Some(Self::SelectProfile(Some(ProfileId(p.to_owned()))))
                } else {
                    id.strip_prefix("device:")
                        .map(|d| Self::ToggleDevice(DeviceId(d.to_owned())))
                }
            }
        }
    }
}

/// Un renglón del menú, independiente de Tauri.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuEntry {
    /// Texto sin acción (apagado).
    Label(String),
    /// Ítem que hace algo.
    Action {
        /// Qué hace.
        action: MenuAction,
        /// Texto.
        text: String,
        /// Si se puede elegir.
        enabled: bool,
    },
    /// Ítem con tilde.
    Check {
        /// Qué hace.
        action: MenuAction,
        /// Texto.
        text: String,
        /// Si está tildado.
        checked: bool,
    },
    /// Línea separadora.
    Separator,
    /// Submenú.
    Submenu {
        /// Texto.
        text: String,
        /// Ítems.
        items: Vec<MenuEntry>,
    },
}

/// Si el efecto está andando o arrancando (el botón pasa a "Detener").
fn is_running(state: SessionState) -> bool {
    !matches!(state, SessionState::Idle)
}

/// Arma el menú a partir del estado y los textos.
pub fn build_model(snap: &TraySnapshot, labels: &Labels) -> Vec<MenuEntry> {
    let mut menu = vec![
        MenuEntry::Label(format!(
            "{APP_NAME} — {}",
            labels.get(key::session_state(snap.session))
        )),
        MenuEntry::Action {
            action: MenuAction::ToggleSession,
            text: labels.get(if is_running(snap.session) {
                key::STOP
            } else {
                key::START
            }),
            // Mientras restaura no se puede pedir otra cosa.
            enabled: snap.session != SessionState::Restoring,
        },
        MenuEntry::Separator,
    ];

    if !snap.profiles.is_empty() {
        let mut items = vec![MenuEntry::Check {
            action: MenuAction::SelectProfile(None),
            text: labels.get(key::NO_PROFILE),
            checked: snap.active_profile.is_none(),
        }];
        items.extend(snap.profiles.iter().map(|p| MenuEntry::Check {
            action: MenuAction::SelectProfile(Some(p.id.clone())),
            text: labels.profile_name(&p.name),
            checked: snap.active_profile.as_ref() == Some(&p.id),
        }));
        menu.push(MenuEntry::Submenu {
            text: labels.get(key::PROFILE),
            items,
        });
    }

    if !snap.devices.is_empty() {
        menu.push(MenuEntry::Label(labels.get(key::DEVICES)));
        menu.extend(snap.devices.iter().map(|d| MenuEntry::Check {
            action: MenuAction::ToggleDevice(d.id.clone()),
            text: d.name.clone(),
            checked: d.enabled,
        }));
    }

    menu.push(MenuEntry::Separator);
    menu.push(MenuEntry::Action {
        action: MenuAction::Open,
        text: labels.get(key::OPEN),
        enabled: true,
    });
    menu.push(MenuEntry::Action {
        action: MenuAction::Quit,
        text: labels.get(key::QUIT),
        enabled: true,
    });
    menu
}

/// Lo que queda por hacer después de [`handle`]: lo que necesita la app y no el backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Nada más.
    None,
    /// Mostrar la ventana principal.
    ShowMainWindow,
    /// Restaurar los focos (`backend.restore_and_quit()`) y cerrar la app.
    Quit,
}

/// Aplica una acción del menú sobre el backend.
pub fn handle(backend: &dyn TrayBackend, action: MenuAction) -> Effect {
    match action {
        MenuAction::ToggleSession => match backend.snapshot().session {
            SessionState::Idle => backend.start(),
            SessionState::Restoring => {}
            SessionState::Connecting | SessionState::Active | SessionState::Paused => {
                backend.stop();
            }
        },
        MenuAction::SelectProfile(profile) => backend.select_profile(profile),
        MenuAction::ToggleDevice(device) => backend.toggle_device(&device),
        MenuAction::Open => return Effect::ShowMainWindow,
        MenuAction::Quit => return Effect::Quit,
    }
    Effect::None
}

// ---------- Tauri ----------

/// El backend, guardado como estado de la app para que [`refresh`] lo encuentre.
struct TrayState(Arc<dyn TrayBackend>);

/// Crea el ícono del tray con su menú y lo deja funcionando.
pub fn install<R: Runtime>(app: &AppHandle<R>, backend: Arc<dyn TrayBackend>) -> tauri::Result<()> {
    let model = build_model(&backend.snapshot(), &Labels::new(Locale::detect()));
    let menu = build_menu(app, &model)?;

    let handler_backend = Arc::clone(&backend);
    let builder = TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon()?)
        .icon_as_template(cfg!(target_os = "macos"))
        .tooltip(APP_NAME)
        .menu(&menu)
        // En macOS el menú se abre con cualquier clic; en Windows el clic izquierdo abre la
        // ventana y el derecho el menú (lo habitual en esos sistemas).
        .show_menu_on_left_click(cfg!(target_os = "macos"))
        .on_menu_event(move |app, event| {
            let Some(action) = MenuAction::from_id(event.id().as_ref()) else {
                return;
            };
            match handle(handler_backend.as_ref(), action) {
                Effect::None => refresh(app),
                Effect::ShowMainWindow => {
                    let _ = crate::windows::show_main(app);
                }
                Effect::Quit => {
                    let app = app.clone();
                    let quit = handler_backend.restore_and_quit();
                    tauri::async_runtime::spawn(async move {
                        quit.await;
                        app.exit(0);
                    });
                }
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let _ = crate::windows::show_main(tray.app_handle());
            }
        });
    builder.build(app)?;

    app.manage(TrayState(backend));
    Ok(())
}

/// Vuelve a armar el menú con el estado actual. Lo llama el estado real cada vez que cambia la
/// sesión, el perfil o los focos. No hace nada si el tray no está instalado.
pub fn refresh<R: Runtime>(app: &AppHandle<R>) {
    let (Some(state), Some(tray)) = (app.try_state::<TrayState>(), app.tray_by_id(TRAY_ID)) else {
        return;
    };
    let snapshot = state.0.snapshot();
    let labels = Labels::new(Locale::detect());
    if let Ok(menu) = build_menu(app, &build_model(&snapshot, &labels)) {
        let _ = tray.set_menu(Some(menu));
    }
    let estado = labels.get(key::session_state(snapshot.session));
    let _ = tray.set_tooltip(Some(format!("{APP_NAME} — {estado}")));
}

/// Ícono del sistema actual: la versión template en macOS, la de color en los demás.
fn icon() -> tauri::Result<Image<'static>> {
    Image::from_bytes(if cfg!(target_os = "macos") {
        ICON_TEMPLATE
    } else {
        ICON
    })
}

/// Convierte el modelo en un menú nativo.
pub fn build_menu<R: Runtime>(app: &AppHandle<R>, model: &[MenuEntry]) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;
    for entry in model {
        menu.append(&*native(app, entry)?)?;
    }
    Ok(menu)
}

fn native<R: Runtime>(
    app: &AppHandle<R>,
    entry: &MenuEntry,
) -> tauri::Result<Box<dyn IsMenuItem<R>>> {
    Ok(match entry {
        MenuEntry::Label(text) => Box::new(MenuItem::new(app, text, false, None::<&str>)?),
        MenuEntry::Action {
            action,
            text,
            enabled,
        } => Box::new(MenuItem::with_id(
            app,
            action.id(),
            text,
            *enabled,
            None::<&str>,
        )?),
        MenuEntry::Check {
            action,
            text,
            checked,
        } => Box::new(CheckMenuItem::with_id(
            app,
            action.id(),
            text,
            true,
            *checked,
            None::<&str>,
        )?),
        MenuEntry::Separator => Box::new(PredefinedMenuItem::separator(app)?),
        MenuEntry::Submenu { text, items } => {
            let submenu = Submenu::new(app, text, true)?;
            for item in items {
                submenu.append(&*native(app, item)?)?;
            }
            Box::new(submenu)
        }
    })
}

// ---------- Backend falso ----------

/// Llamada que recibió un [`FakeTrayBackend`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call {
    /// `start`.
    Start,
    /// `stop`.
    Stop,
    /// `select_profile`.
    SelectProfile(Option<ProfileId>),
    /// `toggle_device`.
    ToggleDevice(DeviceId),
    /// `restore_and_quit`.
    RestoreAndQuit,
}

/// Backend de mentira: guarda un estado en memoria que cambia con cada llamada y las anota. Sirve
/// para las pruebas y para ver el menú andando mientras no está la sesión real.
pub struct FakeTrayBackend {
    state: Mutex<TraySnapshot>,
    calls: Mutex<Vec<Call>>,
}

impl FakeTrayBackend {
    /// Falso con el estado inicial dado.
    pub fn new(snapshot: TraySnapshot) -> Self {
        Self {
            state: Mutex::new(snapshot),
            calls: Mutex::new(Vec::new()),
        }
    }

    /// Falso con dos focos y los tres perfiles de fábrica, apagado.
    pub fn demo() -> Self {
        Self::new(TraySnapshot {
            session: SessionState::Idle,
            profiles: ["movies", "games", "work"]
                .into_iter()
                .map(|p| TrayProfile {
                    id: ProfileId(p.to_owned()),
                    name: format!("profile.{p}"),
                })
                .collect(),
            active_profile: None,
            devices: (1..=2)
                .map(|n| TrayDevice {
                    id: DeviceId(format!("bulb-{n}")),
                    name: format!("Foco {n}"),
                    enabled: true,
                })
                .collect(),
        })
    }

    /// Llamadas recibidas, en orden.
    pub fn calls(&self) -> Vec<Call> {
        self.calls.lock().map(|c| c.clone()).unwrap_or_default()
    }

    fn record(&self, call: Call) {
        if let Ok(mut calls) = self.calls.lock() {
            calls.push(call);
        }
    }

    fn with_state(&self, f: impl FnOnce(&mut TraySnapshot)) {
        if let Ok(mut state) = self.state.lock() {
            f(&mut state);
        }
    }
}

impl TrayBackend for FakeTrayBackend {
    fn snapshot(&self) -> TraySnapshot {
        self.state.lock().expect("estado envenenado").clone()
    }

    fn start(&self) {
        self.record(Call::Start);
        self.with_state(|s| s.session = SessionState::Active);
    }

    fn stop(&self) {
        self.record(Call::Stop);
        self.with_state(|s| s.session = SessionState::Idle);
    }

    fn select_profile(&self, profile: Option<ProfileId>) {
        self.record(Call::SelectProfile(profile.clone()));
        self.with_state(|s| s.active_profile = profile);
    }

    fn toggle_device(&self, device: &DeviceId) {
        self.record(Call::ToggleDevice(device.clone()));
        self.with_state(|s| {
            if let Some(d) = s.devices.iter_mut().find(|d| &d.id == device) {
                d.enabled = !d.enabled;
            }
        });
    }

    fn restore_and_quit(&self) -> BoxFuture<()> {
        self.record(Call::RestoreAndQuit);
        Box::pin(std::future::ready(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn es() -> Labels {
        Labels::new(Locale::Es)
    }

    fn textos(model: &[MenuEntry]) -> Vec<String> {
        model
            .iter()
            .map(|e| match e {
                MenuEntry::Label(t)
                | MenuEntry::Action { text: t, .. }
                | MenuEntry::Check { text: t, .. }
                | MenuEntry::Submenu { text: t, .. } => t.clone(),
                MenuEntry::Separator => "---".into(),
            })
            .collect()
    }

    #[test]
    fn menu_de_la_demo_apagada() {
        let fake = FakeTrayBackend::demo();
        let model = build_model(&fake.snapshot(), &es());
        assert_eq!(
            textos(&model),
            [
                "OpenLight — apagado",
                "Arrancar",
                "---",
                "Perfil",
                "Focos",
                "Foco 1",
                "Foco 2",
                "---",
                "Abrir OpenLight",
                "Salir (restaura los focos)",
            ]
        );
    }

    #[test]
    fn el_boton_cambia_con_el_estado_de_la_sesion() {
        let fake = FakeTrayBackend::demo();
        let mut snap = fake.snapshot();
        for (state, texto, habilitado) in [
            (SessionState::Idle, "Arrancar", true),
            (SessionState::Connecting, "Detener", true),
            (SessionState::Active, "Detener", true),
            (SessionState::Paused, "Detener", true),
            (SessionState::Restoring, "Detener", false),
        ] {
            snap.session = state;
            let model = build_model(&snap, &es());
            assert_eq!(
                model[1],
                MenuEntry::Action {
                    action: MenuAction::ToggleSession,
                    text: texto.into(),
                    enabled: habilitado
                },
                "{state:?}"
            );
        }
    }

    #[test]
    fn el_perfil_activo_lleva_tilde_y_los_de_fabrica_se_traducen() {
        let fake = FakeTrayBackend::demo();
        let mut snap = fake.snapshot();
        snap.active_profile = Some(ProfileId("games".into()));
        let MenuEntry::Submenu { text, items } = build_model(&snap, &es())[3].clone() else {
            panic!("falta el submenú de perfiles");
        };
        assert_eq!(text, "Perfil");
        let marcados: Vec<_> = items
            .iter()
            .filter_map(|i| match i {
                MenuEntry::Check { text, checked, .. } => Some((text.as_str(), *checked)),
                _ => None,
            })
            .collect();
        assert_eq!(
            marcados,
            [
                ("Sin perfil", false),
                ("Películas", false),
                ("Juegos", true),
                ("Trabajo", false)
            ]
        );
    }

    #[test]
    fn sin_perfil_activo_se_tilda_sin_perfil() {
        let snap = FakeTrayBackend::demo().snapshot();
        let MenuEntry::Submenu { items, .. } = build_model(&snap, &es())[3].clone() else {
            panic!("falta el submenú de perfiles");
        };
        assert!(matches!(
            items[0],
            MenuEntry::Check {
                checked: true,
                action: MenuAction::SelectProfile(None),
                ..
            }
        ));
    }

    #[test]
    fn un_item_por_foco_con_su_tilde() {
        let fake = FakeTrayBackend::demo();
        fake.toggle_device(&DeviceId("bulb-2".into()));
        let model = build_model(&fake.snapshot(), &es());
        let focos: Vec<_> = model
            .iter()
            .filter_map(|e| match e {
                MenuEntry::Check {
                    action: MenuAction::ToggleDevice(id),
                    text,
                    checked,
                } => Some((id.0.as_str(), text.as_str(), *checked)),
                _ => None,
            })
            .collect();
        assert_eq!(
            focos,
            [("bulb-1", "Foco 1", true), ("bulb-2", "Foco 2", false)]
        );
    }

    #[test]
    fn sin_focos_ni_perfiles_el_menu_queda_corto() {
        let snap = TraySnapshot {
            session: SessionState::Idle,
            profiles: Vec::new(),
            active_profile: None,
            devices: Vec::new(),
        };
        assert_eq!(
            textos(&build_model(&snap, &es())),
            [
                "OpenLight — apagado",
                "Arrancar",
                "---",
                "---",
                "Abrir OpenLight",
                "Salir (restaura los focos)"
            ]
        );
    }

    #[test]
    fn el_menu_sale_en_ingles() {
        let snap = FakeTrayBackend::demo().snapshot();
        let model = build_model(&snap, &Labels::new(Locale::En));
        let t = textos(&model);
        assert_eq!(t[0], "OpenLight — off");
        assert_eq!(t[1], "Start");
        assert_eq!(t[t.len() - 1], "Quit (restores the bulbs)");
    }

    #[test]
    fn el_idioma_sale_de_la_etiqueta_del_sistema() {
        assert_eq!(Locale::from_tag("es-AR"), Locale::Es);
        assert_eq!(Locale::from_tag("es_ES"), Locale::Es);
        assert_eq!(Locale::from_tag("en-US"), Locale::En);
        assert_eq!(Locale::from_tag("pt-BR"), Locale::En);
    }

    #[test]
    fn todas_las_claves_del_menu_existen_en_los_dos_idiomas() {
        let mut claves = vec![
            key::START,
            key::STOP,
            key::PROFILE,
            key::NO_PROFILE,
            key::DEVICES,
            key::OPEN,
            key::QUIT,
        ];
        use SessionState::{Active, Connecting, Idle, Paused, Restoring};
        claves.extend([Idle, Connecting, Active, Paused, Restoring].map(key::session_state));
        for locale in [Locale::Es, Locale::En] {
            for clave in &claves {
                assert!(
                    lookup(locale, clave).is_some(),
                    "falta {clave} en {locale:?}"
                );
            }
            for perfil in ["movies", "games", "work"] {
                let clave = format!("profile.{perfil}");
                assert!(
                    lookup(locale, &clave).is_some(),
                    "falta {clave} en {locale:?}"
                );
            }
        }
    }

    #[test]
    fn los_ids_de_los_items_van_y_vuelven() {
        for action in [
            MenuAction::ToggleSession,
            MenuAction::Open,
            MenuAction::Quit,
            MenuAction::SelectProfile(None),
            MenuAction::SelectProfile(Some(ProfileId("trabajo:2".into()))),
            MenuAction::ToggleDevice(DeviceId("bf12:ab".into())),
        ] {
            assert_eq!(MenuAction::from_id(&action.id()), Some(action));
        }
        assert_eq!(MenuAction::from_id("otra-cosa"), None);
    }

    #[test]
    fn arrancar_y_detener_segun_el_estado() {
        let fake = FakeTrayBackend::demo();
        assert_eq!(handle(&fake, MenuAction::ToggleSession), Effect::None);
        assert_eq!(fake.snapshot().session, SessionState::Active);
        assert_eq!(handle(&fake, MenuAction::ToggleSession), Effect::None);
        assert_eq!(fake.snapshot().session, SessionState::Idle);
        assert_eq!(fake.calls(), [Call::Start, Call::Stop]);
    }

    #[test]
    fn mientras_restaura_no_hace_nada() {
        let mut snap = FakeTrayBackend::demo().snapshot();
        snap.session = SessionState::Restoring;
        let fake = FakeTrayBackend::new(snap);
        handle(&fake, MenuAction::ToggleSession);
        assert_eq!(fake.calls(), []);
    }

    #[test]
    fn elegir_perfil_y_alternar_foco_llegan_al_backend() {
        let fake = FakeTrayBackend::demo();
        handle(
            &fake,
            MenuAction::SelectProfile(Some(ProfileId("work".into()))),
        );
        handle(&fake, MenuAction::ToggleDevice(DeviceId("bulb-1".into())));
        handle(&fake, MenuAction::SelectProfile(None));
        assert_eq!(
            fake.calls(),
            [
                Call::SelectProfile(Some(ProfileId("work".into()))),
                Call::ToggleDevice(DeviceId("bulb-1".into())),
                Call::SelectProfile(None),
            ]
        );
    }

    #[test]
    fn abrir_y_salir_los_resuelve_la_app() {
        let fake = FakeTrayBackend::demo();
        assert_eq!(handle(&fake, MenuAction::Open), Effect::ShowMainWindow);
        assert_eq!(handle(&fake, MenuAction::Quit), Effect::Quit);
        // Salir no restaura por sí solo: el futuro de `restore_and_quit` lo pide la app.
        assert_eq!(fake.calls(), []);
    }

    #[test]
    fn restore_and_quit_se_completa() {
        let fake = FakeTrayBackend::demo();
        tauri::async_runtime::block_on(fake.restore_and_quit());
        assert_eq!(fake.calls(), [Call::RestoreAndQuit]);
    }

    #[test]
    fn los_iconos_son_png_validos() {
        for bytes in [ICON, ICON_TEMPLATE] {
            assert!(Image::from_bytes(bytes).is_ok());
        }
    }
}
