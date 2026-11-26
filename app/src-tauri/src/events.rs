//! Eventos del backend hacia la UI.
//!
//! El backend no conoce a Tauri: emite [`AppEvent`] a un [`EventSink`]. En la app real el sink es
//! [`TauriSink`] (que hace `emit` con los nombres de `openlight_types::ipc::events`); en las pruebas
//! es un [`MemorySink`] que guarda todo lo emitido.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use openlight_types::ipc::{
    DeviceHealthEvent, DiscoveredDevice, EffectDevicesEvent, LiveColorEvent, PipelineStatsEvent,
    SessionStateEvent, events,
};
use openlight_types::{DeviceId, UserFacingError};
use tauri::Emitter;
use tokio::sync::broadcast;

/// Un evento del contrato IPC con su payload.
#[derive(Debug, Clone, PartialEq)]
pub enum AppEvent {
    /// `session://state`.
    SessionState(SessionStateEvent),
    /// `device://health`.
    DeviceHealth(DeviceHealthEvent),
    /// `color://live`.
    ColorLive(LiveColorEvent),
    /// `stats://pipeline`.
    PipelineStats(PipelineStatsEvent),
    /// `error://raised`.
    ErrorRaised(UserFacingError),
    /// `discovery://found`.
    DiscoveryFound(DiscoveredDevice),
    /// `effect://devices`.
    EffectDevices(EffectDevicesEvent),
}

impl AppEvent {
    /// Nombre del evento en el IPC.
    pub const fn name(&self) -> &'static str {
        match self {
            Self::SessionState(_) => events::SESSION_STATE,
            Self::DeviceHealth(_) => events::DEVICE_HEALTH,
            Self::ColorLive(_) => events::COLOR_LIVE,
            Self::PipelineStats(_) => events::PIPELINE_STATS,
            Self::ErrorRaised(_) => events::ERROR_RAISED,
            Self::DiscoveryFound(_) => events::DISCOVERY_FOUND,
            Self::EffectDevices(_) => events::EFFECT_DEVICES,
        }
    }
}

/// A dónde van los eventos.
pub trait EventSink: Send + Sync {
    /// Publica un evento. No bloquea y nunca falla: sin nadie escuchando no pasa nada.
    fn emit(&self, event: AppEvent);
}

/// Cuántos errores se guardan mientras no hay quien los muestre (lo mismo que apila la UI).
pub const MAX_PENDING_ERRORS: usize = 20;

/// La puerta hacia la UI. Con ninguna ventana a la vista, lo que solo sirve para dibujar (la
/// salud de cada foco, el color en vivo y las estadísticas) no sale: cada evento le cuesta CPU y
/// memoria a la ventana, la mire alguien o no. Todo lo demás (la sesión, los errores, la
/// búsqueda, qué focos están en el efecto) pasa siempre.
///
/// Se guarda lo último de cada foco y, cuando una ventana vuelve a verse, se manda junto con el
/// estado de la sesión, así la UI no queda vieja.
///
/// La ventana principal se destruye al cerrarla y la próxima arranca de cero: cuando su página
/// avisa que ya escucha ([`UiGate::attach`]) se le manda todo eso otra vez, más los errores que
/// salieron mientras no había nadie para mostrarlos.
pub struct UiGate {
    sink: Arc<dyn EventSink>,
    ui: Mutex<Ui>,
}

struct Ui {
    visible: bool,
    /// Si la página de la ventana principal está escuchando.
    attached: bool,
    /// Errores que salieron sin nadie escuchando, en orden.
    pending: Vec<UserFacingError>,
    session: Option<SessionStateEvent>,
    health: BTreeMap<DeviceId, DeviceHealthEvent>,
    live: BTreeMap<DeviceId, LiveColorEvent>,
}

impl UiGate {
    /// Una puerta abierta (con la UI a la vista) delante de `sink`.
    pub fn new(sink: Arc<dyn EventSink>) -> Self {
        Self {
            sink,
            ui: Mutex::new(Ui {
                visible: true,
                attached: false,
                pending: Vec::new(),
                session: None,
                health: BTreeMap::new(),
                live: BTreeMap::new(),
            }),
        }
    }

