//! El actor de un foco: una tarea de tokio que es dueña del socket.
//!
//! Port de la clase `Foco` de `ambilight.py`. Cada método del script tiene su equivalente:
//!
//! | `Foco` | acá |
//! |---|---|
//! | `__init__` + `_esperar_foco` | [`Actor::start`] |
//! | `preparar` | [`Actor::prepare`] |
//! | `enviar_hsv` | [`Actor::send_colour`] |
//!
//! El actor no comparte estado con nadie: recibe órdenes por un canal `mpsc` ([`Cmd`]) y los
//! colores por un `watch` (que se queda con el último: la coalescencia), y publica su estado en
//! otro `watch`.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use openlight_types::{
    Capabilities, DeviceError, DeviceSessionState, Dps, ErrorCode, Hsv, LinkHealth,
};
use serde_json::{Value, json};
use tokio::sync::{mpsc, watch};
use tokio::time::{Instant, sleep};
use tuya_proto::{Key, PayloadData, cmd, key_from_str};

use crate::CapabilitiesExt;
use crate::colour::colour_write;
use crate::config::{DeviceConfig, IpMode, Locator};
use crate::conn::{Conn, ConnParams, Incoming, dps_of};
use crate::handle::{Cmd, DeviceSnapshot, StartInfo};
use crate::health::{Counters, HealthTracker, reason_of};
use crate::original::{SavedState, StateStore, choose_original};

pub(crate) struct Actor {
    cfg: DeviceConfig,
    key: Result<Key, DeviceError>,
    locator: Arc<dyn Locator>,
    store: Arc<dyn StateStore>,
    cmds: mpsc::Receiver<Cmd>,
    colour: watch::Receiver<Option<Hsv>>,
    snapshot: watch::Sender<DeviceSnapshot>,
    counters: Arc<Counters>,
    health: HealthTracker,

    caps: Option<Capabilities>,
    ip: Option<IpAddr>,
    original: Option<Dps>,
    conn: Option<Conn>,
    /// El foco ya se encontró y se lo mantiene conectado.
    started: bool,
    /// El foco sigue los colores de la pantalla (`_en_color`).
    active: bool,
    state: DeviceSessionState,
}

/// El código visible que corresponde a un error que frena el arranque.
fn error_code(error: &DeviceError) -> ErrorCode {
    match error {
        DeviceError::KeyRejected => ErrorCode::DeviceKeyRejected,
        DeviceError::NotFound => ErrorCode::DeviceNotFound,
        DeviceError::Timeout => ErrorCode::DeviceTimeout,
        DeviceError::Unsupported(_) => ErrorCode::DeviceNoColour,
        DeviceError::Disconnected(_) | DeviceError::Protocol(_) | DeviceError::Io(_) => {
            ErrorCode::Internal
        }
    }
}

/// Qué hacer con lo que llega sin que nadie lo haya pedido (pushes de `STATUS`, acks viejos).
fn on_incoming(health: &mut HealthTracker, incoming: &Incoming, now: Instant) {
    match incoming.cmd {
        cmd::HEART_BEAT => health.heartbeat_reply(now),
        other => tracing::trace!(cmd = other, "frame del foco descartado"),
    }
}

impl Actor {
    pub fn new(
        cfg: DeviceConfig,
        locator: Arc<dyn Locator>,
        store: Arc<dyn StateStore>,
        cmds: mpsc::Receiver<Cmd>,
        colour: watch::Receiver<Option<Hsv>>,
        snapshot: watch::Sender<DeviceSnapshot>,
        counters: Arc<Counters>,
    ) -> Self {
        let key = key_from_str(cfg.creds.local_key.expose()).map_err(|e| {
            // Como tinytuya: una clave que no mide 16 bytes es `ERR_KEY_OR_VER`.
            tracing::warn!(error = %e, "la clave local no es válida");
            DeviceError::KeyRejected
        });
        // `config.json` manda; si no hay IP, vale la última que funcionó.
        let ip = match cfg.ip_mode {
            IpMode::Fixed => cfg.creds.ip,
            IpMode::Auto => cfg.creds.ip.or_else(|| store.load(&cfg.creds.id).ip),
        };
        Self {
            cfg,
            key,
            locator,
            store,
            cmds,
            colour,
            snapshot,
            counters,
            health: HealthTracker::default(),
            caps: None,
            ip,
            original: None,
            conn: None,
            started: false,
            active: false,
            state: DeviceSessionState::Idle,
        }
    }

