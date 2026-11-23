//! El handle de un foco: lo que usan `openlight-core` y la UI para hablar con el actor.

use std::net::IpAddr;
use std::sync::Arc;

use async_trait::async_trait;
use openlight_types::{
    Capabilities, DeviceError, DeviceId, DeviceSessionState, DisconnectReason, Dps, Hsv,
    LightDevice, LinkHealth,
};
use tokio::sync::{mpsc, oneshot, watch};

use crate::actor::Actor;
use crate::config::{DeviceConfig, Locator};
use crate::health::{Counters, with_bytes};
use crate::original::StateStore;

type Reply<T> = oneshot::Sender<Result<T, DeviceError>>;

/// Qué se sabe del foco apenas se lo encuentra.
#[derive(Debug, Clone, PartialEq)]
pub struct StartInfo {
    /// IP con la que se conectó.
    pub ip: IpAddr,
    /// Capacidades (las de la configuración o las deducidas de `status()`).
    pub capabilities: Capabilities,
    /// Estado al que se vuelve al restaurar, ya guardado en el [`crate::StateStore`].
    pub original: Dps,
}

/// Estado publicado por el actor (evento `device://health` y estado del foco en la sesión).
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceSnapshot {
    /// Estado del foco dentro de la sesión.
    pub state: DeviceSessionState,
    /// Salud del enlace.
    pub health: LinkHealth,
    /// IP con la que se habla (o se habló por última vez) con el foco.
    pub ip: Option<IpAddr>,
}

/// Órdenes del handle al actor.
pub(crate) enum Cmd {
    Start(Reply<StartInfo>),
    Activate(Reply<()>),
    Status(Reply<Dps>),
    SetDps {
        dps: Dps,
        wait: bool,
        reply: Reply<()>,
    },
    Heartbeat(Reply<()>),
    ReturnToOriginal(Reply<()>),
    Restore(Reply<()>),
}

/// Un foco. Se puede clonar y mandar a otras tareas; el actor termina cuando se sueltan todos.
#[derive(Clone)]
pub struct DeviceHandle {
    id: DeviceId,
    cmds: mpsc::Sender<Cmd>,
    colour: Arc<watch::Sender<Option<Hsv>>>,
    snapshot: watch::Receiver<DeviceSnapshot>,
    counters: Arc<Counters>,
}

impl std::fmt::Debug for DeviceHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceHandle")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

/// Lanza el actor de un foco en la runtime de tokio actual. No se conecta hasta
/// [`DeviceHandle::start`].
pub fn spawn(
    config: DeviceConfig,
    locator: Arc<dyn Locator>,
    store: Arc<dyn StateStore>,
) -> DeviceHandle {
    let id = config.creds.id.clone();
    let (cmds_tx, cmds_rx) = mpsc::channel(32);
    let (colour_tx, colour_rx) = watch::channel(None);
    let counters = Arc::new(Counters::default());
    let initial = DeviceSnapshot {
        state: DeviceSessionState::Idle,
        health: LinkHealth::default(),
        ip: config.creds.ip,
    };
    let (snapshot_tx, snapshot_rx) = watch::channel(initial);
    let actor = Actor::new(
        config,
        locator,
        store,
        cmds_rx,
        colour_rx,
        snapshot_tx,
        counters.clone(),
    );
    tokio::spawn(actor.run());
    DeviceHandle {
        id,
        cmds: cmds_tx,
        colour: Arc::new(colour_tx),
        snapshot: snapshot_rx,
        counters,
    }
}

fn gone() -> DeviceError {
    DeviceError::Disconnected(DisconnectReason::Io("el actor del foco ya no está".into()))
}

impl DeviceHandle {
    /// Id del foco.
    pub fn id(&self) -> &DeviceId {
        &self.id
    }

    async fn ask<T>(&self, make: impl FnOnce(Reply<T>) -> Cmd) -> Result<T, DeviceError> {
        let (tx, rx) = oneshot::channel();
        self.cmds.send(make(tx)).await.map_err(|_| gone())?;
        rx.await.map_err(|_| gone())?
    }

