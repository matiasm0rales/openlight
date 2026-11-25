//! El backend frente a la shell de la app: el tray (menú y acciones), la ventana de
//! calibración y los focos que el usuario saca del efecto.
#![allow(missing_docs)]

mod common;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::{
    DISPLAY, FAKE_ACCESS_ID, FAKE_ACCESS_SECRET, Harness, Setup, eventually, harness, sim_id,
};
use openlight_lib::shell::Shell;
use openlight_lib::tray::TrayBackend;
use openlight_lib::tray_backend::AppTray;
use openlight_types::ipc::{CloudCredentialsInput, DeviceAddRequest};
use openlight_types::{DisplayId, ProfileId, Region, SessionState};
use serde_json::json;

#[derive(Default)]
struct FakeShell {
    log: Mutex<Vec<String>>,
    /// Como en Windows cuando el instalador no llega a abrirse: sin tray ni ventana.
    dismantled: AtomicBool,
}

impl FakeShell {
    fn log(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }

    fn count(&self, what: &str) -> usize {
        self.log().iter().filter(|l| l.as_str() == what).count()
    }
}

impl Shell for FakeShell {
    fn changed(&self) {
        self.log.lock().unwrap().push("changed".into());
    }

    fn open_calibration(&self, display: &DisplayId) -> Result<(), String> {
        self.log.lock().unwrap().push(format!("open:{display}"));
        Ok(())
    }

    fn close_calibration(&self) {
        self.log.lock().unwrap().push("close".into());
    }

    fn is_dismantled(&self) -> bool {
        self.dismantled.load(Ordering::SeqCst)
    }

    fn restart(&self) {
        self.log.lock().unwrap().push("restart".into());
    }
}

async fn ready(h: &Harness, devices: usize) -> Arc<FakeShell> {
    let shell = Arc::new(FakeShell::default());
    h.backend.set_shell(shell.clone());
    h.backend
        .cloud_validate(CloudCredentialsInput {
            access_id: FAKE_ACCESS_ID.into(),
            access_secret: FAKE_ACCESS_SECRET.into(),
            region: Region::Us,
        })
        .await
        .unwrap();
    for n in 1..=devices {
        h.backend
            .device_add(DeviceAddRequest {
                id: sim_id(n),
                name: format!("Foco {n}"),
                ip: Some("127.0.0.1".into()),
            })
            .await
            .unwrap();
    }
    shell
}

#[tokio::test]
async fn la_calibracion_abre_y_cierra_su_ventana() {
    let h = harness(Setup::default()).await;
    let shell = ready(&h, 1).await;
    let display = DisplayId(DISPLAY.into());

    h.backend
        .calibration_start(sim_id(1), display.clone(), true)
        .await
        .unwrap();
    assert_eq!(shell.count(&format!("open:{DISPLAY}")), 1);
    h.backend.calibration_save().await.unwrap();
    assert_eq!(shell.count("close"), 1);

    h.backend
        .calibration_start(sim_id(1), display, true)
        .await
        .unwrap();
    h.backend.calibration_cancel().await.unwrap();
    assert_eq!(shell.count("close"), 2);
    // Cancelar sin nada abierto no toca ninguna ventana
    h.backend.calibration_cancel().await.unwrap();
    assert_eq!(shell.count("close"), 2);
}

#[tokio::test]
async fn el_tray_ve_perfiles_focos_y_estado_y_se_entera_de_cada_cambio() {
    let h = harness(Setup { sims: 2, lan: true }).await;
    let shell = ready(&h, 2).await;
    let tray = AppTray::new(h.backend.clone());

    let snapshot = tray.snapshot();
    assert_eq!(snapshot.session, SessionState::Idle);
    assert_eq!(snapshot.profiles.len(), 3);
    assert_eq!(snapshot.devices.len(), 2);
    assert!(snapshot.devices.iter().all(|d| d.enabled));

    let before = shell.count("changed");
    tray.select_profile(Some(ProfileId("games".into())));
    assert_eq!(
        tray.snapshot().active_profile,
        Some(ProfileId("games".into()))
    );
    assert!(
        shell.count("changed") > before,
        "el perfil avisa a la shell"
    );

    // Arrancar y parar desde el tray, y el menú se entera de los cambios de la sesión
    h.screen.paint([255, 0, 0]);
    tray.start();
    eventually("sesión activa", Duration::from_secs(8), || {
        tray.snapshot().session == SessionState::Active
    })
    .await;
    assert!(shell.count("changed") > before + 1);
    tray.stop();
    eventually("sesión detenida", Duration::from_secs(8), || {
        tray.snapshot().session == SessionState::Idle
    })
    .await;
}

