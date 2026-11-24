//! Ajustes (globales, por foco y por perfil) y perfiles.

use openlight_types::ipc::{DeviceOverrides, SettingScope, SettingsSnapshot};
use openlight_types::{
    ErrorCode, FixAction, Profile, ProfileId, SettingId, SettingValue, Settings, UserFacingError,
    catalog,
};

use crate::errors::{CmdResult, invalid, unknown_device};
use crate::state::Backend;

impl Backend {
    /// Catálogo y valores actuales.
    pub fn settings_get(&self) -> CmdResult<SettingsSnapshot> {
        let config = self.config()?;
        let global = Settings::default().with_overrides(&config.global)?;
        Ok(SettingsSnapshot {
            catalog: catalog().to_vec(),
            effective: config.effective_settings(None)?,
            global,
            devices: self
                .records()?
                .into_iter()
                .map(|r| DeviceOverrides {
                    device: r.id,
                    overrides: r.overrides,
                })
                .collect(),
            active_profile: config.active_profile,
        })
    }

    /// Cambia un ajuste después de validarlo contra el catálogo.
    pub fn settings_set(
        &self,
        scope: SettingScope,
        id: SettingId,
        value: SettingValue,
    ) -> CmdResult<SettingsSnapshot> {
        id.def().validate(&value)?;
        self.edit_overrides(&scope, |values| {
            values.0.insert(id, value);
        })?;
        self.settings_get()
    }

    /// Vuelve un ajuste (o todos) a su valor por defecto en ese ámbito.
    pub fn settings_reset(
        &self,
        scope: SettingScope,
        id: Option<SettingId>,
    ) -> CmdResult<SettingsSnapshot> {
        self.edit_overrides(&scope, |values| match id {
            Some(id) => {
                values.0.remove(&id);
            }
            None => values.0.clear(),
        })?;
        self.settings_get()
    }

    /// Aplica `change` a los valores que pisa el ámbito y los guarda.
    fn edit_overrides(
        &self,
        scope: &SettingScope,
        change: impl FnOnce(&mut openlight_types::SettingsOverride),
    ) -> CmdResult<()> {
        let _edit = self.edit_lock();
        match scope {
            SettingScope::Global => {
                let mut config = self.config()?;
                change(&mut config.global);
                self.store.save_config(&config)?;
            }
            SettingScope::Profile(id) => {
                let mut config = self.config()?;
                let profile = config
                    .profiles
                    .iter_mut()
                    .find(|p| &p.id == id)
                    .ok_or_else(|| unknown_profile(id))?;
                change(&mut profile.values);
                self.store.save_config(&config)?;
            }
            SettingScope::Device(id) => {
                let mut records = self.records()?;
                let record = records
                    .iter_mut()
                    .find(|r| &r.id == id)
                    .ok_or_else(|| unknown_device(id))?;
                change(&mut record.overrides);
                self.store.save_devices(&records)?;
            }
        }
        Ok(())
    }

    // ---------- perfiles ----------

    /// Perfiles de fábrica y propios.
    pub fn profiles_list(&self) -> CmdResult<Vec<Profile>> {
        Ok(self.config()?.profiles)
    }

    /// Crea un perfil, vacío o copiando los valores de otro.
    pub fn profiles_create(&self, name: String, from: Option<ProfileId>) -> CmdResult<Profile> {
        let name = name.trim().to_owned();
        if name.is_empty() || name.chars().count() > 60 {
            return Err(invalid("profile_name", "el nombre va de 1 a 60 caracteres"));
        }
        let _edit = self.edit_lock();
        let mut config = self.config()?;
        let values = match &from {
            Some(id) => config
                .profiles
                .iter()
                .find(|p| &p.id == id)
                .map(|p| p.values.clone())
                .ok_or_else(|| unknown_profile(id))?,
            None => Default::default(),
        };
        let profile = Profile {
            id: ProfileId(unique_id(&name, &config.profiles)),
            name,
            values,
            shortcut: None,
            builtin: false,
        };
        config.profiles.push(profile.clone());
        self.store.save_config(&config)?;
        self.notify_changed();
        Ok(profile)
    }

