//! Diagnóstico: el visor del log y (más abajo) el zip para pedir ayuda.

use openlight_types::ipc::LogLine;

use crate::logging::{LogHandle, RING_CAPACITY};
use crate::state::Backend;

/// Cuántas líneas devuelve `diagnostics_log` si la UI no pide una cantidad.
const DEFAULT_LINES: usize = 500;

impl Backend {
    /// Conecta el log de la app (el anillo y la carpeta de archivos).
    pub fn set_logs(&self, logs: LogHandle) {
        *self
            .logs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(logs);
    }

    pub(crate) fn logs(&self) -> Option<LogHandle> {
        self.logs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Las últimas `limit` líneas del log (500 si no se pide una cantidad), las más nuevas al
    /// final. Sin log instalado (pruebas) no hay líneas.
    pub fn diagnostics_log(&self, limit: Option<u32>) -> Vec<LogLine> {
        let limit = limit
            .map_or(DEFAULT_LINES, |n| n as usize)
            .min(RING_CAPACITY);
        self.logs()
            .map(|logs| logs.ring.last(limit))
            .unwrap_or_default()
    }
}
