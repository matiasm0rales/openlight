//! Pruebas del backend de Windows contra el sistema real. Corren en el job de Windows del CI.
//!
//! Las que necesitan capturar de verdad un escritorio llevan `#[ignore]` si el runner no lo
//! permite; se corren a mano con `cargo test -p capture -- --ignored` en una PC con Windows.
#![cfg(target_os = "windows")]

use std::time::{Duration, Instant};

use capture::windows::{WindowsCapture, exclude_window_from_capture};
use openlight_types::{CaptureError, DisplayId, PermissionState, ScreenSource};

#[test]
fn lista_monitores_sin_error() {
    let fuente = WindowsCapture::new().unwrap();
    let monitores = fuente.displays().expect("displays() no debería fallar");
    println!("{monitores:#?}");
    assert!(monitores.iter().filter(|m| m.primary).count() <= 1);
    for m in &monitores {
        assert!(m.id.0.starts_with(r"\\.\"), "id inesperado: {}", m.id);
        assert!(m.width > 0 && m.height > 0);
        assert!(!m.name.is_empty());
    }
}

#[test]
fn permiso_no_aplica_en_windows() {
    let fuente = WindowsCapture::new().unwrap();
    assert_eq!(fuente.permission(), PermissionState::NotApplicable);
    assert_eq!(fuente.request_permission(), PermissionState::NotApplicable);
}

#[test]
fn monitor_inexistente_da_display_not_found() {
    let mut fuente = WindowsCapture::new().unwrap();
    let id = DisplayId(r"\\.\DISPLAY99".into());
    match fuente.start(id.clone(), (96, 54), 20.0) {
        Err(CaptureError::DisplayNotFound(d)) => assert_eq!(d, id),
        Err(CaptureError::Unsupported(_)) => {} // Windows sin Graphics Capture
        otro => panic!("se esperaba DisplayNotFound, llegó {otro:?}"),
    }
}

#[test]
fn tamano_cero_es_un_error() {
    let mut fuente = WindowsCapture::new().unwrap();
    assert!(fuente.start(DisplayId("x".into()), (0, 54), 20.0).is_err());
}

#[test]
fn excluir_una_ventana_invalida_falla() {
    assert!(exclude_window_from_capture(0).is_err());
}

/// Necesita un escritorio con Graphics Capture funcionando (GPU o WARP y una sesión interactiva).
#[test]
// PRUEBA-CI: sin #[ignore] por ahora, para ver si el runner puede capturar.
fn captura_cuadros_del_monitor_principal() {
    let mut fuente = WindowsCapture::new().unwrap();
    let monitores = fuente.displays().unwrap();
    let principal = monitores
        .iter()
        .find(|m| m.primary)
        .or(monitores.first())
        .expect("no hay monitores");
    let rx = fuente.start(principal.id.clone(), (96, 54), 20.0).unwrap();

    let limite = Instant::now() + Duration::from_secs(5);
    let cuadro = loop {
        if let Some(c) = rx.borrow().clone() {
            break c;
        }
        assert!(Instant::now() < limite, "no llegó ningún cuadro en 5 s");
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!((cuadro.width, cuadro.height), (96, 54));
    assert_eq!(cuadro.bgra.len(), 96 * 54 * 4);
    assert!(cuadro.bgra.as_chunks::<4>().0.iter().all(|p| p[3] == 255));

    // stop() termina el hilo: el emisor se suelta y el canal queda cerrado.
    fuente.stop();
    assert!(rx.has_changed().is_err());
}
