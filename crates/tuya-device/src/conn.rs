//! Una conexión TCP con un foco: socket, sesión de `tuya-proto` y decodificador de stream.
//!
//! Hace lo que `_get_socket`, `_send_receive` y `_negotiate_session_key` de tinytuya, pero sin
//! dormir ni reintentar por su cuenta: eso lo decide el actor. Los errores se traducen igual
//! que los códigos de tinytuya:
//!
//! | tinytuya | acá |
//! |---|---|
//! | `ERR_OFFLINE` (905, timeout al conectar) | [`DeviceError::Timeout`] |
//! | `ERR_CONNECT` (901, conexión rechazada u otro error) | `Disconnected(Refused)` / `Io` |
//! | `ERR_KEY_OR_VER` (914, falló la negociación de la clave de sesión) | [`DeviceError::KeyRejected`] |
//! | `ERR_TIMEOUT` (902, la respuesta no llegó) | [`DeviceError::Timeout`] |

use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use openlight_types::{DeviceError, DisconnectReason, Dps, ProtocolVersion};
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{Instant, timeout, timeout_at};
use tuya_proto::{
    ClientSession, DecodedFrame, Key, MessagePayload, OrderedDps, PayloadData, PayloadRequest,
    ProtoError, RetcodeMode, StreamDecoder, StreamError, cmd,
};

use crate::health::Counters;

/// Tamaño de cada lectura del socket (`sock.recv(4096)` del script).
const READ_CHUNK: usize = 4096;

/// Qué hace falta para abrir una conexión.
#[derive(Debug, Clone)]
pub(crate) struct ConnParams {
    pub version: ProtocolVersion,
    pub key: Key,
    pub dev_id: String,
    pub io_timeout: Duration,
    pub connect_timeout: Duration,
}

/// Contenido de un frame que mandó el foco.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Body {
    /// Sin payload (ack de un comando, respuesta al latido).
    Empty,
    /// JSON descifrado.
    Json(Value),
    /// No se pudo descifrar ni interpretar.
    Undecodable(ProtoError),
}

/// Un frame que mandó el foco, ya desarmado.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Incoming {
    pub seqno: u32,
    pub cmd: u32,
    pub body: Body,
}

/// Lo que se mandó: sirve para reconocer la respuesta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Sent {
    pub seqno: u32,
    /// Comando en el que el foco contesta (el de salida, ya remapeado por versión).
    pub reply_cmd: u32,
}

/// Una conexión abierta (y con la clave de sesión negociada en 3.4/3.5).
pub(crate) struct Conn {
    stream: TcpStream,
    session: ClientSession,
    decoder: StreamDecoder,
    chunk: Vec<u8>,
    dev_id: String,
    io_timeout: Duration,
    counters: Arc<Counters>,
    /// Cuándo llegaron bytes por última vez, los haya pedido alguien o no (`_t_respuesta`).
    last_rx: Instant,
}

/// Un IO que falló, como motivo de corte o error.
pub(crate) fn io_error(error: &io::Error) -> DeviceError {
    use io::ErrorKind::{
        BrokenPipe, ConnectionAborted, ConnectionRefused, ConnectionReset, NotConnected,
        UnexpectedEof,
    };
    match error.kind() {
        ConnectionRefused => DeviceError::Disconnected(DisconnectReason::Refused),
        ConnectionReset | ConnectionAborted | BrokenPipe | UnexpectedEof | NotConnected => {
            DeviceError::Disconnected(DisconnectReason::PeerClosed)
        }
        _ => DeviceError::Io(error.to_string()),
    }
}

fn unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn random<const N: usize>() -> Result<[u8; N], DeviceError> {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes).map_err(|e| DeviceError::Io(format!("getrandom: {e}")))?;
    Ok(bytes)
}

