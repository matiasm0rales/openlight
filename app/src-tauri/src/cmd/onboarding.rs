//! Progreso del asistente inicial: se guarda para poder pausar y retomar.

use std::fs;
use std::io::Write;

use openlight_types::ipc::{OnboardingState, OnboardingStep};
use secrets::SecretKey;
use serde::{Deserialize, Serialize};

use crate::errors::{CmdResult, internal};
use crate::state::Backend;

const FILE: &str = "onboarding.json";
const VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
struct OnboardingFile {
    version: u32,
    step: OnboardingStep,
    #[serde(default)]
    completed: Vec<OnboardingStep>,
}

impl Backend {
    /// Paso actual del asistente, pasos hechos, si hay credenciales y los focos configurados.
    pub fn onboarding_state(&self) -> CmdResult<OnboardingState> {
        let saved = self.read_onboarding();
        let has_credentials = self.secrets.get(&SecretKey::CloudAccessId)?.is_some()
            && self.secrets.get(&SecretKey::CloudAccessSecret)?.is_some();
        Ok(OnboardingState {
            step: saved.as_ref().map_or(OnboardingStep::Welcome, |f| f.step),
            completed: saved.map(|f| f.completed).unwrap_or_default(),
            has_credentials,
            devices: self.records()?.into_iter().map(|r| r.id).collect(),
        })
    }

    /// Guarda el paso actual y los completados.
    pub fn onboarding_save(
        &self,
        step: OnboardingStep,
        completed: Vec<OnboardingStep>,
    ) -> CmdResult<OnboardingState> {
        let file = OnboardingFile {
            version: VERSION,
            step,
            completed,
        };
        let path = self.store.dir().join(FILE);
        let tmp = path.with_extension("json.tmp");
        let text = serde_json::to_string_pretty(&file).map_err(|e| internal(e.to_string()))?;
        let write = || -> std::io::Result<()> {
            let mut out = fs::File::create(&tmp)?;
            out.write_all(text.as_bytes())?;
            out.sync_all()?;
            fs::rename(&tmp, &path)
        };
        write().map_err(|e| {
            let _ = fs::remove_file(&tmp);
            openlight_core::CoreError::Io {
                path: path.clone(),
                detail: e.to_string(),
            }
        })?;
        self.onboarding_state()
    }

    /// Lo guardado. Un archivo ilegible o de una versión nueva se ignora: el asistente
    /// arranca de cero, que es lo peor que puede pasar.
    fn read_onboarding(&self) -> Option<OnboardingFile> {
        let text = fs::read_to_string(self.store.dir().join(FILE)).ok()?;
        serde_json::from_str::<OnboardingFile>(&text)
            .ok()
            .filter(|f| f.version <= VERSION)
    }
}
