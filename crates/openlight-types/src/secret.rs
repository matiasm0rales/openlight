//! Envoltorio para valores sensibles (clave local del foco, Access Secret de Tuya).

use std::fmt;

use serde::{Deserialize, Deserializer};

/// Un valor que nunca debe aparecer en logs, en `Debug` ni en exportaciones de diagnóstico.
///
/// - `Debug` imprime `Secret([REDACTED])`.
/// - **No** implementa `Serialize` ni `specta::Type` a propósito: un secreto no puede terminar
///   en un JSON de configuración ni cruzar el IPC hacia la UI por accidente. Para guardarlo se
///   usa el crate `secrets` (llavero del sistema), que llama a [`Secret::expose`].
/// - Sí implementa `Deserialize`, para importar el setup legado (`devices.json`).
#[derive(Clone, PartialEq, Eq)]
pub struct Secret<T>(T);

impl<T> Secret<T> {
    /// Envuelve un valor sensible.
    pub fn new(value: T) -> Self {
        Self(value)
    }

    /// Acceso explícito al valor. Cada llamada debería ser fácil de auditar.
    pub fn expose(&self) -> &T {
        &self.0
    }

    /// Consume el envoltorio y devuelve el valor.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> From<T> for Secret<T> {
    fn from(value: T) -> Self {
        Self(value)
    }
}

impl<T> fmt::Debug for Secret<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret([REDACTED])")
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Secret<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize(deserializer).map(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_no_muestra_el_valor() {
        let s = Secret::new("0123456789abcdef".to_string());
        assert_eq!(format!("{s:?}"), "Secret([REDACTED])");
        assert_eq!(s.expose(), "0123456789abcdef");
    }

    #[test]
    fn se_deserializa_transparente() {
        let s: Secret<String> = serde_json::from_str("\"k\"").unwrap();
        assert_eq!(s.into_inner(), "k");
    }
}
