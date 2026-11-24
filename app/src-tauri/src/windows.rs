//! Ventanas de la app: la principal (que se esconde en vez de cerrarse) y la de calibración a
//! pantalla completa.

use openlight_types::DisplayId;
use tauri::{
    AppHandle, LogicalPosition, Manager, Monitor, PhysicalPosition, PhysicalSize, Position,
    Runtime, Size, WebviewUrl, WebviewWindowBuilder, Window, WindowEvent,
};

/// Etiqueta de la ventana principal (la de `tauri.conf.json`).
pub const MAIN: &str = "main";

/// Etiqueta de la ventana de calibración.
pub const CALIBRATION: &str = "calibration";

/// Ruta de la UI que se carga en la ventana de calibración (el router de la UI es por hash).
pub const CALIBRATION_ROUTE: &str = "index.html#/calibrate";

/// Muestra la ventana principal y le da el foco (desde el tray, o cuando se abre una segunda
/// instancia). Si estaba escondida o minimizada, la recupera.
pub fn show_main<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(MAIN) {
        window.unminimize()?;
        window.show()?;
        window.set_focus()?;
    }
    Ok(())
}

/// Esconde la ventana principal; la app sigue en el tray.
pub fn hide_main<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(MAIN) {
        window.hide()?;
    }
    Ok(())
}

/// Cerrar la ventana principal la esconde en vez de salir: la app sigue en el tray y los focos
/// siguen andando. Para salir de verdad está "Salir" del tray. Se engancha con
/// `Builder::on_window_event`.
pub fn on_window_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    if let WindowEvent::CloseRequested { api, .. } = event
        && hides_on_close(window.label())
    {
        api.prevent_close();
        let _ = window.hide();
    }
}

/// Si cerrar esa ventana la esconde (la principal) o la cierra de verdad (la calibración).
fn hides_on_close(label: &str) -> bool {
    label == MAIN
}

// ---------- Calibración a pantalla completa ----------

/// Un monitor de Tauri, reducido a lo que hace falta para reconocerlo.
#[derive(Debug, Clone, PartialEq)]
pub struct MonitorGeom {
    /// Nombre que informa el sistema (`\\.\DISPLAY1` en Windows, la salida en X11; en macOS no
    /// sirve: es un número de modelo).
    pub name: Option<String>,
    /// Esquina superior izquierda, en píxeles físicos.
    pub position: (i32, i32),
    /// Factor de escala.
    pub scale: f64,
}

impl MonitorGeom {
    fn of(monitor: &Monitor) -> Self {
        Self {
            name: monitor.name().cloned(),
            position: (monitor.position().x, monitor.position().y),
            scale: monitor.scale_factor(),
        }
    }
}

/// Qué monitor de Tauri es `display`.
///
/// Los ids los arma el crate `capture` ([`DisplayId`]):
///
/// - **Windows y Linux**: el id es el nombre del monitor (`\\.\DISPLAY1`, `DP-1`), el mismo que
///   informa Tauri: se busca por nombre.
/// - **macOS**: el id es el `CGDirectDisplayID`, que Tauri no expone (el nombre que da es un
///   número de modelo). Se reconoce por la posición: `origin_points` es la esquina del monitor en
///   puntos (`CGDisplayBounds`), y Tauri la informa en píxeles físicos (= puntos × escala).
///
/// `None` si ninguno coincide (monitor desconectado, o ids de Wayland que no se parecen a los
/// nombres de GDK): quien llama elige el principal.
pub fn find_monitor(
    display: &DisplayId,
    origin_points: Option<(f64, f64)>,
    monitors: &[MonitorGeom],
) -> Option<usize> {
    let por_nombre: Vec<usize> = (0..monitors.len())
        .filter(|&i| monitors[i].name.as_deref() == Some(display.0.as_str()))
        .collect();
    if let [unico] = por_nombre[..] {
        return Some(unico);
    }
    let (x, y) = origin_points?;
    monitors.iter().position(|m| {
        let esperado = ((x * m.scale).round() as i32, (y * m.scale).round() as i32);
        m.position == esperado
    })
}

