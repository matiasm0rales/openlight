//! Estados de la sesión y de cada foco.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::ErrorCode;

/// Estado global de la sesión:
/// `Idle → Connecting → Active ⇄ Paused (sin pantalla) → Restoring → Idle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    /// Sin efecto; los focos en su estado original.
    Idle,
    /// Conectando con los focos.
    Connecting,
    /// Los focos siguen la pantalla.
    Active,
    /// Sin pantalla durante `no_screen_wait_s`: focos devueltos a su estado.
    Paused,
    /// Devolviendo los focos a su estado antes de parar.
    Restoring,
}

/// Estado de un foco dentro de la sesión.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(tag = "kind", content = "code", rename_all = "snake_case")]
pub enum DeviceSessionState {
    /// Sin usar.
    Idle,
    /// Conectando por primera vez.
    Connecting,
    /// Recibiendo colores.
    Active,
    /// Se cortó y se está reconectando.
    Reconnecting,
    /// En pausa por falta de pantalla.
    Paused,
    /// Volviendo a su estado original.
    Restoring,
    /// Error que necesita acción del usuario.
    Error(ErrorCode),
}
