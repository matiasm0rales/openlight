//! Servidor TCP y la API para pruebas ([`SimHandle`]).

use std::collections::VecDeque;
use std::io;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use openlight_types::{DeviceId, DpMapping, Dps, ProtocolVersion};
use serde_json::Value;
use tokio::net::TcpListener;
use tokio::sync::{Notify, broadcast, watch};
use tokio::task::JoinHandle;

use crate::conn;
use crate::profile::accepts;
use crate::{Faults, SimConfig, SimStats, Suite};

/// Estado compartido entre el servidor, las conexiones y el [`SimHandle`].
pub(crate) struct Shared {
    pub version: ProtocolVersion,
    pub dev_id: DeviceId,
    pub local_key: [u8; 16],
    pub mapping: DpMapping,
    pub suite: Arc<dyn Suite>,
    dps: Mutex<Dps>,
    stats: Mutex<SimStats>,
    writes: Mutex<Vec<Dps>>,
    rate: Mutex<VecDeque<Instant>>,
    /// DPs que cambiaron: cada conexión los manda como push de `STATUS`.
    pub changes: broadcast::Sender<Dps>,
    pub faults: watch::Sender<Faults>,
    /// Se incrementa para cortar todas las conexiones abiertas.
    pub kill: watch::Sender<u64>,
    pub shutdown: watch::Sender<bool>,
    pub active: AtomicUsize,
    notify: Notify,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Shared {
    pub fn dps(&self) -> Dps {
        lock(&self.dps).clone()
    }

    pub fn faults(&self) -> Faults {
        self.faults.borrow().clone()
    }

    pub fn stat(&self, f: impl FnOnce(&mut SimStats)) {
        f(&mut lock(&self.stats));
        self.notify.notify_waiters();
    }

    /// Aplica los DPs que el foco acepta y avisa a todas las conexiones. Devuelve los aceptados.
    pub fn apply(&self, requested: &Dps, record: bool) -> Dps {
        let accepted: Dps = requested
            .iter()
            .filter(|(dp, v)| self.mapping.0.get(*dp).is_some_and(|s| accepts(s, v)))
            .map(|(dp, v)| (dp.clone(), v.clone()))
            .collect();
        if !accepted.is_empty() {
            lock(&self.dps).extend(accepted.clone());
            if record {
                lock(&self.writes).push(accepted.clone());
            }
            // Sin conexiones no hay receptores: no es un error.
            let _ = self.changes.send(accepted.clone());
        }
        self.notify.notify_waiters();
        accepted
    }

    /// Si un comando de control entra en el límite por segundo (y lo cuenta).
    pub fn rate_ok(&self) -> bool {
        let Some(max) = self.faults().max_commands_per_s else {
            return true;
        };
        let now = Instant::now();
        let mut window = lock(&self.rate);
        while window
            .front()
            .is_some_and(|t| now.duration_since(*t) >= Duration::from_secs(1))
        {
            window.pop_front();
        }
        if window.len() < max as usize {
            window.push_back(now);
            true
        } else {
            false
        }
    }
}

/// Un foco simulado en marcha. Al soltarlo se apaga.
pub struct SimHandle {
    shared: Arc<Shared>,
    addr: SocketAddr,
    tasks: Vec<JoinHandle<()>>,
}

/// Arranca un foco simulado con la [`Suite`] dada.
pub async fn start_with(config: SimConfig, suite: Arc<dyn Suite>) -> io::Result<SimHandle> {
    let listener = TcpListener::bind(config.bind).await?;
    let addr = listener.local_addr()?;
    let mapping = config.profile.mapping();
    let dps = config
        .initial_dps
        .clone()
        .unwrap_or_else(|| config.profile.default_dps());
    let (changes, _) = broadcast::channel(64);
    let shared = Arc::new(Shared {
        version: config.version,
        dev_id: config.dev_id.clone(),
        local_key: config.local_key,
        mapping,
        suite,
        dps: Mutex::new(dps),
        stats: Mutex::new(SimStats::default()),
        writes: Mutex::new(Vec::new()),
        rate: Mutex::new(VecDeque::new()),
        changes,
        faults: watch::channel(config.faults.clone()).0,
        kill: watch::channel(0).0,
        shutdown: watch::channel(false).0,
        active: AtomicUsize::new(0),
        notify: Notify::new(),
    });

    let mut tasks = vec![tokio::spawn(accept_loop(listener, shared.clone()))];
    if let Some(b) = &config.broadcast {
        let ip = b.advertised_ip.unwrap_or_else(|| {
            if addr.ip().is_unspecified() {
                std::net::Ipv4Addr::LOCALHOST.into()
            } else {
                addr.ip()
            }
        });
        tasks.extend(crate::udp::spawn(shared.clone(), b.clone(), ip, config.product_key.clone()).await?);
    }
    tracing::info!(%addr, version = %config.version, "foco simulado escuchando");
    Ok(SimHandle {
        shared,
        addr,
        tasks,
    })
}

async fn accept_loop(listener: TcpListener, shared: Arc<Shared>) {
    let mut shutdown = shared.shutdown.subscribe();
    loop {
        let accepted = tokio::select! {
            r = listener.accept() => r,
            _ = shutdown.changed() => return,
        };
        let (sock, peer) = match accepted {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, "accept falló");
                tokio::time::sleep(Duration::from_millis(50)).await;
                continue;
            }
        };
        let f = shared.faults();
        if f.occupied || (f.exclusive && shared.active.load(Ordering::SeqCst) > 0) {
            tracing::debug!(%peer, "conexión rechazada: el foco ya está ocupado");
            shared.stat(|s| s.rejected += 1);
            drop(sock);
            continue;
        }
        shared.active.fetch_add(1, Ordering::SeqCst);
        shared.stat(|s| s.connections += 1);
        let _ = sock.set_nodelay(true);
        tokio::spawn(conn::run(shared.clone(), sock, peer));
    }
}

