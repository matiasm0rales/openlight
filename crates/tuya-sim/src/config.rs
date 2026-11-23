//! Configuración del simulador, fallas inyectables y contadores.

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use openlight_types::{DeviceId, Dps, ProtocolVersion};

use crate::Profile;

/// Id falso por defecto (22 caracteres, como los reales).
pub const DEFAULT_DEV_ID: &str = "openlightsimdevice0001";
/// Clave local falsa por defecto (16 caracteres).
pub const DEFAULT_LOCAL_KEY: &str = "OpenLightSimKey1";
/// Puerto TCP de los focos Tuya.
pub const TCP_PORT: u16 = 6668;

/// Cómo arrancar un foco simulado.
#[derive(Debug, Clone)]
pub struct SimConfig {
    /// Versión del protocolo que habla.
    pub version: ProtocolVersion,
    /// Id del dispositivo.
    pub dev_id: DeviceId,
    /// Clave local (16 bytes).
    pub local_key: [u8; 16],
    /// Dirección TCP donde escucha. Puerto 0 = efímero.
    pub bind: SocketAddr,
    /// Esquema de DPs.
    pub profile: Profile,
    /// DPs iniciales. `None` = los del perfil.
    pub initial_dps: Option<Dps>,
    /// `productKey` que se anuncia por UDP.
    pub product_key: String,
    /// Broadcasts UDP. `None` = no se anuncia (lo normal en las pruebas).
    pub broadcast: Option<BroadcastConfig>,
    /// Fallas activas desde el arranque.
    pub faults: Faults,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            version: ProtocolVersion::V35,
            dev_id: DeviceId(DEFAULT_DEV_ID.into()),
            local_key: *b"OpenLightSimKey1",
            bind: SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), TCP_PORT),
            profile: Profile::V2,
            initial_dps: None,
            product_key: "openlightsimprod".into(),
            broadcast: None,
            faults: Faults::default(),
        }
    }
}

impl SimConfig {
    /// Configuración por defecto con la versión dada.
    pub fn new(version: ProtocolVersion) -> Self {
        Self {
            version,
            ..Self::default()
        }
    }

    /// Escucha en `127.0.0.1` con un puerto efímero (para pruebas).
    pub fn ephemeral(mut self) -> Self {
        self.bind = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        self
    }

    /// Cambia la dirección de escucha.
    pub fn bind(mut self, addr: SocketAddr) -> Self {
        self.bind = addr;
        self
    }

    /// Cambia el perfil de DPs.
    pub fn profile(mut self, profile: Profile) -> Self {
        self.profile = profile;
        self
    }

    /// Cambia el id.
    pub fn dev_id(mut self, id: impl Into<String>) -> Self {
        self.dev_id = DeviceId(id.into());
        self
    }

    /// Cambia la clave local.
    pub fn local_key(mut self, key: [u8; 16]) -> Self {
        self.local_key = key;
        self
    }

    /// DPs iniciales propios.
    pub fn dps(mut self, dps: Dps) -> Self {
        self.initial_dps = Some(dps);
        self
    }

    /// Activa los broadcasts UDP.
    pub fn broadcast(mut self, broadcast: BroadcastConfig) -> Self {
        self.broadcast = Some(broadcast);
        self
    }

    /// Fallas activas desde el arranque.
    pub fn faults(mut self, faults: Faults) -> Self {
        self.faults = faults;
        self
    }
}

/// Broadcasts UDP como los de un foco real (ver `tinytuya/scanner.py`).
#[derive(Debug, Clone)]
pub struct BroadcastConfig {
    /// A dónde se mandan los anuncios (por defecto `255.255.255.255`).
    pub target: IpAddr,
    /// Puerto de los anuncios. `None` = el de la versión: 6666 (3.1) o 6667 (3.3 a 3.5).
    pub port: Option<u16>,
    /// Cada cuánto se anuncia.
    pub interval: Duration,
    /// IP que el foco dice tener en el anuncio. `None` = la de `bind` (o `127.0.0.1`).
    pub advertised_ip: Option<IpAddr>,
    /// Puerto donde escucha `REQ_DEVINFO` (7000). `None` = no escucha.
    pub listen_port: Option<u16>,
    /// Puerto al que responde `REQ_DEVINFO` (7000, a la IP del pedido).
    pub reply_port: u16,
}

