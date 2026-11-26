//! El orden de los focos: el que se agrega va al final, `devices_reorder` lo cambia y el tray y
//! la app ven la misma lista.
#![allow(missing_docs)]

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::{FAKE_ACCESS_ID, FAKE_ACCESS_SECRET, Harness, Setup, harness, sim_id};
use openlight_lib::shell::Shell;
use openlight_lib::tray::TrayBackend;
use openlight_lib::tray_backend::AppTray;
use openlight_types::ipc::{CloudCredentialsInput, DeviceAddRequest};
use openlight_types::{DeviceId, DisplayId, ErrorCode, FixAction, Region};

/// Cuenta cuántas veces se le avisó a la shell que algo cambió.
#[derive(Default)]
struct Changes(AtomicUsize);

impl Shell for Changes {
    fn changed(&self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }

    fn open_calibration(&self, _display: &DisplayId) -> Result<(), String> {
        Ok(())
    }

    fn close_calibration(&self) {}

    fn is_dismantled(&self) -> bool {
        false
    }

    fn restart(&self) {}
}

async fn add(h: &Harness, n: usize, name: &str) {
    h.backend
        .device_add(DeviceAddRequest {
            id: sim_id(n),
            name: name.to_owned(),
            ip: Some("127.0.0.1".into()),
        })
        .await
        .unwrap();
}

/// Tres focos agregados en un orden que no es ni el de sus ids ni el de sus nombres.
async fn three() -> Harness {
    let h = harness(Setup { sims: 3, lan: true }).await;
    h.backend
        .cloud_validate(CloudCredentialsInput {
            access_id: FAKE_ACCESS_ID.into(),
            access_secret: FAKE_ACCESS_SECRET.into(),
            region: Region::Us,
        })
        .await
        .unwrap();
    add(&h, 2, "Techo").await;
    add(&h, 3, "Velador").await;
    add(&h, 1, "Escritorio").await;
    h
}

fn listed(h: &Harness) -> Vec<DeviceId> {
    let devices = h.backend.devices_list().unwrap();
    devices.into_iter().map(|d| d.id).collect()
}

fn in_tray(h: &Harness) -> Vec<DeviceId> {
    let snapshot = AppTray::new(h.backend.clone()).snapshot();
    snapshot.devices.into_iter().map(|d| d.id).collect()
}

fn ids(numbers: [usize; 3]) -> Vec<DeviceId> {
    numbers.into_iter().map(sim_id).collect()
}

#[tokio::test]
async fn un_foco_nuevo_va_al_final_y_al_volver_a_agregarlo_no_se_mueve() {
    let h = three().await;
    assert_eq!(listed(&h), ids([2, 3, 1]));
    add(&h, 2, "Antes que todos").await;
    assert_eq!(listed(&h), ids([2, 3, 1]));
}

#[tokio::test]
async fn reordenar_cambia_la_lista_de_la_app_y_la_del_tray() {
    let h = three().await;
    let shell = Arc::new(Changes::default());
    h.backend.set_shell(shell.clone());

    h.backend.devices_reorder(ids([1, 2, 3])).unwrap();

    assert_eq!(listed(&h), ids([1, 2, 3]));
    assert_eq!(in_tray(&h), ids([1, 2, 3]));
    assert_eq!(
        h.backend.onboarding_state().unwrap().devices,
        ids([1, 2, 3])
    );
    assert_eq!(
        shell.0.load(Ordering::SeqCst),
        1,
        "el tray se vuelve a armar"
    );

    // Renombrar un foco no le quita el lugar elegido
    h.backend
        .device_update(DeviceAddRequest {
            id: sim_id(1),
            name: "Zócalo".into(),
            ip: None,
        })
        .unwrap();
    assert_eq!(listed(&h), ids([1, 2, 3]));
}

#[tokio::test]
async fn reordenar_con_otra_lista_de_focos_es_un_error_claro_y_no_cambia_nada() {
    let h = three().await;
    let shell = Arc::new(Changes::default());
    h.backend.set_shell(shell.clone());

    for list in [
        vec![sim_id(1), sim_id(2)],
        vec![sim_id(1), sim_id(2), sim_id(3), sim_id(4)],
        vec![sim_id(1), sim_id(1), sim_id(2)],
    ] {
        let error = h.backend.devices_reorder(list.clone()).unwrap_err();
        assert_eq!(error.code, ErrorCode::DeviceOrderMismatch, "{list:?}");
        assert_eq!(error.actions, [FixAction::Retry]);
    }
    assert_eq!(listed(&h), ids([2, 3, 1]));
    assert_eq!(shell.0.load(Ordering::SeqCst), 0);
}
