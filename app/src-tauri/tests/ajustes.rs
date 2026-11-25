//! Asistente, ajustes y perfiles contra el backend real (sin focos ni pantalla).
#![allow(missing_docs)]

mod common;

use common::{Setup, harness};
use openlight_types::ipc::{OnboardingStep, SettingScope};
use openlight_types::{ErrorCode, ProfileId, SettingId, SettingValue};

fn num(v: f64) -> SettingValue {
    SettingValue::Number(v)
}

#[tokio::test]
async fn el_asistente_arranca_en_la_bienvenida_y_recuerda_el_paso() {
    let h = harness(Setup {
        sims: 0,
        lan: false,
    })
    .await;
    let state = h.backend.onboarding_state().unwrap();
    assert_eq!(state.step, OnboardingStep::Welcome);
    assert!(state.completed.is_empty() && state.devices.is_empty() && !state.has_credentials);

    let saved = h
        .backend
        .onboarding_save(
            OnboardingStep::Credentials,
            vec![OnboardingStep::Welcome, OnboardingStep::Permissions],
        )
        .unwrap();
    assert_eq!(saved.step, OnboardingStep::Credentials);
    // Se lee del disco, no de memoria: sobrevive a reabrir la app
    let again = h.backend.onboarding_state().unwrap();
    assert_eq!(again, saved);
}

#[tokio::test]
async fn los_ajustes_se_validan_se_guardan_y_se_resetean() {
    let h = harness(Setup {
        sims: 0,
        lan: false,
    })
    .await;
    let b = &h.backend;
    let snapshot = b.settings_get().unwrap();
    assert_eq!(snapshot.catalog.len(), 17);
    assert_eq!(snapshot.active_profile, None);

    let after = b
        .settings_set(SettingScope::Global, SettingId::Saturation, num(1.8))
        .unwrap();
    assert!((after.effective.saturation - 1.8).abs() < 1e-6);
    assert!((after.global.saturation - 1.8).abs() < 1e-6);

    // Fuera de rango: el catálogo lo rechaza y no se guarda nada
    let error = b
        .settings_set(SettingScope::Global, SettingId::Saturation, num(9.0))
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidSetting);
    assert!((b.settings_get().unwrap().global.saturation - 1.8).abs() < 1e-6);

    let reset = b
        .settings_reset(SettingScope::Global, Some(SettingId::Saturation))
        .unwrap();
    assert!(
        (reset.global.saturation - 1.3).abs() < 1e-6,
        "vuelve al defecto del catálogo"
    );
}

#[tokio::test]
async fn un_perfil_activo_pisa_los_globales_y_los_de_fabrica_no_se_borran() {
    let h = harness(Setup {
        sims: 0,
        lan: false,
    })
    .await;
    let b = &h.backend;
    let profiles = b.profiles_list().unwrap();
    let ids: Vec<_> = profiles.iter().map(|p| p.id.0.as_str()).collect();
    assert_eq!(ids, ["movies", "games", "work"]);
    assert!(profiles.iter().all(|p| p.builtin));

    b.profiles_activate(Some(ProfileId("movies".into())))
        .unwrap();
    let snapshot = b.settings_get().unwrap();
    assert_eq!(snapshot.active_profile, Some(ProfileId("movies".into())));
    assert!(
        (snapshot.effective.saturation - 1.4).abs() < 1e-6,
        "el perfil manda"
    );
    assert!(
        (snapshot.global.saturation - 1.3).abs() < 1e-6,
        "los globales no cambian"
    );

    let error = b.profiles_delete(ProfileId("movies".into())).unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidSetting);

    // Un perfil propio, copiado de otro, se edita y se borra; si estaba activo se desactiva
    let mine = b
        .profiles_create("Mi perfil".into(), Some(ProfileId("games".into())))
        .unwrap();
    assert_eq!(mine.id.0, "mi-perfil");
    assert!(!mine.builtin);
    assert_eq!(mine.values.0.len(), 3, "copia los valores de Juegos");
    b.profiles_activate(Some(mine.id.clone())).unwrap();
    b.settings_set(
        SettingScope::Profile(mine.id.clone()),
        SettingId::SmoothingMs,
        num(50.0),
    )
    .unwrap();
    assert!((b.settings_get().unwrap().effective.smoothing_ms - 50.0).abs() < 1e-6);
    b.profiles_delete(mine.id).unwrap();
    assert_eq!(b.settings_get().unwrap().active_profile, None);
}

