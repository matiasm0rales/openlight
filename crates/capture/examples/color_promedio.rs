//! Imprime el color promedio de un monitor y los cuadros por segundo que llegan.
//!
//! ```text
//! cargo run -p capture --example color_promedio -- [display_id] [fps]
//! ```
//!
//! Sin `display_id` usa el primer monitor externo (o el integrado si no hay otro). En Linux con
//! Wayland, la primera vez el portal muestra el diálogo; el `restore_token` se guarda en
//! `$TMPDIR/openlight-restore-token` para que la segunda corrida no lo muestre.
//!
//! El backend de cada sistema expone `capture::PlatformCapture`. Cuando se suma uno, se agrega su
//! `target_os` a los `cfg` de este archivo.

#[cfg(target_os = "linux")]
fn main() {
    real::main();
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("todavía no hay backend de captura para este sistema");
}

#[cfg(target_os = "linux")]
mod real {
    use std::time::{Duration, Instant};

    use capture::PlatformCapture;
    use openlight_types::{DisplayId, Frame, PermissionState, ScreenSource};

    pub fn main() {
        let mut args = std::env::args().skip(1);
        let wanted = args.next().filter(|a| !a.is_empty() && a != "-");
        let fps: f32 = args.next().and_then(|f| f.parse().ok()).unwrap_or(20.0);

        let mut source = PlatformCapture::new().unwrap_or_else(|e| fail(&e));
        load_token(&mut source);

        let displays = source.displays().unwrap_or_else(|e| fail(&e));
        println!("Monitores:");
        for d in &displays {
            println!(
                "  {:<16} {}×{}{}{}  {}",
                d.id.0,
                d.width,
                d.height,
                if d.primary { " primario" } else { "" },
                if d.builtin { " integrado" } else { "" },
                d.name
            );
        }
        let display = match wanted {
            Some(id) => DisplayId(id),
            None => displays
                .iter()
                .find(|d| !d.builtin)
                .or(displays.first())
                .map(|d| d.id.clone())
                .unwrap_or_else(|| fail(&"no hay monitores")),
        };

        if source.permission() == PermissionState::NotDetermined {
            println!("Pidiendo permiso de captura…");
            println!("  → {:?}", source.request_permission());
            save_token(&source);
        }

        println!("Capturando {display} a {fps} fps (Ctrl+C para salir)");
        let mut rx = source
            .start(display, (96, 54), fps)
            .unwrap_or_else(|e| fail(&e));
        save_token(&source);

        let mut frames = 0u32;
        let mut last: Option<Frame> = None;
        let mut window = Instant::now();
        loop {
            match rx.has_changed() {
                Ok(true) => {
                    last = rx.borrow_and_update().clone();
                    if last.is_some() {
                        frames += 1;
                    }
                }
                Ok(false) => {}
                Err(_) => fail(&"la captura terminó"),
            }
            if window.elapsed() >= Duration::from_secs(1) {
                let fps_real = f64::from(frames) / window.elapsed().as_secs_f64();
                match &last {
                    Some(frame) => {
                        let (r, g, b) = average(frame);
                        println!(
                            "{fps_real:5.1} fps  promedio #{r:02x}{g:02x}{b:02x}  ({r}, {g}, {b})"
                        );
                    }
                    None => println!("{fps_real:5.1} fps  sin pantalla"),
                }
                frames = 0;
                window = Instant::now();
                save_token(&source);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn average(frame: &Frame) -> (u8, u8, u8) {
        let mut sum = [0u64; 3];
        for px in frame.bgra.as_chunks::<4>().0 {
            sum[0] += u64::from(px[2]);
            sum[1] += u64::from(px[1]);
            sum[2] += u64::from(px[0]);
        }
        let n = u64::from(frame.width * frame.height).max(1);
        ((sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8)
    }

    /// El token del portal ScreenCast solo existe en Linux.
    #[cfg(target_os = "linux")]
    fn token_path() -> std::path::PathBuf {
        std::env::temp_dir().join("openlight-restore-token")
    }

    #[cfg(target_os = "linux")]
    fn load_token(source: &mut PlatformCapture) {
        if let Ok(token) = std::fs::read_to_string(token_path()) {
            source.set_restore_token(Some(token.trim().to_string()));
        }
    }

    #[cfg(target_os = "linux")]
    fn save_token(source: &PlatformCapture) {
        if let Some(token) = source.restore_token() {
            let _ = std::fs::write(token_path(), token);
        }
    }

    #[cfg(not(target_os = "linux"))]
    fn load_token(_: &mut PlatformCapture) {}

    #[cfg(not(target_os = "linux"))]
    fn save_token(_: &PlatformCapture) {}

    fn fail(e: &dyn std::fmt::Display) -> ! {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
