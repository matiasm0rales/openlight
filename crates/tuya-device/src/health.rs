//! Salud del enlace: cortes, motivos, reconexiones y latencia (`LinkHealth`).
//!
//! Sirve para investigar los 272 cortes del log del script: cada corte queda con su motivo
//! (EOF del foco, timeout, sin respuesta, 914 o error de IO) y la hora.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use openlight_types::{DeviceError, DisconnectReason, LinkHealth};
use tokio::time::Instant;

/// Ventana de `disconnects_last_hour`.
const HOUR: Duration = Duration::from_secs(3600);

/// Bytes que pasaron por el socket. Se comparten con el handle para que `health()` los vea al
/// instante sin que el actor tenga que publicar cada lectura.
#[derive(Debug, Default)]
pub(crate) struct Counters {
    rx: AtomicU64,
    tx: AtomicU64,
}

impl Counters {
    pub fn add_rx(&self, n: usize) {
        self.rx.fetch_add(n as u64, Ordering::Relaxed);
    }

    pub fn add_tx(&self, n: usize) {
        self.tx.fetch_add(n as u64, Ordering::Relaxed);
    }

    fn get(counter: &AtomicU64) -> u32 {
        u32::try_from(counter.load(Ordering::Relaxed)).unwrap_or(u32::MAX)
    }
}

/// Lleva la cuenta de lo que le pasa al enlace. Lo usa solo el actor.
#[derive(Debug, Default)]
pub(crate) struct HealthTracker {
    health: LinkHealth,
    cuts: VecDeque<Instant>,
    heartbeat_sent: Option<Instant>,
    had_connection: bool,
}

/// El motivo de corte que corresponde a un error.
pub(crate) fn reason_of(error: &DeviceError) -> DisconnectReason {
    match error {
        DeviceError::Disconnected(reason) => reason.clone(),
        DeviceError::Timeout => DisconnectReason::Timeout,
        DeviceError::KeyRejected => DisconnectReason::KeyRejected,
        DeviceError::NotFound => DisconnectReason::Refused,
        DeviceError::Protocol(e) | DeviceError::Io(e) | DeviceError::Unsupported(e) => {
            DisconnectReason::Io(e.clone())
        }
    }
}

fn now_ms() -> Option<f64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs_f64() * 1000.0)
}

impl HealthTracker {
    /// Se abrió una sesión. Si ya había habido otra antes, cuenta como reconexión.
    pub fn connected(&mut self) {
        if self.had_connection {
            self.health.reconnects = self.health.reconnects.saturating_add(1);
        }
        self.had_connection = true;
        self.health.connected = true;
        self.heartbeat_sent = None;
    }

    /// Se cerró la sesión a propósito (al restaurar): no es un corte.
    pub fn closed(&mut self) {
        self.health.connected = false;
        self.heartbeat_sent = None;
        self.health.rtt_ms = None;
    }

    /// Se cortó una sesión abierta.
    pub fn disconnected(&mut self, reason: DisconnectReason, now: Instant) {
        self.health.connected = false;
        self.heartbeat_sent = None;
        self.health.rtt_ms = None;
        self.cuts.push_back(now);
        self.record(reason);
    }

    /// Un intento de conectar falló. No es un corte (no había sesión), pero el motivo del
    /// último fallo importa si es 914: la UI ofrece renovar la clave.
    pub fn attempt_failed(&mut self, error: &DeviceError) {
        if matches!(error, DeviceError::KeyRejected) {
            self.record(DisconnectReason::KeyRejected);
        }
    }

    fn record(&mut self, reason: DisconnectReason) {
        self.health.last_disconnect = Some(reason);
        self.health.last_disconnect_at_ms = now_ms();
    }

    /// Motivo del último corte.
    pub fn last_reason(&self) -> Option<DisconnectReason> {
        self.health.last_disconnect.clone()
    }

    /// Salió una señal de vida.
    pub fn heartbeat_sent(&mut self, now: Instant) {
        self.heartbeat_sent = Some(now);
    }

    /// Llegó la respuesta a una señal de vida: mide la latencia.
    pub fn heartbeat_reply(&mut self, now: Instant) {
        if let Some(sent) = self.heartbeat_sent.take() {
            self.health.rtt_ms = Some(now.saturating_duration_since(sent).as_secs_f32() * 1000.0);
        }
    }

    /// Estado actual. `counters` aporta los bytes y `now` la ventana de la última hora.
    pub fn snapshot(&mut self, now: Instant, counters: &Counters) -> LinkHealth {
        while self
            .cuts
            .front()
            .is_some_and(|t| now.saturating_duration_since(*t) >= HOUR)
        {
            self.cuts.pop_front();
        }
        let mut health = self.health.clone();
        health.disconnects_last_hour = u32::try_from(self.cuts.len()).unwrap_or(u32::MAX);
        health.bytes_rx = Counters::get(&counters.rx);
        health.bytes_tx = Counters::get(&counters.tx);
        health
    }
}

