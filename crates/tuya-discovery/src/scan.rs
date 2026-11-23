//! Sockets: escucha de broadcasts y envío periódico de `REQ_DEVINFO`.
//!
//! Como `tinytuya/scanner.py::devices`: un socket por puerto (6666, 6667 y 7000), atado a todas
//! las interfaces, y cada [`BROADCAST_INTERVAL`] un `REQ_DEVINFO` al broadcast de cada interfaz,
//! puerto 7000, para que los focos 3.5 se anuncien (`send_discovery_request`).

use std::collections::HashMap;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use openlight_types::ipc::DiscoveredDevice;
use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::task::JoinSet;

use crate::parse::{Datagram, parse_datagram, req_devinfo};

/// Puerto de los broadcasts en texto plano (3.1).
pub const PORT_PLAIN: u16 = 6666;
/// Puerto de los broadcasts cifrados con la clave UDP (3.3/3.4).
pub const PORT_ENCRYPTED: u16 = 6667;
/// Puerto de la app y de los broadcasts 3.5 (6699 + GCM).
pub const PORT_APP: u16 = 7000;
/// Cada cuánto se repite el `REQ_DEVINFO` (`BROADCASTTIME = 6` en `scanner.py`).
pub const BROADCAST_INTERVAL: Duration = Duration::from_secs(6);
/// Tamaño del búfer de recepción (`sock.recvfrom(4048)`).
const RECV_BUFFER: usize = 4048;

/// Error de los sockets.
#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    /// No se pudo escuchar en ninguno de los puertos.
    #[error("no se pudo escuchar en ningún puerto UDP: {0}")]
    Bind(io::Error),
    /// Error de entrada/salida.
    #[error("error de red: {0}")]
    Io(#[from] io::Error),
}

/// Configuración de una búsqueda.
#[derive(Debug, Clone)]
pub struct ScanConfig {
    /// Dirección local donde escuchar (por defecto `0.0.0.0`).
    pub listen_ip: IpAddr,
    /// Puertos donde escuchar (por defecto 6666, 6667 y 7000).
    pub ports: Vec<u16>,
    /// A dónde mandar el `REQ_DEVINFO`: `(ip local de la interfaz, destino)`. `None` = el
    /// broadcast de cada interfaz IPv4, puerto 7000, como `get_ip_to_broadcast`.
    pub request_targets: Option<Vec<(IpAddr, SocketAddr)>>,
    /// Cada cuánto repetir el pedido. `None` = no mandar pedidos (solo escuchar).
    pub request_interval: Option<Duration>,
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            listen_ip: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            ports: vec![PORT_PLAIN, PORT_ENCRYPTED, PORT_APP],
            request_targets: None,
            request_interval: Some(BROADCAST_INTERVAL),
        }
    }
}

/// Socket UDP con `SO_REUSEADDR` (y `SO_REUSEPORT` en Unix) para convivir con otros que
/// escuchen el mismo puerto, y con broadcast habilitado.
fn bind_udp(addr: SocketAddr) -> io::Result<UdpSocket> {
    let socket = Socket::new(Domain::for_address(addr), Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;
    #[cfg(all(unix, not(any(target_os = "solaris", target_os = "illumos"))))]
    socket.set_reuse_port(true)?;
    socket.set_broadcast(true)?;
    socket.set_nonblocking(true)?;
    socket.bind(&addr.into())?;
    UdpSocket::from_std(socket.into())
}

/// Broadcast de cada interfaz IPv4 que no sea loopback, con su IP local.
/// Si no hay ninguna, `255.255.255.255` (como `get_ip_to_broadcast`).
pub fn default_request_targets() -> Vec<(IpAddr, SocketAddr)> {
    let mut out: Vec<(IpAddr, SocketAddr)> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| !i.is_loopback())
        .filter_map(|i| match i.addr {
            if_addrs::IfAddr::V4(v4) => {
                let bcast = v4.broadcast?;
                (bcast != v4.ip).then_some((
                    IpAddr::V4(v4.ip),
                    SocketAddr::new(IpAddr::V4(bcast), PORT_APP),
                ))
            }
            if_addrs::IfAddr::V6(_) => None,
        })
        .collect();
    if out.is_empty() {
        out.push((
            IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            SocketAddr::new(IpAddr::V4(Ipv4Addr::BROADCAST), PORT_APP),
        ));
    }
    out
}