    /// Avisa si hay alguna ventana a la vista. Al pasar de ninguna a alguna, manda el estado.
    pub fn set_visible(&self, visible: bool) {
        let mut ui = self.ui.lock().unwrap_or_else(PoisonError::into_inner);
        if std::mem::replace(&mut ui.visible, visible) || !visible {
            return;
        }
        // Con el candado tomado: un evento nuevo no puede salir antes que esta foto y quedar
        // pisado por ella
        self.snapshot(&ui);
    }

    /// La página de la ventana principal ya escucha los eventos: se le manda el estado completo y
    /// los errores que no vio.
    pub fn attach(&self) {
        let mut ui = self.ui.lock().unwrap_or_else(PoisonError::into_inner);
        ui.attached = true;
        self.snapshot(&ui);
        for error in std::mem::take(&mut ui.pending) {
            self.sink.emit(AppEvent::ErrorRaised(error));
        }
    }

    /// La ventana principal se destruyó: desde ahora los errores se guardan.
    pub fn detach(&self) {
        self.ui
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .attached = false;
    }

    /// La sesión y lo último de cada foco.
    fn snapshot(&self, ui: &Ui) {
        if let Some(session) = &ui.session {
            self.sink.emit(AppEvent::SessionState(session.clone()));
        }
        for health in ui.health.values() {
            self.sink.emit(AppEvent::DeviceHealth(health.clone()));
        }
        for live in ui.live.values() {
            self.sink.emit(AppEvent::ColorLive(live.clone()));
        }
    }
}

impl EventSink for UiGate {
    fn emit(&self, event: AppEvent) {
        let mut ui = self.ui.lock().unwrap_or_else(PoisonError::into_inner);
        let drawing = match &event {
            AppEvent::SessionState(p) => {
                ui.session = Some(p.clone());
                false
            }
            AppEvent::DeviceHealth(p) => {
                ui.health.insert(p.device.clone(), p.clone());
                true
            }
            AppEvent::ColorLive(p) => {
                ui.live.insert(p.device.clone(), p.clone());
                true
            }
            AppEvent::PipelineStats(_) => true,
            AppEvent::ErrorRaised(error) => {
                if !ui.attached {
                    keep(&mut ui.pending, error);
                }
                false
            }
            // El resto, y lo que se agregue, pasa siempre
            _ => false,
        };
        if ui.visible || !drawing {
            self.sink.emit(event);
        }
    }
}

/// Suma `error` a los pendientes, sin repetidos (como la pila de la UI) y sin pasar del tope.
fn keep(pending: &mut Vec<UserFacingError>, error: &UserFacingError) {
    if pending
        .iter()
        .any(|e| e.code == error.code && e.params == error.params)
    {
        return;
    }
    pending.push(error.clone());
    if pending.len() > MAX_PENDING_ERRORS {
        pending.remove(0);
    }
}

/// El sink de la app real: cada evento sale por `AppHandle::emit` hacia todas las ventanas.
pub struct TauriSink<R: tauri::Runtime = tauri::Wry>(pub tauri::AppHandle<R>);

impl<R: tauri::Runtime> EventSink for TauriSink<R> {
    fn emit(&self, event: AppEvent) {
        let name = event.name();
        let sent = match event {
            AppEvent::SessionState(p) => self.0.emit(name, p),
            AppEvent::DeviceHealth(p) => self.0.emit(name, p),
            AppEvent::ColorLive(p) => self.0.emit(name, p),
            AppEvent::PipelineStats(p) => self.0.emit(name, p),
            AppEvent::ErrorRaised(p) => self.0.emit(name, p),
            AppEvent::DiscoveryFound(p) => self.0.emit(name, p),
            AppEvent::EffectDevices(p) => self.0.emit(name, p),
        };
        if let Err(error) = sent {
            tracing::debug!(event = name, %error, "no se pudo emitir el evento");
        }
    }
}

/// Un sink que guarda lo emitido y además lo reparte por un canal, para las pruebas.
#[derive(Debug, Clone)]
pub struct MemorySink {
    log: Arc<Mutex<Vec<AppEvent>>>,
    tx: broadcast::Sender<AppEvent>,
}