#[tokio::test]
async fn sacar_un_foco_del_efecto_lo_deja_fuera_y_volver_a_incluirlo_lo_suma() {
    let h = harness(Setup { sims: 2, lan: true }).await;
    ready(&h, 2).await;
    let tray = AppTray::new(h.backend.clone());
    h.screen.paint([255, 0, 0]);

    // Con la sesión parada, el foco excluido no participa al arrancar
    h.backend.toggle_device_in_effect(sim_id(2)).await.unwrap();
    assert!(
        !tray
            .snapshot()
            .devices
            .iter()
            .find(|d| d.id == sim_id(2))
            .unwrap()
            .enabled
    );
    h.backend.session_start().await.unwrap();
    eventually("foco 1 activo", Duration::from_secs(8), || {
        h.sims[0].dp("21") == Some(json!("colour"))
    })
    .await;
    assert_eq!(h.sims[1].active_connections(), 0, "el foco 2 no se tocó");

    // Con la sesión en marcha, volver a prenderlo lo suma sin reiniciar
    h.backend.toggle_device_in_effect(sim_id(2)).await.unwrap();
    eventually("foco 2 activo", Duration::from_secs(8), || {
        h.sims[1].dp("21") == Some(json!("colour"))
    })
    .await;
    // y apagar el último que queda deja la sesión parada, sin error
    h.backend.toggle_device_in_effect(sim_id(1)).await.unwrap();
    h.backend.toggle_device_in_effect(sim_id(2)).await.unwrap();
    assert_eq!(h.backend.session_state(), SessionState::Idle);
    eventually("focos desconectados", Duration::from_secs(3), || {
        h.sims.iter().all(|s| s.active_connections() == 0)
    })
    .await;
}

#[tokio::test]
async fn los_focos_excluidos_se_recuerdan_al_reabrir_la_app() {
    let h = harness(Setup::default()).await;
    ready(&h, 1).await;
    h.backend.toggle_device_in_effect(sim_id(1)).await.unwrap();
    assert!(h.backend.is_excluded(&sim_id(1)));
    let reopened = common::reopen(&h);
    assert!(reopened.is_excluded(&sim_id(1)));
}

#[tokio::test]
async fn salir_restaura_una_sola_vez_y_marca_el_cierre() {
    let h = harness(Setup::default()).await;
    ready(&h, 1).await;
    h.screen.paint([0, 255, 0]);
    h.backend.session_start().await.unwrap();
    eventually("conectado", Duration::from_secs(8), || {
        h.sims[0].dp("21") == Some(json!("colour"))
    })
    .await;
    let original = h.sims[0].dps();
    assert!(!h.backend.is_shut_down());
    assert!(h.backend.begin_shutdown());
    assert!(!h.backend.begin_shutdown(), "solo uno corre el cierre");
    h.backend.shutdown().await;
    assert!(h.backend.is_shut_down());
    assert_eq!(h.sims[0].dp("21"), original.get("21").cloned());
    assert_eq!(h.backend.session_state(), SessionState::Idle);
}

#[tokio::test]
async fn actualizar_restaura_los_focos_como_salir_y_no_deja_arrancar() {
    let h = harness(Setup::default()).await;
    ready(&h, 1).await;
    h.screen.paint([0, 255, 0]);
    let original = h.sims[0].dps();
    h.backend.session_start().await.unwrap();
    eventually("conectado", Duration::from_secs(8), || {
        h.sims[0].dp("21") == Some(json!("colour"))
    })
    .await;

    h.backend.update_prepare().await;

    assert!(h.backend.is_shut_down());
    assert_eq!(h.sims[0].dp("21"), original.get("21").cloned());
    assert_eq!(h.backend.session_state(), SessionState::Idle);
    // Mientras se instala, ni el panel ni el tray pueden volver a tomar los focos
    assert_eq!(h.backend.session_start().await.unwrap(), SessionState::Idle);
    assert_eq!(h.backend.session_state(), SessionState::Idle);
}

#[tokio::test]
async fn si_la_actualizacion_falla_la_app_vuelve_a_andar_y_salir_restaura() {
    let h = harness(Setup::default()).await;
    ready(&h, 1).await;
    h.screen.paint([0, 255, 0]);
    let original = h.sims[0].dps();
    h.backend.update_prepare().await;

    h.backend.update_abort();

    assert!(!h.backend.is_shut_down());
    h.backend.session_start().await.unwrap();
    eventually("conectado", Duration::from_secs(8), || {
        h.sims[0].dp("21") == Some(json!("colour"))
    })
    .await;
    assert!(h.backend.begin_shutdown(), "Salir vuelve a restaurar");
    h.backend.shutdown().await;
    assert_eq!(h.sims[0].dp("21"), original.get("21").cloned());
}