    /// Guarda nombre, valores y atajo. El id y si es de fábrica no cambian; los de fábrica
    /// conservan su nombre (es una clave i18n).
    pub fn profiles_update(&self, profile: Profile) -> CmdResult<Profile> {
        profile.values.validate().map_err(UserFacingError::from)?;
        let _edit = self.edit_lock();
        let mut config = self.config()?;
        let stored = config
            .profiles
            .iter_mut()
            .find(|p| p.id == profile.id)
            .ok_or_else(|| unknown_profile(&profile.id))?;
        if !stored.builtin {
            let name = profile.name.trim();
            if name.is_empty() || name.chars().count() > 60 {
                return Err(invalid("profile_name", "el nombre va de 1 a 60 caracteres"));
            }
            stored.name = name.to_owned();
        }
        stored.values = profile.values;
        stored.shortcut = profile.shortcut.filter(|s| !s.trim().is_empty());
        let saved = stored.clone();
        self.store.save_config(&config)?;
        self.notify_changed();
        Ok(saved)
    }

    /// Borra un perfil propio (los de fábrica no se borran).
    pub fn profiles_delete(&self, id: ProfileId) -> CmdResult<()> {
        let _edit = self.edit_lock();
        let mut config = self.config()?;
        let profile = config
            .profiles
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| unknown_profile(&id))?;
        if profile.builtin {
            return Err(invalid("profile", "los perfiles de fábrica no se borran"));
        }
        config.profiles.retain(|p| p.id != id);
        if config.active_profile.as_ref() == Some(&id) {
            config.active_profile = None;
        }
        self.store.save_config(&config)?;
        self.notify_changed();
        Ok(())
    }

    /// Activa un perfil (`None` = solo los ajustes globales). Vale desde la próxima sesión.
    pub fn profiles_activate(&self, id: Option<ProfileId>) -> CmdResult<()> {
        let _edit = self.edit_lock();
        let mut config = self.config()?;
        if let Some(id) = &id
            && !config.profiles.iter().any(|p| &p.id == id)
        {
            return Err(unknown_profile(id));
        }
        config.active_profile = id;
        self.store.save_config(&config)?;
        self.notify_changed();
        Ok(())
    }
}

fn unknown_profile(id: &ProfileId) -> UserFacingError {
    UserFacingError::new(ErrorCode::InvalidSetting)
        .with_param("setting", "profile")
        .with_param("detail", format!("no existe el perfil {}", id.0))
        .with_action(FixAction::Retry)
}

/// Un id a partir del nombre (`Mi perfil` → `mi-perfil`), distinto de los que ya hay.
fn unique_id(name: &str, existing: &[Profile]) -> String {
    let mut slug: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    slug = slug
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        slug = "perfil".into();
    }
    let taken = |candidate: &str| existing.iter().any(|p| p.id.0 == candidate);
    if !taken(&slug) {
        return slug;
    }
    (2..)
        .map(|n| format!("{slug}-{n}"))
        .find(|candidate| !taken(candidate))
        .expect("siempre hay un sufijo libre")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(id: &str) -> Profile {
        Profile {
            id: ProfileId(id.into()),
            name: id.into(),
            values: Default::default(),
            shortcut: None,
            builtin: false,
        }
    }

    #[test]
    fn el_id_sale_del_nombre_y_no_se_repite() {
        assert_eq!(unique_id("Mi perfil", &[]), "mi-perfil");
        assert_eq!(unique_id("  ¡¡Cine!!  ", &[]), "cine");
        assert_eq!(unique_id("???", &[]), "perfil");
        let existing = [profile("mi-perfil"), profile("mi-perfil-2")];
        assert_eq!(unique_id("Mi perfil", &existing), "mi-perfil-3");
    }
}