impl Default for MemorySink {
    fn default() -> Self {
        Self::new()
    }
}

impl MemorySink {
    /// Un sink vacío.
    pub fn new() -> Self {
        Self {
            log: Arc::default(),
            tx: broadcast::channel(1024).0,
        }
    }

    /// Todo lo emitido hasta ahora, en orden.
    pub fn events(&self) -> Vec<AppEvent> {
        self.log
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Los eventos que se emitan de ahora en adelante.
    pub fn subscribe(&self) -> broadcast::Receiver<AppEvent> {
        self.tx.subscribe()
    }
}

impl EventSink for MemorySink {
    fn emit(&self, event: AppEvent) {
        self.log
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(event.clone());
        let _ = self.tx.send(event);
    }
}

#[cfg(test)]
mod tests {
    use openlight_types::{
        DeviceSessionState, ErrorCode, LinkHealth, ProtocolVersion, ScreenColor, SessionState,
    };

    use super::*;

    #[test]
    fn cada_evento_usa_su_nombre_del_contrato() {
        let event = AppEvent::SessionState(SessionStateEvent {
            state: SessionState::Idle,
        });
        assert_eq!(event.name(), "session://state");
    }

    fn color(device: &str, luz: f32) -> AppEvent {
        AppEvent::ColorLive(LiveColorEvent {
            device: DeviceId(device.into()),
            screen: ScreenColor {
                h: None,
                s: 0.0,
                luz,
            },
            sent: None,
        })
    }

    fn salud(device: &str, state: DeviceSessionState) -> AppEvent {
        AppEvent::DeviceHealth(DeviceHealthEvent {
            device: DeviceId(device.into()),
            state,
            health: LinkHealth::default(),
        })
    }

    fn sesion(state: SessionState) -> AppEvent {
        AppEvent::SessionState(SessionStateEvent { state })
    }

    fn estadisticas() -> AppEvent {
        AppEvent::PipelineStats(PipelineStatsEvent {
            capture_fps: 20.0,
            devices: Vec::new(),
        })
    }

    fn puerta() -> (MemorySink, UiGate) {
        let sink = MemorySink::new();
        let gate = UiGate::new(Arc::new(sink.clone()));
        (sink, gate)
    }

    #[test]
    fn a_la_vista_pasa_todo() {
        let (sink, gate) = puerta();
        let eventos = [
            sesion(SessionState::Active),
            salud("a", DeviceSessionState::Active),
            color("a", 0.5),
            estadisticas(),
        ];
        for e in &eventos {
            gate.emit(e.clone());
        }
        assert_eq!(sink.events(), eventos);
    }

    #[test]
    fn oculta_solo_pasan_la_sesion_los_errores_y_la_busqueda() {
        let (sink, gate) = puerta();
        gate.set_visible(false);
        let error = AppEvent::ErrorRaised(UserFacingError::new(ErrorCode::NoZones));
        let encontrado = AppEvent::DiscoveryFound(DiscoveredDevice {
            id: DeviceId("a".into()),
            ip: "127.0.0.1".into(),
            version: ProtocolVersion::V35,
            product_key: None,
        });
        gate.emit(sesion(SessionState::Active));
        gate.emit(salud("a", DeviceSessionState::Active));
        gate.emit(color("a", 0.5));
        gate.emit(estadisticas());
        gate.emit(error.clone());
        gate.emit(encontrado.clone());
        assert_eq!(
            sink.events(),
            vec![sesion(SessionState::Active), error, encontrado]
        );
    }