/// Esquina del monitor `display` en puntos, si el sistema lo permite (solo macOS).
#[cfg(target_os = "macos")]
fn origin_points(display: &DisplayId) -> Option<(f64, f64)> {
    let id: u32 = display.0.parse().ok()?;
    let bounds = objc2_core_graphics::CGDisplayBounds(id);
    Some((bounds.origin.x, bounds.origin.y))
}

#[cfg(not(target_os = "macos"))]
fn origin_points(_display: &DisplayId) -> Option<(f64, f64)> {
    None
}

/// El monitor de Tauri de `display`; si no se reconoce, el principal, y si no hay, el primero.
fn monitor_for<R: Runtime>(app: &AppHandle<R>, display: &DisplayId) -> tauri::Result<Monitor> {
    let monitors = app.available_monitors()?;
    let geoms: Vec<MonitorGeom> = monitors.iter().map(MonitorGeom::of).collect();
    if let Some(i) = find_monitor(display, origin_points(display), &geoms) {
        return Ok(monitors[i].clone());
    }
    match app.primary_monitor()? {
        Some(primary) => Ok(primary),
        None => monitors.into_iter().next().ok_or_else(|| {
            tauri::Error::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "no hay monitores",
            ))
        }),
    }
}

/// Abre la calibración a pantalla completa, sin bordes, en el monitor `display`, con la ruta de
/// calibración de la UI. Si ya está abierta, la mueve a ese monitor y le da el foco.
pub fn open_calibration(app: &AppHandle, display: DisplayId) -> tauri::Result<()> {
    open_calibration_in(app, &display)
}

fn open_calibration_in<R: Runtime>(app: &AppHandle<R>, display: &DisplayId) -> tauri::Result<()> {
    let monitor = monitor_for(app, display)?;
    let window = match app.get_webview_window(CALIBRATION) {
        Some(existing) => {
            // Sale de pantalla completa para poder moverla al otro monitor.
            existing.set_fullscreen(false)?;
            existing
        }
        None => {
            WebviewWindowBuilder::new(app, CALIBRATION, WebviewUrl::App(CALIBRATION_ROUTE.into()))
                .title("OpenLight")
                .decorations(false)
                .visible(false)
                .build()?
        }
    };
    // La ventana va primero al monitor elegido y recién ahí pasa a pantalla completa: la
    // pantalla completa se hace en el monitor donde está la ventana.
    window.set_position(position_on(&monitor))?;
    window.set_size(Size::Physical(PhysicalSize::new(
        monitor.size().width,
        monitor.size().height,
    )))?;
    window.show()?;
    window.set_fullscreen(true)?;
    window.set_focus()
}

/// Posición de la esquina del monitor, en la unidad que cada sistema interpreta bien.
///
/// macOS convierte una posición física con el factor de escala del monitor donde está la ventana
/// *ahora*, no el de destino: al pasar de un monitor de escala 1 a uno Retina la ventana termina
/// al doble de distancia. Con coordenadas lógicas (puntos, las de `CGDisplayBounds`) no hay
/// conversión que equivocar. En Windows y Linux, en cambio, lo seguro son los píxeles físicos.
fn position_on(monitor: &Monitor) -> Position {
    let (x, y) = (monitor.position().x, monitor.position().y);
    if cfg!(target_os = "macos") {
        let scale = monitor.scale_factor();
        Position::Logical(LogicalPosition::new(
            f64::from(x) / scale,
            f64::from(y) / scale,
        ))
    } else {
        Position::Physical(PhysicalPosition::new(x, y))
    }
}

