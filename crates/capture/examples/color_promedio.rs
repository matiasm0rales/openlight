//! Imprime el color promedio de la pantalla y cuántos cuadros por segundo llegan.
//!
//! ```text
//! cargo run -p capture --example color_promedio --release -- [display_id] [fps]
//! ```
//!
//! Sin `display_id` sigue el monitor que elige `capture::common::pick_display` (el primer
//! externo, o el integrado) y cambia de monitor cuando se enchufa o se desenchufa uno. Cada
//! cambio en la lista de monitores se imprime en su propia línea.
//!
//! Es uno solo para los tres sistemas (convención de E1/E2/E3): usa `capture::PlatformCapture`.
//! Cada backend agrega su `target_os` a la lista de `cfg` de abajo cuando exporta su alias.
//!
//! En Linux con Wayland, la primera vez el portal muestra el diálogo; el `restore_token` se guarda
//! en `$TMPDIR/openlight-restore-token` para que la segunda corrida no lo muestre.

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
fn main() {
    if let Err(e) = real::run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn main() {
    eprintln!("todavía no hay backend de captura para este sistema");
}

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
mod real {
    use std::io::Write;
    use std::time::{Duration, Instant};

    use capture::PlatformCapture;
    use capture::common::{average_rgb, pick_display};
    use openlight_types::{CaptureError, DisplayId, DisplayInfo, PermissionState, ScreenSource};

    const TARGET: (u32, u32) = (96, 54);

    fn listar(displays: &[DisplayInfo]) -> String {
        if displays.is_empty() {
            return "ninguno".into();
        }
        displays
            .iter()
            .map(|d| {
                let mut marcas = Vec::new();
                if d.builtin {
                    marcas.push("integrado");
                }
                if d.primary {
                    marcas.push("principal");
                }
                format!(
                    "{} «{}» {}×{} [{}]",
                    d.id,
                    d.name,
                    d.width,
                    d.height,
                    marcas.join(", ")
                )
            })
            .collect::<Vec<_>>()
            .join(" | ")
    }

    pub fn run() -> Result<(), CaptureError> {
        let mut args = std::env::args().skip(1);
        let pedido = args.next().filter(|a| a != "-").map(DisplayId);
        let fps: f32 = args.next().and_then(|a| a.parse().ok()).unwrap_or(20.0);

        let mut fuente = PlatformCapture::new()?;
        super::token::cargar(&mut fuente);
        match fuente.permission() {
            PermissionState::Granted | PermissionState::NotApplicable => {}
            _ => {
                if fuente.request_permission() == PermissionState::Denied {
                    return Err(CaptureError::PermissionDenied);
                }
            }
        }

        let tick = Duration::from_secs_f32(1.0 / fps);
        let mut pantallas = fuente.displays()?;
        println!("monitores: {}", listar(&pantallas));
        let mut siguiendo: Option<DisplayId> = None;
        let mut rx = None;
        let mut proxima_revision = Instant::now();
        let mut ventana = Instant::now();
        let (mut cuadros, mut medido) = (0u32, 0.0f32);
        let mut proximo_tick = Instant::now();

        loop {
            let ahora = Instant::now();
            if ahora >= proxima_revision {
                proxima_revision = ahora + Duration::from_secs(1);
                let nuevas = fuente.displays()?;
                if nuevas != pantallas {
                    println!("\nmonitores: {}", listar(&nuevas));
                    pantallas = nuevas;
                }
                let elegido = pick_display(pedido.as_ref(), &pantallas);
                if elegido != siguiendo {
                    fuente.stop();
                    rx = None;
                    if let Some(id) = &elegido {
                        match fuente.start(id.clone(), TARGET, fps) {
                            Ok(r) => {
                                super::token::guardar(&fuente);
                                println!("\nsiguiendo el monitor {id} a {fps} fps");
                                rx = Some(r);
                            }
                            // Se fue justo entre la lista y el start: se reintenta en la
                            // próxima revisión.
                            Err(CaptureError::DisplayNotFound(_)) => {}
                            Err(e) => return Err(e),
                        }
                    } else {
                        println!("\nesperando que aparezca el monitor...");
                    }
                    siguiendo = rx.as_ref().and(elegido);
                }
            }

            if let Some(rx) = rx.as_mut() {
                if rx.has_changed().unwrap_or(false) {
                    cuadros += 1;
                }
                let cuadro = rx.borrow_and_update().clone();
                let texto = match cuadro.as_ref().and_then(|c| average_rgb(&c.bgra)) {
                    Some((r, g, b)) => {
                        let c = |v: f32| (v * 255.0).round() as u8;
                        format!("#{:02x}{:02x}{:02x}", c(r), c(g), c(b))
                    }
                    None => "sin pantalla".into(),
                };
                print!("\r{texto:<12} {medido:5.1} cuadros/s ");
                let _ = std::io::stdout().flush();
            }
            if ventana.elapsed() >= Duration::from_secs(1) {
                medido = cuadros as f32 / ventana.elapsed().as_secs_f32();
                cuadros = 0;
                ventana = Instant::now();
            }

            proximo_tick += tick;
            match proximo_tick.checked_duration_since(Instant::now()) {
                Some(espera) => std::thread::sleep(espera),
                None => proximo_tick = Instant::now(),
            }
        }
    }
}

/// El `restore_token` del portal ScreenCast solo existe en Linux (Wayland).
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
mod token {
    use capture::PlatformCapture;

    #[cfg(target_os = "linux")]
    fn ruta() -> std::path::PathBuf {
        std::env::temp_dir().join("openlight-restore-token")
    }

    #[cfg(target_os = "linux")]
    pub fn cargar(fuente: &mut PlatformCapture) {
        if let Ok(token) = std::fs::read_to_string(ruta()) {
            fuente.set_restore_token(Some(token.trim().to_string()));
        }
    }

    #[cfg(target_os = "linux")]
    pub fn guardar(fuente: &PlatformCapture) {
        if let Some(token) = fuente.restore_token() {
            let _ = std::fs::write(ruta(), token);
        }
    }

    #[cfg(not(target_os = "linux"))]
    pub fn cargar(_: &mut PlatformCapture) {}

    #[cfg(not(target_os = "linux"))]
    pub fn guardar(_: &PlatformCapture) {}
}
