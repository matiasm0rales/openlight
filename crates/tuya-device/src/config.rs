//! Configuración de un foco: credenciales, tiempos, IP y quién la busca.

use std::net::IpAddr;
use std::time::Duration;

use async_trait::async_trait;
use openlight_types::{
    Capabilities, DeviceCreds, DeviceId, ProtocolVersion, SendChannel, Settings,
};

/// Puerto TCP de los focos Tuya.
pub use tuya_proto::consts::TCP_PORT;

/// Cómo se resuelve la IP del foco.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IpMode {
    /// `config.json` trae la IP: es la que manda y nunca se busca otra (`auto_ip = False`).
    Fixed,
    /// La IP guardada es solo la última que funcionó: si deja de servir, se busca de nuevo con
    /// discovery (`auto_ip = True`).
    #[default]
    Auto,
}

/// Tiempos de la conexión. Los de `heartbeat`, `dead_after` y `retry_every` son los ajustes
/// `heartbeat_s`, `dead_after_s` y `retry_every_s`; el resto son constantes del script.
#[derive(Debug, Clone, PartialEq)]
pub struct Timings {
    /// Entre señales de vida (`Foco.LATIDO_CADA`, 10 s).
    pub heartbeat: Duration,
    /// Sin recibir nada durante este tiempo la conexión se da por muerta
    /// (`Foco.SIN_RESPUESTA_MAX`, 30 s).
    pub dead_after: Duration,
    /// Entre intentos de reconexión (`Foco.REINTENTO_CADA`, 3 s).
    pub retry_every: Duration,
    /// Tope del backoff: la espera entre intentos crece al doble en cada fallo hasta
    /// `retry_every × backoff_max_factor`. Con 1 es una espera fija, como en el script.
    pub backoff_max_factor: u32,
    /// Una escritura o una respuesta esperada tarda como mucho esto (`set_socketTimeout(2)`).
    pub io_timeout: Duration,
    /// Conectar el socket (y negociar la clave de sesión) tarda como mucho esto.
    pub connect_timeout: Duration,
    /// Al arrancar, cuánto se espera a que el foco aparezca (`Foco(espera=45)`).
    pub startup_wait: Duration,
    /// Entre intentos al arrancar (`REINTENTO_ARRANQUE`, 1 s).
    pub startup_retry: Duration,
}

impl Timings {
    /// Los tiempos que salen de los ajustes (el resto con los valores del script).
    pub fn from_settings(settings: &Settings) -> Self {
        let secs = |s: f32| Duration::from_secs_f32(s.max(0.0));
        Self {
            heartbeat: secs(settings.heartbeat_s),
            dead_after: secs(settings.dead_after_s),
            retry_every: secs(settings.retry_every_s),
            backoff_max_factor: 4,
            io_timeout: Duration::from_secs(2),
            connect_timeout: Duration::from_secs(3),
            startup_wait: Duration::from_secs(45),
            startup_retry: Duration::from_secs(1),
        }
    }

    /// Cada cuánto sale un latido de verdad: `heartbeat`, pero nunca más de la mitad de
    /// `dead_after`. Los ajustes permiten un latido más largo que `dead_after`; en ese caso la
    /// conexión se daría por muerta antes de poder sondearla.
    pub fn heartbeat_interval(&self) -> Duration {
        self.heartbeat.min(self.dead_after / 2)
    }

    /// Espera antes del próximo intento tras `failures` intentos fallidos seguidos (0 = el
    /// último intento salió bien o es el primero): `retry_every`, ×2, ×4… hasta el tope.
    pub fn backoff(&self, failures: u32) -> Duration {
        let factor = 1u32
            .checked_shl(failures.saturating_sub(1))
            .unwrap_or(u32::MAX)
            .clamp(1, self.backoff_max_factor.max(1));
        self.retry_every * factor
    }
}

impl Default for Timings {
    fn default() -> Self {
        Self::from_settings(&Settings::default())
    }
}

/// Todo lo que el actor necesita saber de un foco.
#[derive(Debug, Clone)]
pub struct DeviceConfig {
    /// Id, clave local, versión del protocolo e IP inicial.
    pub creds: DeviceCreds,
    /// Capacidades (del mapeo de la nube). `None` = deducirlas de `status()` con la heurística
    /// de DPs.
    pub capabilities: Option<Capabilities>,
    /// Si la IP de `creds` es fija o solo la última conocida.
    pub ip_mode: IpMode,
    /// Puerto TCP del foco.
    pub port: u16,
    /// Por dónde se le manda el color (ajuste `send_channel`).
    pub channel: SendChannel,
    /// Tiempos.
    pub timings: Timings,
}

impl DeviceConfig {
    /// Configuración con los valores por defecto del script.
    pub fn new(creds: DeviceCreds) -> Self {
        Self {
            creds,
            capabilities: None,
            ip_mode: IpMode::Auto,
            port: TCP_PORT,
            channel: SendChannel::Colour,
            timings: Timings::default(),
        }
    }

