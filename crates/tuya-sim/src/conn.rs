//! Una conexión TCP con el foco simulado: negociación, consultas, control, latidos y pushes.
//!
//! El comportamiento sigue lo que espera tinytuya (`core/XenonDevice.py`):
//! - `CONTROL`/`CONTROL_NEW` se contesta con un ack vacío del mismo comando y después un push
//!   de `STATUS` con los DPs aceptados (`_send_receive` descarta el ack vacío y espera el push).
//! - `DP_QUERY`/`DP_QUERY_NEW` se contesta con todos los DPs, sin encabezado de versión.
//! - `HEART_BEAT` se contesta con un frame vacío.
//! - `UPDATEDPS` se contesta con un ack vacío y un push con los DPs pedidos que existan.
//! - Con `real_firmware` (medido con un W509Z2 en 3.5) el foco numera todos sus frames con un
//!   contador propio, sin repetir el `seqno` del cliente, y a `CONTROL`/`CONTROL_NEW` contesta
//!   solo con el push de `STATUS`, sin ack.
//! - En 3.4/3.5, nada funciona hasta completar la negociación (cmd 3 → 4 → 5). El cmd 5 no se
//!   contesta: tinytuya lo manda con `_send_receive_quick(step3, None)`.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};

use openlight_types::Dps;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{broadcast, watch};
use tokio::time::Instant;

use crate::server::Shared;
use crate::wire::{Msg, Role, Wire, WireError, cmd};

/// Clave "equivocada" que usa el foco cuando `reject_key` está activa.
pub(crate) fn wrong_key(key: &[u8; 16]) -> [u8; 16] {
    key.map(|b| b ^ 0x5a)
}

enum Phase {
    /// 3.4/3.5 esperando el cmd 3.
    AwaitStart,
    /// 3.4/3.5 esperando el cmd 5.
    AwaitFinish { client: [u8; 16], device: [u8; 16] },
    /// Listo para consultas y control.
    Ready,
}

struct Out {
    msg: Msg,
    header: bool,
}

enum Flow {
    Continue(Vec<Out>),
    Close,
}

struct Conn {
    shared: Arc<Shared>,
    wire: Box<dyn Wire>,
    key: [u8; 16],
    rejecting: bool,
    phase: Phase,
    push_seq: u32,
    opened: Instant,
}

/// Baja el contador de conexiones activas al terminar, pase lo que pase.
struct ActiveGuard(Arc<Shared>);

impl Drop for ActiveGuard {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::SeqCst);
        // Las esperas sobre `active_connections()` tienen que enterarse del cierre.
        self.0.wake();
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn random16() -> [u8; 16] {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).expect("getrandom");
    b
}

pub(crate) async fn run(
    shared: Arc<Shared>,
    mut sock: TcpStream,
    peer: SocketAddr,
    mut changes: broadcast::Receiver<Dps>,
    mut kill: watch::Receiver<u64>,
) {
    let _guard = ActiveGuard(shared.clone());
    let opened = Instant::now();
    let mut faults_rx = shared.faults.subscribe();
    let mut shutdown = shared.shutdown.subscribe();

    let faults = shared.faults();
    let key = if faults.reject_key {
        wrong_key(&shared.local_key)
    } else {
        shared.local_key
    };
    let mut conn = Conn {
        wire: shared.suite.wire(shared.version, key, Role::Device),
        key,
        rejecting: faults.reject_key,
        phase: if shared.version.uses_session_key() {
            Phase::AwaitStart
        } else {
            Phase::Ready
        },
        // Un contador propio y ya avanzado, como el del foco real (17510 en la medición).
        push_seq: if shared.real_firmware { 17_500 } else { 0 },
        opened,
        shared: shared.clone(),
    };
    tracing::debug!(%peer, "conexión nueva");

    let mut buf = Vec::with_capacity(4096);
    let mut chunk = [0u8; 4096];
    loop {
        let faults = shared.faults();
        let drop_at = faults.drop_every.map(|d| opened + d);
        tokio::select! {
            read = sock.read(&mut chunk) => {
                let n = match read {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };
                buf.extend_from_slice(&chunk[..n]);
                if !conn.drain(&mut buf, &mut sock).await {
                    break;
                }
            }
            change = changes.recv() => match change {
                Ok(dps) => {
                    if matches!(conn.phase, Phase::Ready) && !shared.faults().mute {
                        let out = conn.status_push(&dps);
                        if !conn.send(&mut sock, vec![out]).await {
                            break;
                        }
                    }
                }
                Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => break,
            },
            _ = sleep_until(drop_at) => {
                tracing::debug!(%peer, "drop_every: corto la conexión");
                shared.stat(|s| s.drops += 1);
                break;
            }
            changed = faults_rx.changed() => {
                if changed.is_err() {
                    break;
                }
                if shared.faults().reject_key && !conn.rejecting {
                    // El foco "cambió de clave": corta lo que tenga abierto.
                    break;
                }
            }
            _ = kill.changed() => break,
            _ = shutdown.changed() => break,
        }
    }
    let _ = sock.shutdown().await;
    tracing::debug!(%peer, "conexión cerrada");
}