/// Agrega los bytes actuales a una salud ya calculada.
pub(crate) fn with_bytes(mut health: LinkHealth, counters: &Counters) -> LinkHealth {
    health.bytes_rx = Counters::get(&counters.rx);
    health.bytes_tx = Counters::get(&counters.tx);
    health
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn la_primera_conexion_no_es_una_reconexion() {
        let mut h = HealthTracker::default();
        let c = Counters::default();
        h.connected();
        assert_eq!(h.snapshot(Instant::now(), &c).reconnects, 0);
        h.disconnected(DisconnectReason::PeerClosed, Instant::now());
        h.connected();
        let s = h.snapshot(Instant::now(), &c);
        assert_eq!(s.reconnects, 1);
        assert!(s.connected);
        assert_eq!(s.last_disconnect, Some(DisconnectReason::PeerClosed));
        assert!(s.last_disconnect_at_ms.is_some());
    }

    // port: test_ambilight.py::FocoConexion::test_un_intento_fallido_no_cuenta_como_reconexion
    #[tokio::test(start_paused = true)]
    async fn un_intento_fallido_no_cuenta_como_reconexion() {
        let mut h = HealthTracker::default();
        let c = Counters::default();
        h.connected();
        h.disconnected(DisconnectReason::Timeout, Instant::now());
        h.attempt_failed(&DeviceError::Timeout);
        let s = h.snapshot(Instant::now(), &c);
        assert_eq!(s.reconnects, 0);
        assert!(!s.connected);
        assert_eq!(
            s.disconnects_last_hour, 1,
            "los intentos fallidos no son cortes"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn un_914_al_reintentar_queda_como_motivo() {
        let mut h = HealthTracker::default();
        let c = Counters::default();
        h.connected();
        h.disconnected(DisconnectReason::PeerClosed, Instant::now());
        h.attempt_failed(&DeviceError::KeyRejected);
        assert_eq!(
            h.snapshot(Instant::now(), &c).last_disconnect,
            Some(DisconnectReason::KeyRejected)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn los_cortes_de_mas_de_una_hora_no_cuentan() {
        let mut h = HealthTracker::default();
        let c = Counters::default();
        for _ in 0..3 {
            h.connected();
            h.disconnected(DisconnectReason::PeerClosed, Instant::now());
            tokio::time::advance(Duration::from_secs(1200)).await;
        }
        // Cortes a los 0, 20 y 40 minutos; ahora son los 60.
        assert_eq!(h.snapshot(Instant::now(), &c).disconnects_last_hour, 2);
        tokio::time::advance(Duration::from_secs(1800)).await;
        assert_eq!(h.snapshot(Instant::now(), &c).disconnects_last_hour, 1);
        tokio::time::advance(Duration::from_secs(1200)).await;
        assert_eq!(h.snapshot(Instant::now(), &c).disconnects_last_hour, 0);
    }

    #[tokio::test(start_paused = true)]
    async fn mide_la_latencia_del_latido() {
        let mut h = HealthTracker::default();
        let c = Counters::default();
        h.connected();
        h.heartbeat_sent(Instant::now());
        tokio::time::advance(Duration::from_millis(40)).await;
        h.heartbeat_reply(Instant::now());
        let rtt = h.snapshot(Instant::now(), &c).rtt_ms.unwrap();
        assert!((rtt - 40.0).abs() < 1.0, "rtt = {rtt}");
        // Una respuesta sin latido pendiente no cambia nada.
        h.heartbeat_reply(Instant::now());
        assert_eq!(h.snapshot(Instant::now(), &c).rtt_ms, Some(rtt));
    }

    #[tokio::test(start_paused = true)]
    async fn los_bytes_saturan_en_u32() {
        let h = HealthTracker::default();
        let c = Counters::default();
        c.add_rx(5000);
        c.add_tx(7);
        let mut h = h;
        let s = h.snapshot(Instant::now(), &c);
        assert_eq!((s.bytes_rx, s.bytes_tx), (5000, 7));
        c.add_rx(usize::MAX / 2);
        assert_eq!(with_bytes(LinkHealth::default(), &c).bytes_rx, u32::MAX);
    }

    #[test]
    fn cada_error_tiene_su_motivo() {
        assert_eq!(
            reason_of(&DeviceError::KeyRejected),
            DisconnectReason::KeyRejected
        );
        assert_eq!(reason_of(&DeviceError::Timeout), DisconnectReason::Timeout);
        assert_eq!(
            reason_of(&DeviceError::Disconnected(DisconnectReason::NoResponse)),
            DisconnectReason::NoResponse
        );
        assert_eq!(
            reason_of(&DeviceError::Io("x".into())),
            DisconnectReason::Io("x".into())
        );
    }
}