/// Cierra la ventana de calibración (si no está abierta, no hace nada).
pub fn close_calibration(app: &AppHandle) -> tauri::Result<()> {
    match app.get_webview_window(CALIBRATION) {
        Some(window) => window.close(),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(name: &str, position: (i32, i32), scale: f64) -> MonitorGeom {
        MonitorGeom {
            name: Some(name.into()),
            position,
            scale,
        }
    }

    fn id(s: &str) -> DisplayId {
        DisplayId(s.into())
    }

    #[test]
    fn windows_se_reconoce_por_el_nombre_del_dispositivo() {
        let monitores = [
            monitor(r"\\.\DISPLAY1", (0, 0), 1.0),
            monitor(r"\\.\DISPLAY2", (1920, 0), 1.0),
        ];
        assert_eq!(
            find_monitor(&id(r"\\.\DISPLAY2"), None, &monitores),
            Some(1)
        );
        assert_eq!(
            find_monitor(&id(r"\\.\DISPLAY1"), None, &monitores),
            Some(0)
        );
    }

    #[test]
    fn linux_se_reconoce_por_el_nombre_de_la_salida() {
        let monitores = [
            monitor("eDP-1", (0, 0), 2.0),
            monitor("DP-3", (1920, 0), 1.0),
        ];
        assert_eq!(find_monitor(&id("DP-3"), None, &monitores), Some(1));
    }

    #[test]
    fn macos_se_reconoce_por_la_posicion_en_puntos_por_la_escala() {
        // Una pantalla Retina (escala 2) a la izquierda y un externo (escala 1) a la derecha: el
        // externo empieza en el punto 1512, que Tauri informa como píxel 1512.
        let monitores = [
            monitor("Monitor #1", (0, 0), 2.0),
            monitor("Monitor #2", (1512, -120), 1.0),
        ];
        assert_eq!(
            find_monitor(&id("69733382"), Some((1512.0, -120.0)), &monitores),
            Some(1)
        );
        assert_eq!(
            find_monitor(&id("1"), Some((0.0, 0.0)), &monitores),
            Some(0)
        );
    }

    #[test]
    fn macos_en_retina_la_posicion_en_pixeles_es_el_doble() {
        // Un externo Retina a la derecha de la integrada: empieza en el punto 1000 (píxel 2000).
        let monitores = [
            monitor("Monitor #1", (0, 0), 2.0),
            monitor("Monitor #2", (2000, 0), 2.0),
        ];
        assert_eq!(
            find_monitor(&id("7"), Some((1000.0, 0.0)), &monitores),
            Some(1)
        );
    }

    #[test]
    fn el_nombre_manda_sobre_la_posicion() {
        let monitores = [monitor("A", (0, 0), 1.0), monitor("B", (1000, 0), 1.0)];
        assert_eq!(
            find_monitor(&id("B"), Some((0.0, 0.0)), &monitores),
            Some(1)
        );
    }

    #[test]
    fn nombres_repetidos_no_alcanzan_y_se_prueba_con_la_posicion() {
        let monitores = [
            monitor("DELL", (0, 0), 1.0),
            monitor("DELL", (1920, 0), 1.0),
        ];
        assert_eq!(find_monitor(&id("DELL"), None, &monitores), None);
        assert_eq!(
            find_monitor(&id("DELL"), Some((1920.0, 0.0)), &monitores),
            Some(1)
        );
    }

    #[test]
    fn un_monitor_desconocido_no_se_adivina() {
        let monitores = [monitor("DP-1", (0, 0), 1.0)];
        assert_eq!(find_monitor(&id("HDMI-9"), None, &monitores), None);
        assert_eq!(
            find_monitor(&id("HDMI-9"), Some((5000.0, 0.0)), &monitores),
            None
        );
        assert_eq!(find_monitor(&id("x"), None, &[]), None);
    }

    #[test]
    fn solo_la_principal_se_esconde_al_cerrar() {
        assert!(hides_on_close(MAIN));
        assert!(!hides_on_close(CALIBRATION));
    }

    #[test]
    fn la_ruta_de_calibracion_es_la_del_router_de_la_ui() {
        // El router de la UI lee `#/calibrate` (app/ui/src/lib/router.svelte.ts).
        assert!(CALIBRATION_ROUTE.ends_with("#/calibrate"));
    }
}