async fn sleep_until(at: Option<Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}

impl Conn {
    /// Procesa todos los frames completos de `buf`. `false` = cerrar la conexión.
    async fn drain(&mut self, buf: &mut Vec<u8>, sock: &mut TcpStream) -> bool {
        loop {
            match self.wire.decode(buf) {
                Ok(None) => return true,
                Ok(Some(msg)) => {
                    self.shared
                        .stat(|s| *s.by_cmd.entry(msg.cmd).or_insert(0) += 1);
                    if self.shared.faults().mute {
                        self.shared.stat(|s| s.muted += 1);
                        continue;
                    }
                    match self.handle(msg) {
                        Flow::Continue(out) => {
                            if !self.send(sock, out).await {
                                return false;
                            }
                        }
                        Flow::Close => return false,
                    }
                }
                Err(WireError::BadAuth) => {
                    tracing::debug!("frame con otra clave: corto como un foco real");
                    self.shared.stat(|s| s.key_rejections += 1);
                    return false;
                }
                Err(WireError::Malformed(e)) => {
                    tracing::debug!(error = %e, "frame inválido: corto");
                    return false;
                }
            }
        }
    }

    async fn send(&mut self, sock: &mut TcpStream, out: Vec<Out>) -> bool {
        if out.is_empty() {
            return true;
        }
        let faults = self.shared.faults();
        let drop_at = faults.drop_every.map(|d| self.opened + d);
        // La latencia no frena a `drop_every`: si el corte vence antes, no sale la respuesta.
        let dropped = tokio::select! {
            _ = tokio::time::sleep(faults.latency) => drop_at.is_some_and(|at| Instant::now() >= at),
            _ = sleep_until(drop_at) => true,
        };
        if dropped {
            tracing::debug!("drop_every venció antes de responder: corto");
            self.shared.stat(|s| s.drops += 1);
            return false;
        }
        for o in out {
            if o.msg.cmd == cmd::STATUS {
                self.shared.stat(|s| s.pushes += 1);
            }
            let bytes = self.wire.encode(&o.msg, o.header);
            if sock.write_all(&bytes).await.is_err() {
                return false;
            }
        }
        true
    }

    fn reply(&mut self, req: &Msg, cmd: u32, payload: Vec<u8>, header: bool) -> Out {
        let seqno = if self.shared.real_firmware {
            self.push_seq += 1;
            self.push_seq
        } else {
            req.seqno
        };
        Out {
            msg: Msg {
                seqno,
                cmd,
                retcode: Some(0),
                payload,
            },
            header,
        }
    }

    fn ack(&mut self, req: &Msg) -> Out {
        self.reply(req, req.cmd, Vec::new(), false)
    }

    fn status_push(&mut self, dps: &Dps) -> Out {
        self.push_seq += 1;
        let t = now();
        let body = if self.shared.version.uses_session_key() {
            json!({"protocol": 4, "t": t, "data": {"dps": dps}})
        } else {
            json!({"devId": self.shared.dev_id.0, "dps": dps, "t": t})
        };
        Out {
            msg: Msg {
                seqno: self.push_seq,
                cmd: cmd::STATUS,
                retcode: Some(0),
                payload: body.to_string().into_bytes(),
            },
            header: true,
        }
    }