/// Manda un `REQ_DEVINFO` a cada destino. Devuelve cuántos salieron; los que fallan se saltean,
/// como en tinytuya ("skipping interface").
pub async fn send_requests(targets: &[(IpAddr, SocketAddr)]) -> usize {
    let mut sent = 0;
    for (local, target) in targets {
        let Ok(socket) = bind_udp(SocketAddr::new(*local, 0)) else {
            continue;
        };
        if socket.send_to(&req_devinfo(*local), target).await.is_ok() {
            sent += 1;
        }
    }
    sent
}

/// Escucha hasta que se cierre `tx`. Cada foco nuevo, o que cambió de IP o de versión, se manda
/// por el canal (sirve para el evento `discovery://found`).
///
/// Falla solo si no pudo abrir ningún puerto; si algunos están ocupados, sigue con los demás.
pub async fn listen(
    config: &ScanConfig,
    tx: mpsc::Sender<DiscoveredDevice>,
) -> Result<(), DiscoveryError> {
    let mut sockets = Vec::new();
    let mut last_error = None;
    for port in &config.ports {
        match bind_udp(SocketAddr::new(config.listen_ip, *port)) {
            Ok(s) => sockets.push(s),
            Err(e) => last_error = Some(e),
        }
    }
    if sockets.is_empty() {
        return Err(DiscoveryError::Bind(last_error.unwrap_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "sin puertos")
        })));
    }

    let (raw_tx, mut raw_rx) = mpsc::channel::<(Vec<u8>, SocketAddr)>(64);
    let mut tasks = JoinSet::new();
    for socket in sockets {
        let raw_tx = raw_tx.clone();
        tasks.spawn(async move {
            let mut buf = vec![0u8; RECV_BUFFER];
            loop {
                match socket.recv_from(&mut buf).await {
                    Ok((n, src)) => {
                        if raw_tx.send((buf[..n].to_vec(), src)).await.is_err() {
                            return;
                        }
                    }
                    // Windows devuelve ConnectionReset en UDP después de un ICMP: se ignora.
                    Err(_) => tokio::time::sleep(Duration::from_millis(50)).await,
                }
            }
        });
    }
    drop(raw_tx);

    let targets = config
        .request_targets
        .clone()
        .unwrap_or_else(default_request_targets);
    let mut ticker = config.request_interval.map(tokio::time::interval);
    let mut seen: HashMap<String, DiscoveredDevice> = HashMap::new();

    loop {
        tokio::select! {
            () = tx.closed() => return Ok(()),
            _ = async {
                match ticker.as_mut() {
                    Some(t) => t.tick().await,
                    None => std::future::pending().await,
                }
            } => {
                send_requests(&targets).await;
            }
            received = raw_rx.recv() => {
                let Some((data, src)) = received else { return Ok(()) };
                if let Datagram::Device(device) = parse_datagram(&data, src.ip()) {
                    if seen.get(&device.id.0) == Some(&device) {
                        continue;
                    }
                    seen.insert(device.id.0.clone(), device.clone());
                    if tx.send(device).await.is_err() {
                        return Ok(());
                    }
                }
            }
        }
    }
}

/// Busca durante `duration` y devuelve los focos encontrados, uno por id (el último anuncio
/// gana), en el orden en que aparecieron.
pub async fn scan(
    config: &ScanConfig,
    duration: Duration,
) -> Result<Vec<DiscoveredDevice>, DiscoveryError> {
    let (tx, mut rx) = mpsc::channel(64);
    let mut found: Vec<DiscoveredDevice> = Vec::new();
    let listener = listen(config, tx);
    tokio::pin!(listener);
    let deadline = tokio::time::sleep(duration);
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            result = &mut listener => {
                result?;
                break;
            }
            () = &mut deadline => break,
            Some(device) = rx.recv() => {
                match found.iter_mut().find(|d| d.id == device.id) {
                    Some(existing) => *existing = device,
                    None => found.push(device),
                }
            }
        }
    }
    // Lo que haya quedado en el canal también cuenta.
    while let Ok(device) = rx.try_recv() {
        match found.iter_mut().find(|d| d.id == device.id) {
            Some(existing) => *existing = device,
            None => found.push(device),
        }
    }
    Ok(found)
}