impl SimHandle {
    /// Dirección TCP donde escucha (con el puerto real si se pidió uno efímero).
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Puerto TCP.
    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    /// Versión del protocolo.
    pub fn version(&self) -> ProtocolVersion {
        self.shared.version
    }

    /// Id del dispositivo.
    pub fn dev_id(&self) -> &DeviceId {
        &self.shared.dev_id
    }

    /// Clave local.
    pub fn local_key(&self) -> [u8; 16] {
        self.shared.local_key
    }

    /// El mapeo de DPs del foco.
    pub fn mapping(&self) -> &DpMapping {
        &self.shared.mapping
    }

    /// DPs actuales.
    pub fn dps(&self) -> Dps {
        self.shared.dps()
    }

    /// Valor actual de un DP.
    pub fn dp(&self, dp: &str) -> Option<Value> {
        self.shared.dps().get(dp).cloned()
    }

    /// Cambia DPs "desde afuera" (como el interruptor o la app Smart Life). Manda el push de
    /// `STATUS` a las conexiones abiertas. Devuelve los DPs aceptados.
    pub fn set_dps(&self, dps: Dps) -> Dps {
        self.shared.apply(&dps, false)
    }

    /// Escrituras recibidas por la red, en orden (solo los DPs aceptados).
    pub fn writes(&self) -> Vec<Dps> {
        lock(&self.shared.writes).clone()
    }

    /// Fallas activas.
    pub fn faults(&self) -> Faults {
        self.shared.faults()
    }

    /// Reemplaza las fallas activas. Rige en caliente para las conexiones abiertas.
    pub fn set_faults(&self, faults: Faults) {
        self.shared.faults.send_replace(faults);
    }

    /// Modifica las fallas activas.
    pub fn update_faults(&self, f: impl FnOnce(&mut Faults)) {
        self.shared.faults.send_modify(f);
    }

    /// Contadores.
    pub fn stats(&self) -> SimStats {
        lock(&self.shared.stats).clone()
    }

    /// Pone los contadores (y las escrituras) en cero.
    pub fn reset_stats(&self) {
        *lock(&self.shared.stats) = SimStats::default();
        lock(&self.shared.writes).clear();
    }

    /// Conexiones abiertas ahora.
    pub fn active_connections(&self) -> usize {
        self.shared.active.load(Ordering::SeqCst)
    }

    /// Corta todas las conexiones abiertas (como un foco que se reinicia).
    pub fn disconnect_all(&self) {
        self.shared.kill.send_modify(|n| *n += 1);
    }

    /// Espera hasta que `cond` se cumpla sobre los contadores, o hasta `timeout`.
    /// Devuelve si se cumplió.
    pub async fn wait_until(&self, timeout: Duration, cond: impl Fn(&SimStats) -> bool) -> bool {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let notified = self.shared.notify.notified();
            if cond(&self.stats()) {
                return true;
            }
            if tokio::time::timeout_at(deadline, notified).await.is_err() {
                return cond(&self.stats());
            }
        }
    }

    /// Apaga el simulador y corta todas las conexiones.
    pub async fn shutdown(mut self) {
        self.shared.shutdown.send_replace(true);
        for t in self.tasks.drain(..) {
            let _ = t.await;
        }
        let deadline = Instant::now() + Duration::from_secs(1);
        while self.active_connections() > 0 && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }
}

impl Drop for SimHandle {
    fn drop(&mut self) {
        self.shared.shutdown.send_replace(true);
        for t in &self.tasks {
            t.abort();
        }
    }
}
