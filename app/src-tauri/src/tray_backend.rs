//! El [`TrayBackend`] real: el menú del tray sobre el estado de la app.

use std::sync::Arc;

use openlight_types::{DeviceId, ProfileId};

use crate::state::Backend;
use crate::tray::{BoxFuture, TrayBackend, TrayDevice, TrayProfile, TraySnapshot};

/// El tray de la app, sobre el [`Backend`].
pub struct AppTray {
    backend: Arc<Backend>,
}

impl AppTray {
    /// Un tray sobre `backend`.
    pub fn new(backend: Arc<Backend>) -> Self {
        Self { backend }
    }

    /// Corre una acción en segundo plano; si falla, el error llega a la UI por `error://raised`.
    fn spawn<F>(&self, task: impl FnOnce(Arc<Backend>) -> F + Send + 'static)
    where
        F: std::future::Future<Output = crate::errors::CmdResult<()>> + Send + 'static,
    {
        let backend = self.backend.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(error) = task(backend.clone()).await {
                backend.raise(error);
            }
        });
    }
}

impl TrayBackend for AppTray {
    fn snapshot(&self) -> TraySnapshot {
        let backend = &self.backend;
        let config = backend.config().unwrap_or_default();
        let devices = backend.records().unwrap_or_default();
        TraySnapshot {
            session: backend.session_state(),
            profiles: config
                .profiles
                .into_iter()
                .map(|p| TrayProfile {
                    id: p.id,
                    name: p.name,
                })
                .collect(),
            active_profile: config.active_profile,
            devices: devices
                .into_iter()
                .map(|d| TrayDevice {
                    enabled: !backend.is_excluded(&d.id),
                    id: d.id,
                    name: d.name,
                })
                .collect(),
        }
    }

    fn start(&self) {
        self.spawn(|b| async move { b.session_start().await.map(drop) });
    }

    fn stop(&self) {
        self.spawn(|b| async move { b.session_stop().await.map(drop) });
    }

    fn select_profile(&self, profile: Option<ProfileId>) {
        if let Err(error) = self.backend.profiles_activate(profile) {
            self.backend.raise(error);
        }
    }

    fn toggle_device(&self, device: &DeviceId) {
        let device = device.clone();
        self.spawn(|b| async move { b.toggle_device_in_effect(device).await });
    }

    fn restore_and_quit(&self) -> BoxFuture<()> {
        // Ya, antes de que el futuro corra: una actualización en preparación tiene que enterarse
        self.backend.begin_shutdown();
        let backend = self.backend.clone();
        Box::pin(async move { backend.shutdown().await })
    }
}