/// Los DPs de un payload (`{"dps": {...}}`; `decode_payload` ya copia `data.dps` a `dps`).
pub(crate) fn dps_of(value: &Value) -> Option<Dps> {
    let dps = value.get("dps")?.as_object()?;
    Some(dps.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
}

impl Conn {
    /// Conecta y, en 3.4/3.5, negocia la clave de sesión.
    ///
    /// # Errors
    /// Ver la tabla del módulo. Como tinytuya, cualquier falla de la negociación después de
    /// conectar el socket se informa como [`DeviceError::KeyRejected`].
    pub async fn open(
        addr: SocketAddr,
        params: &ConnParams,
        counters: Arc<Counters>,
    ) -> Result<Self, DeviceError> {
        let attempt = async {
            let stream = TcpStream::connect(addr).await.map_err(|e| io_error(&e))?;
            // Los comandos son chicos y el foco tiene que verlos ya.
            let _ = stream.set_nodelay(true);
            let mut conn = Self {
                stream,
                session: ClientSession::new(params.version, params.key),
                decoder: StreamDecoder::new(),
                chunk: vec![0; READ_CHUNK],
                dev_id: params.dev_id.clone(),
                io_timeout: params.io_timeout,
                counters,
                last_rx: Instant::now(),
            };
            if conn.session.needs_negotiation() {
                conn.negotiate().await?;
            }
            Ok(conn)
        };
        timeout(params.connect_timeout, attempt)
            .await
            .unwrap_or(Err(DeviceError::Timeout))
    }

    /// Cuándo llegaron bytes por última vez. Cuenta todo lo que se lee: respuestas a un pedido,
    /// pushes y acks, no solo lo que lee el bucle del actor.
    pub fn last_rx(&self) -> Instant {
        self.last_rx
    }

    /// Negociación de la clave de sesión: cmd 3 → 4 → 5.
    async fn negotiate(&mut self) -> Result<(), DeviceError> {
        let fail = |why: &dyn std::fmt::Display| {
            tracing::debug!(%why, "falló la negociación de la clave de sesión");
            DeviceError::KeyRejected
        };
        let start = self.session.start_negotiation(random()?);
        self.write_payload(&start).await.map_err(|e| fail(&e))?;
        let deadline = Instant::now() + self.io_timeout;
        let response = self.wait_frame(deadline).await.map_err(|e| fail(&e))?;
        let finish = self
            .session
            .handle_negotiation_response(&response)
            .map_err(|e| fail(&e))?;
        // El FINISH todavía va con la local_key; después se pasa a la clave de sesión.
        self.write_payload(&finish).await.map_err(|e| fail(&e))?;
        self.session.finish_negotiation().map_err(|e| fail(&e))?;
        Ok(())
    }

    /// Espera el próximo frame completo (solo para la negociación: cualquier frame roto falla).
    async fn wait_frame(&mut self, deadline: Instant) -> Result<DecodedFrame, DeviceError> {
        loop {
            let key = self.session.frame_key();
            match self.decoder.next_frame(key.as_ref(), RetcodeMode::Present) {
                Some(Ok(frame)) => return Ok(frame),
                Some(Err(e)) => return Err(DeviceError::Protocol(e.to_string())),
                None => {}
            }
            match timeout_at(deadline, self.read_some()).await {
                Err(_) => return Err(DeviceError::Timeout),
                Ok(Err(e)) => return Err(e),
                Ok(Ok(_)) => {}
            }
        }
    }

    /// Cifra, empaqueta y escribe un payload (con el `io_timeout`).
    async fn write_payload(&mut self, msg: &MessagePayload) -> Result<Sent, DeviceError> {
        let seqno = self.session.seqno();
        let bytes = self
            .session
            .encode(msg, Some(random()?))
            .map_err(|e| DeviceError::Protocol(e.to_string()))?;
        match timeout(self.io_timeout, self.stream.write_all(&bytes)).await {
            Err(_) => return Err(DeviceError::Timeout),
            Ok(Err(e)) => return Err(io_error(&e)),
            Ok(Ok(())) => {}
        }
        self.counters.add_tx(bytes.len());
        Ok(Sent {
            seqno,
            reply_cmd: msg.cmd,
        })
    }

    /// Manda un comando sin esperar la respuesta (`nowait=True`).
    ///
    /// # Errors
    /// [`DeviceError::Timeout`] si el socket no acepta los bytes a tiempo, o el error de IO.
    pub async fn send(
        &mut self,
        command: u32,
        data: Option<PayloadData>,
    ) -> Result<Sent, DeviceError> {
        let mut request =
            PayloadRequest::new(self.session.version(), &self.dev_id, command, unix_secs());
        request.data = data;
        let payload = self.session.payload(&request);
        self.write_payload(&payload).await
    }

    /// Lee del socket lo que haya y lo pasa al decodificador. Devuelve cuántos bytes llegaron.
    ///
    /// Se puede cancelar sin perder datos (si el `await` se abandona, no se leyó nada).
    ///
    /// # Errors
    /// `Disconnected(PeerClosed)` si el foco cerró la conexión (EOF), o el error de IO.
    pub async fn read_some(&mut self) -> Result<usize, DeviceError> {
        let n = self
            .stream
            .read(&mut self.chunk)
            .await
            .map_err(|e| io_error(&e))?;
        if n == 0 {
            return Err(DeviceError::Disconnected(DisconnectReason::PeerClosed));
        }
        self.counters.add_rx(n);
        self.last_rx = Instant::now();
        self.decoder.push(&self.chunk[..n]);
        Ok(n)
    }

    /// Saca los frames completos que ya están en el búfer. Los rotos se descartan.
    pub fn drain(&mut self) -> Vec<Incoming> {
        let mut out = Vec::new();
        loop {
            let key = self.session.frame_key();
            match self.decoder.next_frame(key.as_ref(), RetcodeMode::Present) {
                None => return out,
                Some(Ok(frame)) => out.push(self.incoming(&frame)),
                Some(Err(e)) => {
                    let what = match &e {
                        StreamError::BadChecksum(_) => "CRC/HMAC inválido",
                        StreamError::AuthFailed(_) => "tag GCM inválido",
                        StreamError::Proto(_) => "frame inválido",
                    };
                    tracing::debug!(error = %e, "{what}: se descarta el frame");
                }
            }
        }
    }

    fn incoming(&self, frame: &DecodedFrame) -> Incoming {
        let body = match self.session.decode(frame) {
            Ok(None) => Body::Empty,
            Ok(Some(value)) => Body::Json(value),
            Err(e) => Body::Undecodable(e),
        };
        Incoming {
            seqno: frame.seqno,
            cmd: frame.cmd,
            body,
        }
    }

    /// Manda un comando y espera su respuesta (`_send_receive`). Todo lo que llega mientras
    /// tanto (pushes de `STATUS`, acks viejos, latidos) se pasa a `other`.
    ///
    /// Qué cuenta como respuesta depende del comando, y **no** se exige el mismo `seqno`: el
    /// W509Z2 real numera sus frames con un contador propio (medido con tinytuya).
    ///
    /// | comando | respuesta |
    /// |---|---|
    /// | consulta (`DP_QUERY`) | un frame de ese comando con DPs |
    /// | control (`CONTROL`) | un push `STATUS` cuyos DPs incluyen los que se mandaron, con esos valores (lo único que contesta el foco real), o el ack del mismo comando y `seqno` (lo que contesta el simulador de tinytuya) |
    /// | latido y demás | un frame del mismo comando |
    ///
    /// Un push que no trae los DPs mandados (el de un envío anterior sin espera, o un cambio
    /// de afuera) no cuenta: se pasa a `other` y se sigue esperando. Si el control no recibe
    /// respuesta, se lee el estado: un foco que ya tenía esos valores puede no mandar ningún
    /// push, y el pedido está cumplido.
    ///
    /// # Errors
    /// [`DeviceError::Timeout`] si no llega en `io_timeout`; [`DeviceError::KeyRejected`] si
    /// llega algo que no se puede descifrar (la clave o la versión no son las del foco).
    pub async fn request(
        &mut self,
        command: u32,
        data: Option<PayloadData>,
        mut other: impl FnMut(&Incoming),
    ) -> Result<Option<Value>, DeviceError> {
        let sent_dps = match (&data, command) {
            (Some(PayloadData::Dps(items)), cmd::CONTROL | cmd::CONTROL_NEW) => Some(items.clone()),
            _ => None,
        };
        let sent = self.send(command, data).await?;
        let expects_data = matches!(command, cmd::DP_QUERY | cmd::DP_QUERY_NEW);
        let deadline = Instant::now() + self.io_timeout;
        loop {
            for incoming in self.drain() {
                let is_reply = match &sent_dps {
                    Some(dps) => is_control_reply(&incoming, &sent, dps),
                    None => {
                        incoming.cmd == sent.reply_cmd
                            && (!expects_data || matches!(incoming.body, Body::Json(_)))
                    }
                };
                if !is_reply {
                    other(&incoming);
                    continue;
                }
                return match incoming.body {
                    Body::Empty => Ok(None),
                    Body::Json(value) => Ok(Some(value)),
                    Body::Undecodable(ProtoError::DataUnvalid) => Err(DeviceError::Protocol(
                        "el foco respondió 'data unvalid' (device22, no soportado)".into(),
                    )),
                    Body::Undecodable(e) => {
                        tracing::debug!(error = %e, "respuesta indescifrable");
                        Err(DeviceError::KeyRejected)
                    }
                };
            }
            match timeout_at(deadline, self.read_some()).await {
                Err(_) => {
                    // Un foco que ya tenía esos valores puede no mandar ningún push: se lee el
                    // estado para ver si el pedido ya está cumplido.
                    if let Some(dps) = &sent_dps
                        && self.read_back(dps, &mut other).await
                    {
                        return Ok(None);
                    }
                    return Err(DeviceError::Timeout);
                }
                Ok(Err(e)) => return Err(e),
                Ok(Ok(_)) => {}
            }
        }
    }

    /// Consulta el estado y dice si ya trae `wanted`. Cualquier falla cuenta como que no.
    async fn read_back(&mut self, wanted: &OrderedDps, other: &mut impl FnMut(&Incoming)) -> bool {
        let Ok(query) = self.send(cmd::DP_QUERY, None).await else {
            return false;
        };
        let deadline = Instant::now() + self.io_timeout;
        loop {
            for incoming in self.drain() {
                match &incoming.body {
                    Body::Json(value) if incoming.cmd == query.reply_cmd => {
                        return dps_of(value).is_some_and(|dps| contains(&dps, wanted));
                    }
                    _ => other(&incoming),
                }
            }
            if !matches!(timeout_at(deadline, self.read_some()).await, Ok(Ok(_))) {
                return false;
            }
        }
    }
}

/// Si `dps` trae todos los DPs de `wanted` con esos valores.
fn contains(dps: &Dps, wanted: &OrderedDps) -> bool {
    wanted
        .iter()
        .all(|(dp, value)| dps.get(dp).is_some_and(|got| got == value))
}

/// Si `incoming` contesta a un control que mandó `sent_dps` con el frame `sent`.
fn is_control_reply(incoming: &Incoming, sent: &Sent, sent_dps: &OrderedDps) -> bool {
    match incoming.cmd {
        // El push de estado del foco real: vale si trae lo que se pidió.
        cmd::STATUS => match &incoming.body {
            Body::Json(value) => dps_of(value).is_some_and(|dps| contains(&dps, sent_dps)),
            _ => false,
        },
        // El ack del mismo comando, con el seqno que mandamos.
        command => command == sent.reply_cmd && incoming.seqno == sent.seqno,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::json;
    use tuya_sim::{SimConfig, SimHandle};

    use super::*;

    const KEY: &[u8; 16] = b"OpenLightSimKey1";

    async fn sim(version: ProtocolVersion) -> SimHandle {
        tuya_sim::start(SimConfig::new(version).ephemeral())
            .await
            .expect("arranca el simulador")
    }

    fn params(sim: &SimHandle) -> ConnParams {
        ConnParams {
            version: sim.version(),
            key: *KEY,
            dev_id: sim.dev_id().0.clone(),
            io_timeout: Duration::from_secs(2),
            connect_timeout: Duration::from_secs(3),
        }
    }

    async fn open(sim: &SimHandle) -> Result<Conn, DeviceError> {
        Conn::open(sim.addr(), &params(sim), Arc::default()).await
    }

    fn dps_json(value: Value) -> PayloadData {
        let dps: Dps = serde_json::from_value(value).unwrap();
        PayloadData::from(&dps)
    }

    #[tokio::test]
    async fn consulta_control_y_latido_en_todas_las_versiones() {
        for version in ProtocolVersion::ALL {
            let sim = sim(version).await;
            let mut conn = open(&sim)
                .await
                .unwrap_or_else(|e| panic!("{version}: {e}"));

            let reply = conn
                .request(cmd::DP_QUERY, None, |_| {})
                .await
                .unwrap_or_else(|e| panic!("{version}: {e}"))
                .expect("la consulta trae datos");
            assert_eq!(dps_of(&reply).unwrap()["21"], json!("colour"), "{version}");

            // CONTROL con espera: el ack vacío cierra la espera.
            let ack = conn
                .request(cmd::CONTROL, Some(dps_json(json!({"21": "white"}))), |_| {})
                .await
                .unwrap_or_else(|e| panic!("{version}: {e}"));
            assert_eq!(ack, None, "{version}");
            assert_eq!(sim.dp("21"), Some(json!("white")), "{version}");

            // Latido: ack vacío.
            let ack = conn.request(cmd::HEART_BEAT, None, |_| {}).await.unwrap();
            assert_eq!(ack, None, "{version}");
            assert_eq!(sim.stats().heartbeats(), 1, "{version}");
        }
    }

    #[tokio::test]
    async fn el_push_de_status_llega_aparte_de_la_respuesta() {
        let sim = sim(ProtocolVersion::V35).await;
        let mut conn = open(&sim).await.unwrap();
        let mut others = Vec::new();
        conn.request(cmd::CONTROL, Some(dps_json(json!({"22": 500}))), |i| {
            others.push(i.clone())
        })
        .await
        .unwrap();
        // El foco manda el ack y después el push con el DP que cambió: se lee todo.
        conn.read_some().await.ok();
        others.extend(conn.drain());
        let push = others
            .iter()
            .find(|i| i.cmd == cmd::STATUS)
            .expect("push de STATUS");
        let Body::Json(value) = &push.body else {
            panic!("el push trae JSON: {push:?}");
        };
        assert_eq!(dps_of(value).unwrap()["22"], json!(500));
    }

    #[tokio::test]
    async fn clave_equivocada_en_3_4_y_3_5_es_914() {
        for version in [ProtocolVersion::V34, ProtocolVersion::V35] {
            let sim = tuya_sim::start(SimConfig::new(version).ephemeral().faults(
                tuya_sim::Faults {
                    reject_key: true,
                    ..Default::default()
                },
            ))
            .await
            .unwrap();
            let err = open(&sim).await.err().expect("no debería abrir");
            assert_eq!(err, DeviceError::KeyRejected, "{version}");
        }
    }

    #[tokio::test]
    async fn sin_nadie_escuchando_es_conexion_rechazada() {
        // Un puerto libre: se reserva y se suelta.
        let port = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        let sim = sim(ProtocolVersion::V33).await;
        let err = Conn::open(
            format!("127.0.0.1:{port}").parse().unwrap(),
            &params(&sim),
            Arc::default(),
        )
        .await
        .err()
        .unwrap();
        assert_eq!(err, DeviceError::Disconnected(DisconnectReason::Refused));
    }

    #[tokio::test]
    async fn foco_mudo_da_timeout_y_cerrado_da_eof() {
        let sim = sim(ProtocolVersion::V33).await;
        let mut conn = open(&sim).await.unwrap();
        sim.update_faults(|f| f.mute = true);
        let mut p = params(&sim);
        p.io_timeout = Duration::from_millis(100);
        conn.io_timeout = p.io_timeout;
        assert_eq!(
            conn.request(cmd::DP_QUERY, None, |_| {}).await,
            Err(DeviceError::Timeout)
        );
        sim.disconnect_all();
        // Puede entrar algún byte antes del cierre: se lee hasta ver el EOF.
        let eof = loop {
            match timeout(Duration::from_secs(2), conn.read_some()).await {
                Ok(Ok(_)) => {}
                Ok(Err(e)) => break e,
                Err(_) => panic!("no llegó el EOF"),
            }
        };
        assert_eq!(eof, DeviceError::Disconnected(DisconnectReason::PeerClosed));
    }

    async fn sim_real(version: ProtocolVersion) -> SimHandle {
        tuya_sim::start(SimConfig::new(version).ephemeral().real_firmware(true))
            .await
            .expect("arranca el simulador")
    }

    // El W509Z2 real numera sus frames con un contador propio y a un control contesta solo con
    // el push de STATUS (medido con tinytuya en la prueba I2).
    #[tokio::test]
    async fn con_el_firmware_real_el_push_confirma_el_control() {
        for version in ProtocolVersion::ALL {
            let sim = sim_real(version).await;
            let mut conn = open(&sim)
                .await
                .unwrap_or_else(|e| panic!("{version}: {e}"));

            let status = conn
                .request(cmd::DP_QUERY, None, |_| {})
                .await
                .unwrap_or_else(|e| panic!("{version}: {e}"))
                .expect("la consulta trae datos");
            assert_eq!(dps_of(&status).unwrap()["21"], json!("colour"), "{version}");

            let ack = conn
                .request(cmd::CONTROL, Some(dps_json(json!({"21": "white"}))), |_| {})
                .await
                .unwrap_or_else(|e| panic!("{version}: set con espera: {e}"));
            // La respuesta es el push, con el estado nuevo.
            let push = ack.expect("el push trae los DPs");
            assert_eq!(dps_of(&push).unwrap()["21"], json!("white"), "{version}");
            assert_eq!(sim.dp("21"), Some(json!("white")), "{version}");

            conn.request(cmd::HEART_BEAT, None, |_| {})
                .await
                .unwrap_or_else(|e| panic!("{version}: latido: {e}"));
        }
    }

    #[tokio::test]
    async fn un_push_de_otro_envio_no_confirma_el_control() {
        let sim = sim_real(ProtocolVersion::V35).await;
        let mut conn = open(&sim).await.unwrap();
        // Un envío sin espera (la prueba RGB) deja su push en camino...
        conn.send(cmd::CONTROL, Some(dps_json(json!({"22": 300}))))
            .await
            .unwrap();
        // ...y el control con espera que viene después no lo toma por suyo.
        let mut others = Vec::new();
        let reply = conn
            .request(cmd::CONTROL, Some(dps_json(json!({"22": 400}))), |i| {
                others.push(i.clone());
            })
            .await
            .unwrap()
            .expect("push");
        assert_eq!(dps_of(&reply).unwrap()["22"], json!(400));
        assert!(
            others.iter().any(|i| matches!(&i.body,
                Body::Json(v) if dps_of(v).is_some_and(|d| d["22"] == json!(300)))),
            "el push viejo se pasó a `other`: {others:?}"
        );
    }

    #[tokio::test]
    async fn un_control_que_no_cambia_nada_se_confirma_leyendo_el_estado() {
        let sim = sim_real(ProtocolVersion::V35).await;
        let mut conn = open(&sim).await.unwrap();
        // El foco ya tiene 21 = colour: con el firmware real no manda ningún push.
        let result = conn
            .request(
                cmd::CONTROL,
                Some(dps_json(json!({"21": "colour"}))),
                |_| {},
            )
            .await;
        assert_eq!(result, Ok(None));
        // Pero un valor distinto que no se aplica (fuera de rango) sigue siendo un fallo.
        let result = conn
            .request(cmd::CONTROL, Some(dps_json(json!({"22": 5}))), |_| {})
            .await;
        assert_eq!(result, Err(DeviceError::Timeout));
    }

    #[tokio::test]
    async fn un_control_que_nadie_contesta_da_timeout() {
        let sim = sim_real(ProtocolVersion::V33).await;
        let mut conn = open(&sim).await.unwrap();
        conn.io_timeout = Duration::from_millis(100);
        sim.update_faults(|f| f.mute = true);
        let result = conn
            .request(cmd::CONTROL, Some(dps_json(json!({"22": 400}))), |_| {})
            .await;
        assert_eq!(result, Err(DeviceError::Timeout));
    }

    #[tokio::test]
    async fn las_respuestas_a_un_pedido_cuentan_como_actividad() {
        // `status()` y `set_dps(wait = true)` leen sus respuestas dentro de `request`, no en el
        // bucle del actor: la conexión tiene que anotar igual que llegaron bytes.
        let sim = sim(ProtocolVersion::V33).await;
        let mut conn = open(&sim).await.unwrap();
        let abierta = conn.last_rx();
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert_eq!(conn.last_rx(), abierta, "sin tráfico no cambia");
        conn.request(cmd::DP_QUERY, None, |_| {}).await.unwrap();
        assert!(conn.last_rx() > abierta + Duration::from_millis(25));
    }

    #[tokio::test]
    async fn cuenta_los_bytes_que_pasan() {
        let sim = sim(ProtocolVersion::V33).await;
        let counters = Arc::new(Counters::default());
        let mut conn = Conn::open(sim.addr(), &params(&sim), counters.clone())
            .await
            .unwrap();
        conn.request(cmd::DP_QUERY, None, |_| {}).await.unwrap();
        let health = crate::health::with_bytes(Default::default(), &counters);
        assert!(health.bytes_tx > 0 && health.bytes_rx > 0, "{health:?}");
    }
}