    // ---- estado publicado ----

    fn publish(&mut self) {
        let health: LinkHealth = self.health.snapshot(Instant::now(), &self.counters);
        let next = DeviceSnapshot {
            state: self.state,
            health,
            ip: self.ip,
        };
        self.snapshot.send_if_modified(|current| {
            let changed = *current != next;
            if changed {
                *current = next;
            }
            changed
        });
    }

    fn set_state(&mut self, state: DeviceSessionState) {
        self.state = state;
        self.publish();
    }

    // ---- el ciclo de vida ----

    pub async fn run(mut self) {
        loop {
            let connected = self.conn.is_some();
            tokio::select! {
                biased;
                read = read_conn(&mut self.conn), if connected => match read {
                    Ok(_) => self.on_data(),
                    Err(e) => self.lost(&e),
                },
                command = self.cmds.recv() => match command {
                    Some(command) => self.handle(command).await,
                    // Se soltaron todos los handles.
                    None => break,
                },
                changed = self.colour.changed(), if connected && self.active => {
                    if changed.is_err() {
                        break;
                    }
                    self.send_colour().await;
                }
            }
        }
        self.conn = None;
    }

    async fn handle(&mut self, command: Cmd) {
        match command {
            Cmd::Start(mut reply) => {
                let outcome = tokio::select! {
                    result = self.start() => Some(result),
                    () = reply.closed() => None,
                };
                match outcome {
                    Some(result) => {
                        let _ = reply.send(result);
                    }
                    // Quien pidió el arranque lo canceló: se vuelve a como estaba.
                    None => {
                        self.conn = None;
                        self.set_state(DeviceSessionState::Idle);
                    }
                }
            }
            Cmd::Activate(reply) => {
                self.activate().await;
                let _ = reply.send(Ok(()));
            }
            Cmd::Status(reply) => {
                let result = self.request(cmd::DP_QUERY, None).await.and_then(|value| {
                    value
                        .as_ref()
                        .and_then(dps_of)
                        .ok_or_else(|| DeviceError::Protocol("la consulta no trajo DPs".into()))
                });
                let _ = reply.send(result);
            }
            Cmd::SetDps { dps, wait, reply } => {
                let result = if wait {
                    self.request(cmd::CONTROL, Some(PayloadData::from(&dps)))
                        .await
                        .map(drop)
                } else {
                    self.write(cmd::CONTROL, Some(PayloadData::from(&dps)))
                        .await
                };
                let _ = reply.send(result);
            }
            Cmd::Heartbeat(reply) => {
                let sent = Instant::now();
                self.health.heartbeat_sent(sent);
                let result = self.request(cmd::HEART_BEAT, None).await.map(drop);
                if result.is_ok() {
                    self.health.heartbeat_reply(Instant::now());
                    self.publish();
                }
                let _ = reply.send(result);
            }
        }
    }

    // ---- conectar ----

    /// IP a la que conectar: la que se tiene o, en modo automático, la que encuentre discovery.
    async fn resolve_ip(&mut self) -> Option<IpAddr> {
        if self.ip.is_none() && self.cfg.ip_mode == IpMode::Auto {
            self.ip = self.locator.locate(&self.cfg.creds.id).await;
        }
        self.ip
    }

    /// En modo automático, después de un fallo: la IP guardada pudo cambiar, se pregunta a
    /// discovery. Si no encuentra nada, se sigue con la que había.
    async fn refresh_ip(&mut self) {
        if self.cfg.ip_mode == IpMode::Auto
            && let Some(ip) = self.locator.locate(&self.cfg.creds.id).await
        {
            self.ip = Some(ip);
        }
    }

