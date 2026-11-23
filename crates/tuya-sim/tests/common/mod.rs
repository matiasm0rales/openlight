//! Cliente mínimo para probar el simulador (frames y cripto de `tuya-proto`).
#![allow(dead_code, missing_docs)]

use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use openlight_types::{Dps, ProtocolVersion};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tuya_sim::{Msg, Role, SimConfig, SimHandle, Suite, Wire, WireError, cmd};

/// La `Suite` de las pruebas.
pub fn suite() -> Arc<dyn Suite> {
    Arc::new(tuya_sim::ProtoSuite)
}

pub async fn start(config: SimConfig) -> SimHandle {
    tuya_sim::start_with(config.ephemeral(), suite())
        .await
        .expect("arranca el simulador")
}

pub const T: Duration = Duration::from_secs(2);

/// Lo que puede pasar al leer.
#[derive(Debug, PartialEq)]
pub enum Read {
    Msg(Msg),
    Timeout,
    Closed,
}

pub struct Client {
    pub sock: TcpStream,
    wire: Box<dyn Wire>,
    buf: Vec<u8>,
    seq: u32,
    pub version: ProtocolVersion,
    suite: Arc<dyn Suite>,
    key: [u8; 16],
}

impl Client {
    /// Conecta y, en 3.4/3.5, negocia la clave de sesión.
    pub async fn connect(sim: &SimHandle) -> io::Result<Self> {
        Self::connect_with_key(sim.addr(), sim.version(), sim.local_key()).await
    }

    pub async fn connect_with_key(
        addr: SocketAddr,
        version: ProtocolVersion,
        key: [u8; 16],
    ) -> io::Result<Self> {
        let suite = suite();
        let sock = TcpStream::connect(addr).await?;
        let mut c = Self {
            sock,
            wire: suite.wire(version, key, Role::Client),
            buf: Vec::new(),
            seq: 1,
            version,
            suite,
            key,
        };
        if version.uses_session_key() {
            c.negotiate().await?;
        }
        Ok(c)
    }

    async fn negotiate(&mut self) -> io::Result<()> {
        let client: [u8; 16] = *b"nonce-del-client";
        self.send(cmd::SESS_KEY_NEG_START, client.to_vec()).await?;
        let resp = match self.recv(T).await {
            Read::Msg(m) if m.cmd == cmd::SESS_KEY_NEG_RESP => m,
            other => return Err(io::Error::other(format!("negociación: {other:?}"))),
        };
        let device: [u8; 16] = resp.payload[..16].try_into().unwrap();
        if resp.payload[16..48] != self.suite.hmac_sha256(&self.key, &client) {
            return Err(io::Error::other("HMAC del foco inválido"));
        }
        let finish = self.suite.hmac_sha256(&self.key, &device).to_vec();
        self.send(cmd::SESS_KEY_NEG_FINISH, finish).await?;
        let session = self
            .suite
            .session_key(self.version, &self.key, &client, &device);
        self.wire.set_key(session);
        Ok(())
    }

    pub async fn send(&mut self, cmd: u32, payload: Vec<u8>) -> io::Result<()> {
        let header = !matches!(
            cmd,
            cmd::DP_QUERY
                | cmd::DP_QUERY_NEW
                | cmd::UPDATEDPS
                | cmd::HEART_BEAT
                | cmd::SESS_KEY_NEG_START
                | cmd::SESS_KEY_NEG_RESP
                | cmd::SESS_KEY_NEG_FINISH
        );
        let msg = Msg {
            seqno: self.seq,
            cmd,
            retcode: None,
            payload,
        };
        self.seq += 1;
        let bytes = self.wire.encode(&msg, header);
        self.sock.write_all(&bytes).await
    }

    pub async fn recv(&mut self, timeout: Duration) -> Read {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            match self.wire.decode(&mut self.buf) {
                Ok(Some(m)) => return Read::Msg(m),
                Ok(None) => {}
                Err(WireError::BadAuth | WireError::Malformed(_)) => return Read::Closed,
            }
            let mut chunk = [0u8; 4096];
            match tokio::time::timeout_at(deadline, self.sock.read(&mut chunk)).await {
                Err(_) => return Read::Timeout,
                Ok(Ok(0) | Err(_)) => return Read::Closed,
                Ok(Ok(n)) => self.buf.extend_from_slice(&chunk[..n]),
            }
        }
    }

    /// Lee hasta el primer mensaje con payload (salta los acks vacíos).
    pub async fn recv_payload(&mut self, timeout: Duration) -> Read {
        loop {
            match self.recv(timeout).await {
                Read::Msg(m) if m.payload.is_empty() => continue,
                other => return other,
            }
        }
    }

    fn query_cmd(&self) -> u32 {
        if self.version.uses_session_key() {
            cmd::DP_QUERY_NEW
        } else {
            cmd::DP_QUERY
        }
    }

    fn control_cmd(&self) -> u32 {
        if self.version.uses_session_key() {
            cmd::CONTROL_NEW
        } else {
            cmd::CONTROL
        }
    }

    /// `status()`: todos los DPs.
    pub async fn status(&mut self) -> Option<Dps> {
        let body = if self.version.uses_session_key() {
            json!({})
        } else {
            json!({"gwId": "x", "devId": "x", "uid": "x", "t": "0"})
        };
        self.send(self.query_cmd(), body.to_string().into_bytes())
            .await
            .ok()?;
        match self.recv_payload(T).await {
            Read::Msg(m) => dps_of(&m.payload),
            _ => None,
        }
    }

    /// Manda DPs sin esperar respuesta.
    pub async fn set_nowait(&mut self, dps: Value) -> io::Result<()> {
        let body = if self.version.uses_session_key() {
            json!({"protocol": 5, "t": 0, "data": {"dps": dps}})
        } else {
            json!({"devId": "x", "uid": "x", "t": "0", "dps": dps})
        };
        self.send(self.control_cmd(), body.to_string().into_bytes())
            .await
    }

    /// `set_multiple_values()`: manda y espera el push de `STATUS`.
    pub async fn set(&mut self, dps: Value) -> Option<Dps> {
        self.set_nowait(dps).await.ok()?;
        match self.recv_payload(T).await {
            Read::Msg(m) if m.cmd == cmd::STATUS => dps_of(&m.payload),
            _ => None,
        }
    }

    /// Latido: espera el frame vacío.
    pub async fn heartbeat(&mut self) -> Read {
        let body = json!({"gwId": "x", "devId": "x"}).to_string().into_bytes();
        if self.send(cmd::HEART_BEAT, body).await.is_err() {
            return Read::Closed;
        }
        self.recv(T).await
    }
}

pub fn dps_of(payload: &[u8]) -> Option<Dps> {
    let v: Value = serde_json::from_slice(payload).ok()?;
    let dps = v.get("dps").or_else(|| v.get("data")?.get("dps"))?;
    serde_json::from_value(dps.clone()).ok()
}

pub fn dps(v: Value) -> Dps {
    serde_json::from_value(v).unwrap()
}
