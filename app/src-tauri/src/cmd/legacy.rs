//! Importar el setup del script de Python (`devices.json`, `config.json`, `calibracion.json`).

use std::path::Path;

use openlight_types::ipc::ImportReport;

use crate::errors::CmdResult;
use crate::state::Backend;

impl Backend {
    /// Importa los focos, ajustes y calibración de `folder`. Las claves van al llavero y la
    /// carpeta original no se toca. Volver a importar es seguro: conserva zonas y ajustes.
    ///
    /// Los focos nuevos siguen todo el monitor por defecto (el primero externo), como el script.
    pub async fn import_legacy(&self, folder: String) -> CmdResult<ImportReport> {
        let default_display = self.default_zones().into_iter().next().map(|z| z.display);
        let report = openlight_core::import_legacy(
            Path::new(&folder),
            &self.store,
            &*self.secrets,
            default_display.as_ref(),
        )?;
        // Lo que había en memoria (claves, conexiones manuales) puede ser del setup anterior
        self.cache().cloud_devices.clear();
        for device in &report.devices {
            self.release_manual(device);
        }
        Ok(report)
    }
}