    /// Abre la conexión y consulta el estado (`dev.status()` del script).
    async fn connect_and_query(&mut self) -> Result<Dps, DeviceError> {
        let key = self.key.clone()?;
        let ip = self.resolve_ip().await.ok_or(DeviceError::NotFound)?;
        let params = ConnParams {
            version: self.cfg.creds.version,
            key,
            dev_id: self.cfg.creds.id.0.clone(),
            io_timeout: self.cfg.timings.io_timeout,
            connect_timeout: self.cfg.timings.connect_timeout,
        };
        let addr = SocketAddr::new(ip, self.cfg.port);
        let attempt = async {
            let mut conn = Conn::open(addr, &params, self.counters.clone()).await?;
            let reply = conn.request(cmd::DP_QUERY, None, |_| {}).await?;
            let dps = reply
                .as_ref()
                .and_then(dps_of)
                .ok_or_else(|| DeviceError::Protocol("la consulta no trajo DPs".into()))?;
            Ok((conn, dps))
        };
        match attempt.await {
            Ok((conn, dps)) => {
                self.conn = Some(conn);
                Ok(dps)
            }
            Err(e) => {
                self.health.attempt_failed(&e);
                Err(e)
            }
        }
    }

    /// `Foco.__init__`: espera al foco, elige el estado original y lo guarda con la IP.
    async fn start(&mut self) -> Result<StartInfo, DeviceError> {
        if self.started
            && self.conn.is_some()
            && let (Some(ip), Some(caps), Some(original)) =
                (self.ip, self.caps.clone(), self.original.clone())
        {
            return Ok(StartInfo {
                ip,
                capabilities: caps,
                original,
            });
        }
        self.set_state(DeviceSessionState::Connecting);
        match self.wait_for_device().await {
            Ok(info) => {
                self.started = true;
                self.health.connected();
                self.set_state(DeviceSessionState::Idle);
                Ok(info)
            }
            Err(e) => {
                self.conn = None;
                self.set_state(DeviceSessionState::Error(error_code(&e)));
                Err(e)
            }
        }
    }

    /// `_esperar_foco`.
    async fn wait_for_device(&mut self) -> Result<StartInfo, DeviceError> {
        let timings = self.cfg.timings.clone();
        let deadline = Instant::now() + timings.startup_wait;
        // Con IP fija, un 914 es la clave. Con IP guardada, puede ser otro aparato en esa IP.
        let mut search_ip_on_914 = self.cfg.ip_mode == IpMode::Auto;
        let dps = loop {
            match self.connect_and_query().await {
                Ok(dps) => break dps,
                Err(DeviceError::KeyRejected) => {
                    if !search_ip_on_914 {
                        return Err(DeviceError::KeyRejected);
                    }
                    // La IP guardada puede ser de otro aparato: antes de culpar a la clave,
                    // buscarlo de nuevo.
                    search_ip_on_914 = false;
                    self.ip = None;
                }
                Err(e) => {
                    tracing::debug!(error = %e, "el foco todavía no responde");
                    if Instant::now() >= deadline {
                        return Err(DeviceError::NotFound);
                    }
                    self.refresh_ip().await;
                    sleep(timings.startup_retry).await;
                }
            }
        };

        let capabilities = self
            .cfg
            .capabilities
            .clone()
            .or_else(|| Capabilities::from_dps(&dps))
            .ok_or_else(|| DeviceError::Unsupported("no parece un foco de luz".into()))?;
        let saved = self.store.load(&self.cfg.creds.id);
        let original = choose_original(&capabilities, &dps, saved.original.as_ref());
        let ip = self.ip.ok_or(DeviceError::NotFound)?;
        self.store.save(
            &self.cfg.creds.id,
            &SavedState {
                ip: Some(ip),
                original: Some(original.clone()),
            },
        );
        self.caps = Some(capabilities.clone());
        self.original = Some(original.clone());
        Ok(StartInfo {
            ip,
            capabilities,
            original,
        })
    }

    // ---- hablar con el foco ----

