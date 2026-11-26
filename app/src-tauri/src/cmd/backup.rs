//! Respaldo sin claves: exportar lo configurado, importarlo en otra compu y bajar de la nube las
//! claves que quedan faltando. El formato y sus reglas están en `openlight_core::backup`.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use openlight_core::CoreError;
use openlight_core::backup::{self, Destination};
use openlight_types::ipc::{BackupSummary, DuplicatePolicy, ImportReport, MissingKeysReport};
use openlight_types::{DeviceId, DisplayId};
use secrets::SecretKey;

use crate::cmd::cloud::cloud_error;
use crate::cmd::diagnostics::{now_secs, utc_stamp};
use crate::cmd::session::close_handle;
use crate::errors::CmdResult;
use crate::state::{ActiveSession, Backend, factory_profiles, resolved_capabilities};

/// El nombre que se propone al guardar: `openlight-backup-2026-11-24.json`.
pub(crate) fn file_name() -> String {
    file_name_at(now_secs())
}

fn file_name_at(secs: u64) -> String {
    let stamp = utc_stamp(secs);
    let date = stamp
        .split_once('T')
        .map_or(stamp.as_str(), |(date, _)| date);
    format!("openlight-backup-{date}.json")
}

impl Backend {
    /// El texto del respaldo. No toca el llavero: nada de lo que hay ahí va al archivo.
    pub fn backup_export(&self) -> CmdResult<String> {
        // Una foto coherente: sin cambios a medias mientras se junta
        let _edit = self.edit_lock();
        let excluded: Vec<DeviceId> = self
            .records()?
            .into_iter()
            .map(|r| r.id)
            .filter(|id| self.is_excluded(id))
            .collect();
        let backup = backup::collect(
            &self.store,
            &excluded,
            &resolved_capabilities,
            utc_stamp(now_secs()),
            env!("CARGO_PKG_VERSION").to_owned(),
        )?;
        Ok(backup::render(&backup)?)
    }

    /// Escribe el respaldo en `path`.
    pub fn backup_save(&self, path: &Path) -> CmdResult<()> {
        let text = self.backup_export()?;
        std::fs::write(path, text).map_err(|e| CoreError::Io {
            path: path.to_owned(),
            detail: e.to_string(),
        })?;
        // La ruta no va al log: suele incluir el nombre de usuario
        tracing::info!("respaldo exportado");
        Ok(())
    }

    /// Qué trae un respaldo y qué focos ya están acá. No cambia nada.
    pub fn backup_inspect(&self, content: &str) -> CmdResult<BackupSummary> {
        let backup = backup::parse(content)?;
        Ok(backup::summarize(&backup, &self.records()?))
    }

    /// Importa un respaldo. Los focos nuevos quedan sin clave (ver
    /// [`Backend::fetch_missing_keys`]). Con el efecto andando, los que se pisaron se vuelven a
    /// tomar y los que no tienen clave esperan a tenerla.
    pub async fn backup_import(
        self: &Arc<Self>,
        content: &str,
        duplicates: DuplicatePolicy,
    ) -> CmdResult<ImportReport> {
        let backup = backup::parse(content)?;
        let found = (self.sources)().displays().unwrap_or_default();
        let fallback = capture::common::pick_display(None, &found);
        let displays: Vec<DisplayId> = found.into_iter().map(|d| d.id).collect();

        // La sesión queda tomada hasta anotar todo: no puede ver un import a medias
        let mut session = self.session.lock().await;
        let applied = {
            let _edit = self.edit_lock();
            backup::apply(
                backup,
                &self.store,
                duplicates,
                &Destination {
                    factory_profiles: &factory_profiles(),
                    displays: &displays,
                    fallback_display: fallback.as_ref(),
                },
            )?
        };
        for id in &applied.report.devices {
            self.save_excluded(id, applied.excluded.contains(id))?;
        }
        for id in &applied.replaced {
            // La conexión manual puede tener otra versión del protocolo
            self.release_manual(id);
        }
        if let Some(active) = session.as_mut() {
            self.reload_in_session(active, &applied.report.devices)
                .await?;
        }
        drop(session);

        self.effect_changed();
        self.config_changed();
        Ok(applied.report)
    }

    /// Saca de la sesión los focos importados que ya estaban en ella (la próxima puesta al día
    /// los vuelve a tomar con su calibración y su versión nuevas) y deja esperando a los que no
    /// tienen clave, para no avisar un error por cada uno.
    async fn reload_in_session(
        &self,
        active: &mut ActiveSession,
        imported: &[DeviceId],
    ) -> CmdResult<()> {
        let pending: HashSet<DeviceId> = self.devices_without_key()?.into_iter().collect();
        let current = active.session.devices();
        for id in imported {
            if pending.contains(id) {
                active.skipped.insert(id.clone());
            } else {
                active.skipped.remove(id);
            }
            if !current.contains(id) {
                continue;
            }
            active.session.remove_device(id).await;
            if let Some(index) = active.handles.iter().position(|h| h.id() == id) {
                close_handle(
                    active.handles.swap_remove(index),
                    self.options.restore_timeout,
                )
                .await;
            }
        }
        Ok(())
    }

    /// Los focos configurados que no tienen su clave en el llavero.
    pub fn devices_without_key(&self) -> CmdResult<Vec<DeviceId>> {
        let mut missing = Vec::new();
        for record in self.records()? {
            let key = SecretKey::DeviceLocalKey(record.id.clone());
            if self.secrets.get(&key)?.is_none() {
                missing.push(record.id);
            }
        }
        Ok(missing)
    }

    /// Baja de la nube, con las credenciales guardadas, las claves que faltan. Sin credenciales
    /// es un error y los focos siguen pendientes.
    pub async fn fetch_missing_keys(self: &Arc<Self>) -> CmdResult<MissingKeysReport> {
        let pending = self.devices_without_key()?;
        let mut report = MissingKeysReport::default();
        if pending.is_empty() {
            return Ok(report);
        }
        let client = self.cloud_client()?;
        let account = client.devices().await.map_err(|e| cloud_error(&e))?;
        for id in pending {
            let Some(device) = account.iter().find(|d| d.id == id) else {
                report.not_found.push(id);
                continue;
            };
            let mapping = if device.is_light() {
                match client.mapping(&id).await {
                    Ok(mapping) => Some(mapping),
                    Err(error) => {
                        tracing::warn!(device = %id, %error, "no se pudo leer el mapeo");
                        None
                    }
                }
            } else {
                None
            };
            if self.store_cloud_key(&id, &device.local_key, mapping)? {
                self.forget_skipped(&id).await;
                report.fetched.push(id);
            }
        }
        self.config_changed();
        self.notify_changed();
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_nombre_propuesto_lleva_la_fecha_y_es_json() {
        assert_eq!(
            file_name_at(1_795_545_005),
            "openlight-backup-2026-11-24.json"
        );
    }
}