impl Default for BroadcastConfig {
    fn default() -> Self {
        Self {
            target: IpAddr::V4(Ipv4Addr::BROADCAST),
            port: None,
            interval: Duration::from_secs(5),
            advertised_ip: None,
            listen_port: Some(7000),
            reply_port: 7000,
        }
    }
}

/// Fallas inyectables. Se pueden cambiar en caliente con [`crate::SimHandle::set_faults`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Faults {
    /// Cierra cada conexión cuando cumple este tiempo (los cortes del log del script).
    pub drop_every: Option<Duration>,
    /// Lee todo y no responde nada (ni pushes): el foco "colgado".
    pub mute: bool,
    /// El foco tiene otra clave: no puede autenticar al cliente y corta.
    ///
    /// En 3.4/3.5 la negociación de sesión falla y tinytuya devuelve **914**
    /// (`_get_socket` → `ERR_KEY_OR_VER`). En 3.1/3.3 no hay negociación: tinytuya ve el corte
    /// como 904 (`ERR_PAYLOAD`). Al activarla se cortan también las conexiones abiertas, como
    /// un foco que se re-vinculó.
    pub reject_key: bool,
    /// Demora antes de cada respuesta.
    pub latency: Duration,
    /// Máximo de comandos de control por segundo. Los que sobran se ignoran sin respuesta.
    pub max_commands_per_s: Option<u32>,
    /// Acepta una sola conexión a la vez: la segunda se cierra apenas llega.
    pub exclusive: bool,
    /// Otra app (Smart Life) ocupa la conexión: toda conexión nueva se cierra apenas llega.
    pub occupied: bool,
}

/// Contadores del simulador.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SimStats {
    /// Conexiones aceptadas.
    pub connections: u32,
    /// Conexiones rechazadas por `exclusive` u `occupied`.
    pub rejected: u32,
    /// Conexiones cortadas por `drop_every`.
    pub drops: u32,
    /// Conexiones cortadas por no poder autenticar al cliente (`reject_key`).
    pub key_rejections: u32,
    /// Negociaciones de clave de sesión completas.
    pub negotiations: u32,
    /// Frames recibidos, por comando.
    pub by_cmd: BTreeMap<u32, u32>,
    /// Comandos de control ignorados por `max_commands_per_s`.
    pub rate_limited: u32,
    /// Frames ignorados por `mute`.
    pub muted: u32,
    /// Pushes de `STATUS` enviados.
    pub pushes: u32,
}

impl SimStats {
    /// Frames recibidos de un comando.
    pub fn count(&self, cmd: u32) -> u32 {
        self.by_cmd.get(&cmd).copied().unwrap_or(0)
    }

    /// Frames recibidos en total.
    pub fn commands(&self) -> u32 {
        self.by_cmd.values().sum()
    }

    /// Escrituras de DPs (`CONTROL` + `CONTROL_NEW`).
    pub fn controls(&self) -> u32 {
        self.count(crate::cmd::CONTROL) + self.count(crate::cmd::CONTROL_NEW)
    }

    /// Consultas (`DP_QUERY` + `DP_QUERY_NEW`).
    pub fn queries(&self) -> u32 {
        self.count(crate::cmd::DP_QUERY) + self.count(crate::cmd::DP_QUERY_NEW)
    }

    /// Latidos.
    pub fn heartbeats(&self) -> u32 {
        self.count(crate::cmd::HEART_BEAT)
    }
}
