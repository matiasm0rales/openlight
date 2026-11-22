//! Perfiles: conjuntos de valores de ajustes que se cambian desde el tray.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::SettingsOverride;

/// Identificador de perfil (`movies`, `games`, `work` o uno generado para los propios).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Type)]
#[serde(transparent)]
pub struct ProfileId(pub String);

/// Un perfil. Los de fábrica (`builtin`) se pueden duplicar pero no borrar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Profile {
    /// Identificador.
    pub id: ProfileId,
    /// Nombre (para los de fábrica, una clave i18n: `profile.movies`).
    pub name: String,
    /// Valores que pisa sobre los ajustes globales.
    pub values: SettingsOverride,
    /// Atajo de teclado global (`CmdOrCtrl+Alt+1`).
    pub shortcut: Option<String>,
    /// Si es de fábrica.
    pub builtin: bool,
}