#[tokio::test]
async fn cambios_de_ajustes_al_mismo_tiempo_no_se_pisan() {
    // Un slider manda cambios seguidos: cada uno lee, modifica y escribe la configuración entera
    let h = harness(Setup {
        sims: 0,
        lan: false,
    })
    .await;
    let ids = [
        (SettingId::Saturation, 1.1),
        (SettingId::BrightnessMin, 0.1),
        (SettingId::BrightnessMax, 0.9),
        (SettingId::SmoothingMs, 100.0),
        (SettingId::CaptureFps, 30.0),
        (SettingId::MaxSendsPerS, 5.0),
        (SettingId::HueStickiness, 2.0),
        (SettingId::DarkScreen, 0.2),
    ];
    for _ in 0..10 {
        let threads: Vec<_> = ids
            .iter()
            .map(|&(id, value)| {
                let backend = h.backend.clone();
                std::thread::spawn(move || {
                    backend
                        .settings_set(SettingScope::Global, id, num(value))
                        .unwrap();
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        let global = h.backend.settings_get().unwrap().global;
        for (id, value) in ids {
            assert!(
                (global.get(id).as_number().unwrap() - value).abs() < 1e-6,
                "{id:?} se perdió"
            );
        }
        for (id, _) in ids {
            h.backend
                .settings_reset(SettingScope::Global, Some(id))
                .unwrap();
        }
    }
}

#[tokio::test]
async fn el_brillo_maximo_no_puede_quedar_por_debajo_del_minimo() {
    let h = harness(Setup {
        sims: 0,
        lan: false,
    })
    .await;
    let b = &h.backend;
    b.settings_set(SettingScope::Global, SettingId::BrightnessMin, num(0.4))
        .unwrap();
    for (id, value) in [
        (SettingId::BrightnessMax, 0.3),
        (SettingId::BrightnessMin, 1.0),
    ] {
        if id == SettingId::BrightnessMin {
            b.settings_set(SettingScope::Global, SettingId::BrightnessMax, num(0.9))
                .unwrap();
        }
        let error = b
            .settings_set(SettingScope::Global, id, num(value))
            .unwrap_err();
        assert_eq!(error.title, "error.brightness_range.title", "{id}");
    }
    let global = b.settings_get().unwrap().global;
    assert!((global.brightness_min - 0.4).abs() < 1e-6, "no se guardó");
    assert!((global.brightness_max - 0.9).abs() < 1e-6, "no se guardó");

    // En un perfil, contra lo que hereda de los globales
    let work = SettingScope::Profile(ProfileId("work".into()));
    let error = b
        .settings_set(work.clone(), SettingId::BrightnessMax, num(0.2))
        .unwrap_err();
    assert_eq!(error.title, "error.brightness_range.title");
    let mut profile = b
        .profiles_list()
        .unwrap()
        .into_iter()
        .find(|p| p.id.0 == "work")
        .unwrap();
    profile.values.0.insert(SettingId::BrightnessMax, num(0.2));
    let error = b.profiles_update(profile).unwrap_err();
    assert_eq!(error.title, "error.brightness_range.title");

    // Cero no es un brillo: para apagar el foco hay otra cosa
    let error = b
        .settings_set(SettingScope::Global, SettingId::BrightnessMax, num(0.0))
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidSetting);
}