    fn offline_error(&self) -> DeviceError {
        if self.started {
            DeviceError::Disconnected(
                self.health
                    .last_reason()
                    .unwrap_or(openlight_types::DisconnectReason::PeerClosed),
            )
        } else {
            DeviceError::Unsupported("el foco no está iniciado".into())
        }
    }

    /// Se perdió la conexión por `error`.
    fn lost(&mut self, error: &DeviceError) {
        if self.conn.take().is_none() {
            return;
        }
        let reason = reason_of(error);
        tracing::warn!(?reason, "se perdió la conexión con el foco");
        self.health.disconnected(reason, Instant::now());
        if self.started {
            self.state = DeviceSessionState::Reconnecting;
        }
        self.publish();
    }

    /// Llegaron bytes: se descartan los frames que nadie pidió, como `_leer_respuestas`.
    fn on_data(&mut self) {
        let now = Instant::now();
        if let Some(conn) = self.conn.as_mut() {
            for incoming in conn.drain() {
                on_incoming(&mut self.health, &incoming, now);
            }
        }
    }

    /// Manda un comando y espera la respuesta. Si falla la conexión, se da por perdida.
    async fn request(
        &mut self,
        command: u32,
        data: Option<PayloadData>,
    ) -> Result<Option<Value>, DeviceError> {
        let offline = self.offline_error();
        let Some(conn) = self.conn.as_mut() else {
            return Err(offline);
        };
        let health = &mut self.health;
        let result = conn
            .request(command, data, |incoming| {
                on_incoming(health, incoming, Instant::now());
            })
            .await;
        if let Err(e) = &result {
            // Un foco que no entiende el comando no es un foco caído.
            if !matches!(e, DeviceError::Protocol(_)) {
                self.lost(e);
            }
        }
        result
    }

    /// Manda un comando sin esperar la respuesta (`nowait=True`).
    async fn write(&mut self, command: u32, data: Option<PayloadData>) -> Result<(), DeviceError> {
        let offline = self.offline_error();
        let Some(conn) = self.conn.as_mut() else {
            return Err(offline);
        };
        let result = conn.send(command, data).await.map(drop);
        if let Err(e) = &result {
            self.lost(e);
        }
        result
    }

    // ---- seguir la pantalla ----

    /// `preparar`: enciende el foco y lo pone en modo color. Sin conexión no hace nada; se
    /// repite al reconectar.
    async fn prepare(&mut self) {
        self.active = true;
        let Some(caps) = self.caps.clone() else {
            return;
        };
        if self.conn.is_none() {
            return;
        }
        let mut dps = Dps::new();
        dps.insert(caps.dp_switch.clone(), json!(true));
        if let Some(mode) = &caps.dp_mode {
            dps.insert(mode.clone(), json!("colour"));
        }
        // Si falla, `write` ya dio la conexión por perdida; se reintenta al reconectar.
        let _ = self
            .write(cmd::CONTROL, Some(PayloadData::from(&dps)))
            .await;
    }

    async fn activate(&mut self) {
        // Un color que quedó de antes de pausar no vale.
        self.colour.borrow_and_update();
        self.prepare().await;
        if self.conn.is_some() {
            self.set_state(DeviceSessionState::Active);
        }
    }

    /// `enviar_hsv`: pide el último color recibido, tal cual. Sin conexión no hace nada.
    async fn send_colour(&mut self) {
        let Some(hsv) = *self.colour.borrow_and_update() else {
            return;
        };
        let Some(caps) = self.caps.clone() else {
            return;
        };
        match colour_write(&caps, self.cfg.channel, hsv) {
            Ok(write) => {
                let mut dps = Dps::new();
                dps.insert(write.dp, Value::String(write.value));
                let _ = self
                    .write(cmd::CONTROL, Some(PayloadData::from(&dps)))
                    .await;
            }
            Err(e) => tracing::warn!(error = %e, "el foco no puede mostrar este color"),
        }
    }
}

/// Lee del socket si hay uno; si no, espera para siempre (la rama del `select!` no corre).
async fn read_conn(conn: &mut Option<Conn>) -> Result<usize, DeviceError> {
    match conn {
        Some(conn) => conn.read_some().await,
        None => std::future::pending().await,
    }
}