    #[test]
    fn al_volver_manda_el_estado_completo_con_lo_ultimo_de_cada_foco() {
        let (sink, gate) = puerta();
        gate.emit(sesion(SessionState::Connecting));
        gate.set_visible(false);
        gate.emit(sesion(SessionState::Active));
        gate.emit(salud("a", DeviceSessionState::Connecting));
        gate.emit(salud("a", DeviceSessionState::Active));
        gate.emit(salud("b", DeviceSessionState::Reconnecting));
        gate.emit(color("a", 0.1));
        gate.emit(color("a", 0.9));
        gate.emit(color("b", 0.4));
        gate.emit(estadisticas());
        let antes = sink.events().len();
        gate.set_visible(true);
        assert_eq!(
            sink.events()[antes..],
            [
                sesion(SessionState::Active),
                salud("a", DeviceSessionState::Active),
                salud("b", DeviceSessionState::Reconnecting),
                color("a", 0.9),
                color("b", 0.4),
            ]
        );
    }

    #[test]
    fn avisar_dos_veces_lo_mismo_no_repite_el_estado() {
        let (sink, gate) = puerta();
        gate.emit(color("a", 0.5));
        gate.set_visible(true);
        assert_eq!(sink.events().len(), 1);
        gate.set_visible(false);
        gate.set_visible(false);
        assert_eq!(sink.events().len(), 1);
        gate.set_visible(true);
        gate.set_visible(true);
        assert_eq!(sink.events().len(), 2);
    }

    fn error(code: ErrorCode) -> AppEvent {
        AppEvent::ErrorRaised(UserFacingError::new(code))
    }

    #[test]
    fn al_conectarse_la_ventana_recibe_el_estado_completo() {
        let (sink, gate) = puerta();
        // Sin ventana desde el principio, como cuando la app arranca sola
        gate.set_visible(false);
        gate.emit(sesion(SessionState::Active));
        gate.emit(salud("a", DeviceSessionState::Active));
        gate.emit(color("a", 0.9));
        // La ventana se crea y se ve antes de que su página escuche: eso se pierde...
        gate.set_visible(true);
        let antes = sink.events().len();
        // ... y cuando avisa que ya escucha, se manda otra vez
        gate.attach();
        assert_eq!(
            sink.events()[antes..],
            [
                sesion(SessionState::Active),
                salud("a", DeviceSessionState::Active),
                color("a", 0.9),
            ]
        );
    }

    #[test]
    fn los_errores_sin_ventana_se_guardan_y_salen_una_vez_al_conectarse() {
        let (sink, gate) = puerta();
        gate.emit(error(ErrorCode::NoZones));
        gate.emit(error(ErrorCode::NoZones));
        gate.emit(error(ErrorCode::Internal));
        // Salen igual en el momento (la ventana de calibración puede estar escuchando)
        assert_eq!(sink.events().len(), 3);
        gate.attach();
        assert_eq!(
            sink.events()[3..],
            [error(ErrorCode::NoZones), error(ErrorCode::Internal)]
        );
        // Ya entregados: la próxima ventana no los repite
        gate.detach();
        gate.attach();
        assert_eq!(sink.events().len(), 5);
    }

    #[test]
    fn con_la_ventana_escuchando_los_errores_no_se_guardan() {
        let (sink, gate) = puerta();
        gate.attach();
        gate.emit(error(ErrorCode::NoZones));
        gate.detach();
        gate.attach();
        assert_eq!(sink.events(), vec![error(ErrorCode::NoZones)]);
    }

    #[test]
    fn solo_se_guardan_los_ultimos_errores() {
        let (sink, gate) = puerta();
        for n in 0..MAX_PENDING_ERRORS + 5 {
            gate.emit(AppEvent::ErrorRaised(
                UserFacingError::new(ErrorCode::Internal).with_param("cause", n.to_string()),
            ));
        }
        let antes = sink.events().len();
        gate.attach();
        let guardados = &sink.events()[antes..];
        assert_eq!(guardados.len(), MAX_PENDING_ERRORS);
        assert!(matches!(
            &guardados[0],
            AppEvent::ErrorRaised(e) if e.params["cause"] == "5"
        ));
    }

    #[test]
    fn el_sink_en_memoria_guarda_y_reparte() {
        let sink = MemorySink::new();
        let mut rx = sink.subscribe();
        let event = AppEvent::SessionState(SessionStateEvent {
            state: SessionState::Active,
        });
        sink.emit(event.clone());
        assert_eq!(sink.events(), vec![event.clone()]);
        assert_eq!(rx.try_recv().unwrap(), event);
    }
}