    fn handle(&mut self, msg: Msg) -> Flow {
        match &self.phase {
            Phase::AwaitStart => {
                if msg.cmd != cmd::SESS_KEY_NEG_START {
                    // Sin sesión el foco no atiende nada más.
                    return Flow::Continue(Vec::new());
                }
                let Ok(client) = <[u8; 16]>::try_from(msg.payload.get(..16).unwrap_or(&[])) else {
                    return Flow::Close;
                };
                let device = random16();
                let mut payload = device.to_vec();
                payload.extend_from_slice(&self.shared.suite.hmac_sha256(&self.key, &client));
                self.phase = Phase::AwaitFinish { client, device };
                return Flow::Continue(vec![self.reply(
                    &msg,
                    cmd::SESS_KEY_NEG_RESP,
                    payload,
                    false,
                )]);
            }
            Phase::AwaitFinish { client, device } => {
                if msg.cmd != cmd::SESS_KEY_NEG_FINISH {
                    return Flow::Continue(Vec::new());
                }
                let expected = self.shared.suite.hmac_sha256(&self.key, device);
                if msg.payload.get(..32) != Some(&expected[..]) {
                    self.shared.stat(|s| s.key_rejections += 1);
                    return Flow::Close;
                }
                let session =
                    self.shared
                        .suite
                        .session_key(self.shared.version, &self.key, client, device);
                self.wire.set_key(session);
                self.phase = Phase::Ready;
                self.shared.stat(|s| s.negotiations += 1);
                return Flow::Continue(Vec::new());
            }
            Phase::Ready => {}
        }

        match msg.cmd {
            cmd::HEART_BEAT => Flow::Continue(vec![self.ack(&msg)]),
            cmd::DP_QUERY | cmd::DP_QUERY_NEW => {
                let dps = self.shared.dps();
                let body = if self.shared.version.uses_session_key() {
                    json!({"dps": dps})
                } else {
                    json!({"devId": self.shared.dev_id.0, "dps": dps, "t": now()})
                };
                Flow::Continue(vec![self.reply(
                    &msg,
                    msg.cmd,
                    body.to_string().into_bytes(),
                    false,
                )])
            }
            cmd::CONTROL | cmd::CONTROL_NEW => {
                if !self.shared.rate_ok() {
                    self.shared.stat(|s| s.rate_limited += 1);
                    return Flow::Continue(Vec::new());
                }
                let requested = control_dps(&msg.payload);
                // El push sale por `changes` hacia todas las conexiones, esta incluida.
                self.shared.apply(&requested, true);
                if self.shared.real_firmware {
                    // El foco real no confirma: el push con el estado nuevo es la respuesta.
                    return Flow::Continue(Vec::new());
                }
                Flow::Continue(vec![self.ack(&msg)])
            }
            cmd::UPDATEDPS => {
                let current = self.shared.dps();
                let wanted: Dps = serde_json::from_slice::<Value>(&msg.payload)
                    .ok()
                    .and_then(|v| v.get("dpId").cloned())
                    .and_then(|v| v.as_array().cloned())
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|dp| {
                        let dp = match dp {
                            Value::String(s) => s.clone(),
                            other => other.to_string(),
                        };
                        current.get(&dp).map(|v| (dp, v.clone()))
                    })
                    .collect();
                let mut out = vec![self.ack(&msg)];
                if !wanted.is_empty() {
                    out.push(self.status_push(&wanted));
                }
                Flow::Continue(out)
            }
            _ => Flow::Continue(vec![self.ack(&msg)]),
        }
    }
}

/// DPs de un payload de control: `{"dps": {...}}` (3.1/3.3) o `{"data": {"dps": {...}}}` (3.4+).
fn control_dps(payload: &[u8]) -> Dps {
    let Ok(v) = serde_json::from_slice::<Value>(payload) else {
        return Dps::new();
    };
    let dps = v
        .get("dps")
        .or_else(|| v.get("data").and_then(|d| d.get("dps")));
    match dps {
        Some(Value::Object(map)) => map.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        _ => Dps::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_lee_los_dos_formatos() {
        let a = control_dps(br#"{"devId":"x","uid":"x","t":"1","dps":{"20":true}}"#);
        assert_eq!(a["20"], json!(true));
        let b = control_dps(br#"{"protocol":5,"t":1,"data":{"dps":{"21":"colour"}}}"#);
        assert_eq!(b["21"], json!("colour"));
        assert!(control_dps(b"basura").is_empty());
    }
}