    /// Fija las capacidades.
    #[must_use]
    pub fn capabilities(mut self, capabilities: Capabilities) -> Self {
        self.capabilities = Some(capabilities);
        self
    }

    /// Cambia cómo se resuelve la IP.
    #[must_use]
    pub fn ip_mode(mut self, ip_mode: IpMode) -> Self {
        self.ip_mode = ip_mode;
        self
    }

    /// Cambia el puerto TCP.
    #[must_use]
    pub fn port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// Cambia el canal de envío del color.
    #[must_use]
    pub fn channel(mut self, channel: SendChannel) -> Self {
        self.channel = channel;
        self
    }

    /// Cambia los tiempos.
    #[must_use]
    pub fn timings(mut self, timings: Timings) -> Self {
        self.timings = timings;
        self
    }
}

/// Dónde está un foco en la red y qué versión del protocolo anuncia.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Located {
    /// IP actual del foco.
    pub ip: IpAddr,
    /// Versión que anuncia el foco (`None` si el locator no la sabe). Si difiere de la
    /// configurada, el actor usa la anunciada y la guarda.
    pub version: Option<ProtocolVersion>,
}

impl Located {
    /// Solo la IP.
    pub fn ip(ip: IpAddr) -> Self {
        Self { ip, version: None }
    }

    /// La IP y la versión que anuncia el foco.
    pub fn with_version(ip: IpAddr, version: ProtocolVersion) -> Self {
        Self {
            ip,
            version: Some(version),
        }
    }
}

/// Quién encuentra un foco en la red (discovery UDP): su IP y la versión que anuncia.
///
/// En producción es [`crate::UdpLocator`]; las pruebas usan uno falso.
#[async_trait]
pub trait Locator: Send + Sync {
    /// Dónde está ahora el foco `id`, o `None` si no apareció.
    async fn locate(&self, id: &DeviceId) -> Option<Located>;
}

/// Un [`Locator`] que nunca encuentra nada: para focos con IP fija.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoLocator;

#[async_trait]
impl Locator for NoLocator {
    async fn locate(&self, _id: &DeviceId) -> Option<Located> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn los_tiempos_por_defecto_son_los_del_script() {
        let t = Timings::default();
        assert_eq!(t.heartbeat, Duration::from_secs(10)); // Foco.LATIDO_CADA
        assert_eq!(t.dead_after, Duration::from_secs(30)); // Foco.SIN_RESPUESTA_MAX
        assert_eq!(t.retry_every, Duration::from_secs(3)); // Foco.REINTENTO_CADA
        assert_eq!(t.startup_wait, Duration::from_secs(45)); // Foco(espera=45)
        assert_eq!(t.startup_retry, Duration::from_secs(1)); // REINTENTO_ARRANQUE
        assert_eq!(t.io_timeout, Duration::from_secs(2)); // set_socketTimeout(2)
    }

    #[test]
    fn los_ajustes_mandan_sobre_los_tiempos() {
        let settings = Settings {
            heartbeat_s: 5.0,
            dead_after_s: 12.5,
            retry_every_s: 1.0,
            ..Settings::default()
        };
        let t = Timings::from_settings(&settings);
        assert_eq!(t.heartbeat, Duration::from_secs(5));
        assert_eq!(t.dead_after, Duration::from_millis(12_500));
        assert_eq!(t.retry_every, Duration::from_secs(1));
    }

    #[test]
    fn el_latido_siempre_sale_antes_de_dar_la_conexion_por_muerta() {
        let t = Timings::default();
        assert_eq!(
            t.heartbeat_interval(),
            t.heartbeat,
            "10 s con dead_after de 30 s"
        );
        let t = Timings {
            heartbeat: Duration::from_secs(30),
            dead_after: Duration::from_secs(5),
            ..Timings::default()
        };
        assert_eq!(t.heartbeat_interval(), Duration::from_millis(2500));
        let t = Timings {
            heartbeat: Duration::from_secs(10),
            dead_after: Duration::from_secs(10),
            ..Timings::default()
        };
        assert_eq!(
            t.heartbeat_interval(),
            Duration::from_secs(5),
            "también si son iguales"
        );
    }

    #[test]
    fn el_backoff_duplica_hasta_el_tope() {
        let t = Timings::default();
        let segs: Vec<u64> = (0..7).map(|n| t.backoff(n).as_secs()).collect();
        // 0 fallos = el primero; después 3, 6, 12 y tope en ×4.
        assert_eq!(segs, [3, 3, 6, 12, 12, 12, 12]);
        let fija = Timings {
            backoff_max_factor: 1,
            ..Timings::default()
        };
        assert!((0..6).all(|n| fija.backoff(n) == fija.retry_every));
        // Sin desbordar con muchísimos fallos.
        assert_eq!(t.backoff(u32::MAX), Duration::from_secs(12));
    }
}