#[tokio::test]
async fn abortar_sin_haber_preparado_no_interrumpe_un_cierre() {
    let h = harness(Setup::default()).await;
    assert!(h.backend.begin_shutdown());
    h.backend.update_abort();
    assert!(
        !h.backend.begin_shutdown(),
        "el cierre en curso sigue marcado"
    );
}

#[tokio::test]
async fn despues_de_actualizar_se_muestra_la_ventana_una_sola_vez() {
    let h = harness(Setup::default()).await;
    assert!(!h.backend.take_reopen_after_update());
    h.backend.update_prepare().await;
    let reopened = common::reopen(&h);
    assert!(reopened.take_reopen_after_update());
    assert!(!reopened.take_reopen_after_update());
}

#[tokio::test]
async fn si_la_actualizacion_falla_no_queda_marcado_reabrir_la_ventana() {
    let h = harness(Setup::default()).await;
    h.backend.update_prepare().await;
    h.backend.update_abort();
    assert!(!h.backend.take_reopen_after_update());
}

#[tokio::test]
async fn si_el_instalador_no_se_abrio_y_la_app_quedo_desarmada_se_reinicia() {
    let h = harness(Setup::default()).await;
    let shell = Arc::new(FakeShell::default());
    shell.dismantled.store(true, Ordering::SeqCst);
    h.backend.set_shell(shell.clone());
    h.backend.update_prepare().await;

    h.backend.update_abort();

    assert_eq!(shell.count("restart"), 1);
    // El reinicio tiene que mostrar la ventana, que es lo que la persona estaba mirando.
    assert!(h.backend.take_reopen_after_update());
}

#[tokio::test]
async fn si_la_app_sigue_entera_una_actualizacion_fallida_no_reinicia() {
    let h = harness(Setup::default()).await;
    let shell = Arc::new(FakeShell::default());
    h.backend.set_shell(shell.clone());
    h.backend.update_prepare().await;

    h.backend.update_abort();

    assert_eq!(shell.count("restart"), 0);
    assert!(!h.backend.take_reopen_after_update());
}

#[tokio::test]
async fn salir_mientras_se_prepara_una_actualizacion_no_se_pierde() {
    let h = harness(Setup::default()).await;
    ready(&h, 1).await;
    h.screen.paint([0, 255, 0]);
    let original = h.sims[0].dps();
    h.backend.session_start().await.unwrap();
    eventually("conectado", Duration::from_secs(8), || {
        h.sims[0].dp("21") == Some(json!("colour"))
    })
    .await;
    let backend = h.backend.clone();
    let mut preparing = tokio::spawn(async move { backend.update_prepare().await });
    tokio::task::yield_now().await;
    assert!(
        !h.backend.is_shut_down(),
        "los focos todavía están volviendo"
    );

    // Cmd+Q: quien recibe la salida tiene que correr su cierre y cerrar la app
    assert!(h.backend.begin_shutdown());
    h.backend.shutdown().await;

    assert!(h.backend.is_shut_down());
    assert_eq!(h.sims[0].dp("21"), original.get("21").cloned());
    // La actualización no le contesta a la UI: no se instala nada mientras la app se cierra
    let answered = tokio::time::timeout(Duration::from_millis(300), &mut preparing).await;
    assert!(answered.is_err());
    h.backend.update_abort();
    assert!(
        h.backend.is_shut_down(),
        "con la salida pedida no se vuelve atrás"
    );
    assert!(!h.backend.take_reopen_after_update(), "salió, no actualizó");
}

#[tokio::test]
async fn salir_desde_el_tray_mientras_se_prepara_una_actualizacion_tampoco_se_pierde() {
    let h = harness(Setup::default()).await;
    ready(&h, 1).await;
    h.screen.paint([0, 255, 0]);
    h.backend.session_start().await.unwrap();
    eventually("conectado", Duration::from_secs(8), || {
        h.sims[0].dp("21") == Some(json!("colour"))
    })
    .await;
    let backend = h.backend.clone();
    let mut preparing = tokio::spawn(async move { backend.update_prepare().await });
    tokio::task::yield_now().await;

    AppTray::new(h.backend.clone()).restore_and_quit().await;

    assert!(h.backend.is_shut_down());
    let answered = tokio::time::timeout(Duration::from_millis(300), &mut preparing).await;
    assert!(answered.is_err());
}