    /// Encuentra el foco y guarda su estado original (`Foco(...)` del script).
    ///
    /// Espera hasta `startup_wait` a que aparezca. Si el foco rechaza la clave con una IP
    /// guardada, primero busca la IP de nuevo y solo después culpa a la clave. Soltar el futuro
    /// antes de que termine cancela la espera.
    ///
    /// # Errors
    /// [`DeviceError::NotFound`] si no apareció; [`DeviceError::KeyRejected`] (914) si rechazó
    /// la clave aun con la IP buscada de nuevo; [`DeviceError::Unsupported`] si no es una luz.
    pub async fn start(&self) -> Result<StartInfo, DeviceError> {
        self.ask(Cmd::Start).await
    }

    /// Pone el foco a seguir colores: `switch = true` y `mode = colour`. Si se corta la
    /// conexión, se repite sola al reconectar (`preparar`).
    ///
    /// # Errors
    /// Solo si el actor ya no existe.
    pub async fn activate(&self) -> Result<(), DeviceError> {
        self.ask(Cmd::Activate).await
    }

    /// Pide un color. No espera ni bloquea: si llegan varios antes de que el actor pueda
    /// mandarlos, se manda solo el último. Se ignora mientras el foco no esté activo.
    pub fn send_colour(&self, hsv: Hsv) {
        self.colour.send_replace(Some(hsv));
    }

    /// Lee todos los DPs (`DP_QUERY`).
    ///
    /// # Errors
    /// Si no hay conexión o el foco no responde.
    pub async fn status(&self) -> Result<Dps, DeviceError> {
        self.ask(Cmd::Status).await
    }

    /// Escribe DPs. Con `wait = false` no espera la respuesta (`nowait=True`).
    ///
    /// # Errors
    /// Si no hay conexión o, con `wait`, el foco no responde.
    pub async fn set_dps(&self, dps: Dps, wait: bool) -> Result<(), DeviceError> {
        self.ask(|reply| Cmd::SetDps { dps, wait, reply }).await
    }

    /// Manda una señal de vida y espera la respuesta.
    ///
    /// # Errors
    /// Si no hay conexión o el foco no responde.
    pub async fn heartbeat(&self) -> Result<(), DeviceError> {
        self.ask(Cmd::Heartbeat).await
    }

    /// Deja el foco como estaba antes de empezar, pero sigue conectado y listo para retomar con
    /// [`DeviceHandle::activate`] (`devolver_estado`: la pausa por falta de pantalla). El estado
    /// original sigue guardado.
    ///
    /// # Errors
    /// Si no pudo devolverlo (sin conexión, sin respuesta).
    pub async fn return_to_original(&self) -> Result<(), DeviceError> {
        self.ask(Cmd::ReturnToOriginal).await
    }

    /// Devuelve el foco a su estado original, olvida lo guardado y cierra la conexión
    /// (`restaurar`: salir). Si falla, el original queda guardado para la próxima ejecución y la
    /// conexión se cierra igual. Si el foco no se había iniciado, no hace nada.
    ///
    /// # Errors
    /// Si no pudo devolverlo. Si se había cortado la conexión, antes hace un intento de volver a
    /// conectar.
    pub async fn restore(&self) -> Result<(), DeviceError> {
        self.ask(Cmd::Restore).await
    }

    /// Estado publicado por el actor, con los bytes al día.
    pub fn snapshot(&self) -> DeviceSnapshot {
        let mut snapshot = self.snapshot.borrow().clone();
        snapshot.health = with_bytes(snapshot.health, &self.counters);
        snapshot
    }

    /// Salud del enlace.
    pub fn health(&self) -> LinkHealth {
        self.snapshot().health
    }

    /// Para enterarse de cada cambio de estado o de salud.
    pub fn subscribe(&self) -> watch::Receiver<DeviceSnapshot> {
        self.snapshot.clone()
    }
}

#[async_trait]
impl LightDevice for DeviceHandle {
    async fn status(&mut self) -> Result<Dps, DeviceError> {
        DeviceHandle::status(self).await
    }

    async fn set_dps(&mut self, dps: Dps, wait: bool) -> Result<(), DeviceError> {
        DeviceHandle::set_dps(self, dps, wait).await
    }

    async fn heartbeat(&mut self) -> Result<(), DeviceError> {
        DeviceHandle::heartbeat(self).await
    }

    fn health(&self) -> LinkHealth {
        DeviceHandle::health(self)
    }
}
