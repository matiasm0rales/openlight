//! Imprime el color promedio de un monitor y cuántos cuadros nuevos llegan por segundo.
//!
//! ```text
//! cargo run -p capture --release --example color_promedio -- [display_id] [fps]
//! ```
//!
//! Sin `display_id`, sigue el primer monitor externo (o el integrado si no hay otro). `fps` es 20
//! por defecto. En Windows, Graphics Capture manda cuadros solo cuando cambia la pantalla: con
//! la imagen quieta, "cuadros nuevos/s" baja (es lo esperado). Para medir, poné un video.
//!
//! Solo usa el trait `ScreenSource` y `capture::PlatformCapture`, así que sirve en los tres
//! sistemas a medida que cada backend define su `PlatformCapture`.

#[cfg(target_os = "windows")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::time::{Duration, Instant};

    use capture::PlatformCapture;
    use openlight_types::{DisplayId, ScreenSource};

    let mut args = std::env::args().skip(1);
    let pedido = args.next();
    let fps: f32 = args.next().map(|s| s.parse()).transpose()?.unwrap_or(20.0);

    let mut fuente = PlatformCapture::new()?;
    println!("Permiso de captura: {:?}", fuente.permission());
    let monitores = fuente.displays()?;
    println!("Monitores:");
    for m in &monitores {
        println!(
            "  {}  {} ({}×{}){}{}",
            m.id,
            m.name,
            m.width,
            m.height,
            if m.builtin { ", integrado" } else { "" },
            if m.primary { ", principal" } else { "" },
        );
    }
    let id = match pedido {
        Some(id) => DisplayId(id),
        None => monitores
            .iter()
            .find(|m| !m.builtin)
            .or(monitores.first())
            .ok_or("no hay monitores")?
            .id
            .clone(),
    };
    println!("Siguiendo {id} a {fps} fps. Ctrl+C para salir.");

    let mut rx = fuente.start(id, (96, 54), fps)?;
    let mut nuevos = 0u32;
    let mut desde = Instant::now();
    loop {
        if rx.has_changed()? {
            nuevos += 1;
        }
        let cuadro = rx.borrow_and_update().clone();
        if desde.elapsed() >= Duration::from_secs(1) {
            match cuadro {
                Some(c) => {
                    let n = (c.width * c.height) as u64;
                    let mut suma = [0u64; 3];
                    for p in c.bgra.as_chunks::<4>().0 {
                        suma[0] += u64::from(p[2]);
                        suma[1] += u64::from(p[1]);
                        suma[2] += u64::from(p[0]);
                    }
                    let [r, g, b] = suma.map(|s| s / n);
                    println!(
                        "rgb({r:3}, {g:3}, {b:3})  #{r:02x}{g:02x}{b:02x}  {nuevos:2} cuadros nuevos/s"
                    );
                }
                None => println!("sin pantalla  {nuevos:2} cuadros nuevos/s"),
            }
            nuevos = 0;
            desde = Instant::now();
        }
        std::thread::sleep(Duration::from_secs_f32(1.0 / fps));
    }
}

#[cfg(not(target_os = "windows"))]
fn main() {
    // Cuando cada backend define `capture::PlatformCapture`, este `cfg` se va.
    eprintln!("Todavía no hay backend de captura para este sistema en esta rama.");
}
