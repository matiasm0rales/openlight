//! Ventanas de la app: la principal y la de calibración a pantalla completa.
//!
//! La principal **se destruye al cerrarla**: escondida, su proceso WebContent seguía ocupando
//! cientos de MB. La app sigue en el tray y la ventana se vuelve a crear al abrirla, con el
//! mismo tamaño y en el mismo lugar. Su estado vive en el backend: la página nueva lo pide al
//! cargar (ver [`Backend::ui_ready`]). Solo se esconde en vez de destruirse mientras la página
//! tiene algo a medias ([`Backend::ui_hold`]).

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use openlight_types::DisplayId;
use tauri::{
    AppHandle, LogicalPosition, LogicalSize, Manager, Monitor, PhysicalPosition, PhysicalSize,
    Position, Runtime, Size, WebviewUrl, WebviewWindow, WebviewWindowBuilder, Window, WindowEvent,
};

use crate::state::Backend;

/// Cuánto se espera, al perder el foco una ventana, para volver a mirar si sigue a la vista: al
/// minimizar (macOS) el foco se pierde antes de que la ventana figure minimizada.
const SETTLE: Duration = Duration::from_millis(800);

/// Etiqueta de la ventana principal (la de `tauri.conf.json`).
pub const MAIN: &str = "main";

/// Etiqueta de la ventana de calibración.
pub const CALIBRATION: &str = "calibration";

/// Ruta de la UI que se carga en la ventana de calibración (el router de la UI es por hash).
pub const CALIBRATION_ROUTE: &str = "index.html#/calibrate";

/// Muestra la ventana principal y le da el foco (desde el tray, el Dock, o cuando se abre una
/// segunda instancia). Si estaba escondida o minimizada, la recupera; si estaba cerrada, la
/// vuelve a crear.
///
/// Se la puede llamar desde un manejador de eventos: en Windows crear una ventana ahí adentro
/// traba la app (wry#583), así que la crea desde otro hilo.
pub fn show_main<R: Runtime>(app: &AppHandle<R>) {
    if app.get_webview_window(MAIN).is_some() {
        open(app);
    } else {
        let app = app.clone();
        tauri::async_runtime::spawn_blocking(move || open(&app));
    }
}

/// Crea la ventana principal (si hace falta) y la trae al frente. Directo solo al arrancar o
/// fuera de un manejador de eventos; si no, [`show_main`].
pub fn open_main<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let window = match app.get_webview_window(MAIN) {
        Some(window) => window,
        None => create_main(app)?,
    };
    window.unminimize()?;
    window.show()?;
    window.set_focus()?;
    sync_visibility(app, None);
    Ok(())
}

fn open<R: Runtime>(app: &AppHandle<R>) {
    if let Err(error) = open_main(app) {
        tracing::warn!(%error, "no se pudo abrir la ventana principal");
    }
}

/// La ventana principal, con la configuración de `tauri.conf.json` (allí tiene `create: false`:
/// al arrancar sola con la sesión del sistema ni se la crea) y donde había quedado.
fn create_main<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<WebviewWindow<R>> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == MAIN)
        .ok_or(tauri::Error::WindowNotFound)?;
    let place = remembered(app);
    let window = WebviewWindowBuilder::from_config(app, config)?
        // Escondida hasta que esté en su lugar, para que no salte
        .visible(place.is_none() && config.visible)
        .build()?;
    if let Some(place) = place {
        place.restore(&window);
    }
    Ok(window)
}

/// Dónde estaba la ventana principal al cerrarla, en píxeles físicos.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Place {
    position: (i32, i32),
    size: (u32, u32),
    scale: f64,
    maximized: bool,
}

/// El lugar de la última ventana principal que se cerró.
#[derive(Default)]
struct LastPlace(Mutex<Option<Place>>);

impl Place {
    fn of<R: Runtime>(window: &Window<R>) -> Option<Self> {
        // Minimizada o a pantalla completa no dice dónde quedó
        if window.is_minimized().unwrap_or(false) || window.is_fullscreen().unwrap_or(false) {
            return None;
        }
        let position = window.outer_position().ok()?;
        let size = window.inner_size().ok()?;
        Some(Self {
            position: (position.x, position.y),
            size: (size.width, size.height),
            scale: window.scale_factor().ok()?,
            maximized: window.is_maximized().unwrap_or(false),
        })
    }

    /// Pone la ventana donde estaba. Si ese lugar ya no está en ningún monitor, queda donde la
    /// puso el sistema.
    fn restore<R: Runtime>(&self, window: &WebviewWindow<R>) {
        let monitors = window.available_monitors().unwrap_or_default();
        let areas: Vec<_> = monitors
            .iter()
            .map(|m| {
                let (p, s) = (m.position(), m.size());
                ((p.x, p.y), (s.width, s.height))
            })
            .collect();
        let place = || -> tauri::Result<()> {
            if reachable(self.position, self.size, &areas) {
                window.set_position(physical_position(self.position, self.scale))?;
                window.set_size(physical_size(self.size, self.scale))?;
            }
            // Después de moverla, así se maximiza en el monitor donde estaba
            if self.maximized {
                window.maximize()?;
            }
            window.show()
        };
        if let Err(error) = place() {
            tracing::warn!(%error, "no se pudo volver a poner la ventana donde estaba");
        }
    }
}

/// El rectángulo de un monitor en píxeles físicos: esquina y tamaño.
type Area = ((i32, i32), (u32, u32));

/// Si una ventana ahí se puede agarrar: algo de su borde de arriba cae dentro de un monitor.
fn reachable((x, y): (i32, i32), (width, _): (u32, u32), monitors: &[Area]) -> bool {
    let right = i64::from(x) + i64::from(width);
    monitors.iter().any(|&((mx, my), (mw, mh))| {
        let (mx, my) = (i64::from(mx), i64::from(my));
        i64::from(x) < mx + i64::from(mw)
            && right > mx
            && i64::from(y) >= my
            && i64::from(y) < my + i64::from(mh)
    })
}

fn remember<R: Runtime>(app: &AppHandle<R>, window: &Window<R>) {
    let Some(place) = Place::of(window) else {
        return;
    };
    // `manage` no pisa lo que ya hay
    app.manage(LastPlace::default());
    let last = app.state::<LastPlace>();
    *last.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(place);
}

fn remembered<R: Runtime>(app: &AppHandle<R>) -> Option<Place> {
    let last = app.try_state::<LastPlace>()?;
    *last.0.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Cerrar la ventana principal la destruye, salvo que la página tenga algo a medias
/// ([`Backend::ui_hold`]): ahí se esconde. En los dos casos la app sigue en el tray y los focos
/// siguen andando. Para salir de verdad está "Salir" del tray. Se engancha con
/// `Builder::on_window_event`; que la app no termine al quedarse sin ventanas lo resuelve
/// [`keep_running`].
pub fn on_window_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    let app = window.app_handle();
    if let WindowEvent::CloseRequested { api, .. } = event
        && window.label() == MAIN
    {
        let holds = app
            .try_state::<Arc<Backend>>()
            .is_some_and(|backend| backend.keeps_window());
        if holds {
            api.prevent_close();
            let _ = window.hide();
        } else {
            remember(app, window);
        }
    }
    match event {
        WindowEvent::CloseRequested { .. }
        | WindowEvent::Resized(_)
        | WindowEvent::Focused(true) => sync_visibility(app, None),
        WindowEvent::Focused(false) => {
            sync_visibility(app, None);
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(SETTLE).await;
                sync_visibility(&app, None);
            });
        }
        WindowEvent::Destroyed => destroyed(app, window.label()),
        _ => {}
    }
}

/// Se destruyó la ventana `label` (que todavía puede figurar entre las ventanas).
pub fn destroyed<R: Runtime>(app: &AppHandle<R>, label: &str) {
    sync_visibility(app, Some(label));
    let Some(backend) = app.try_state::<Arc<Backend>>() else {
        return;
    };
    if label == MAIN {
        backend.ui_closed();
    }
    // Sin ninguna ventana no hay desde dónde seguir una calibración: se cancela, así el foco
    // vuelve a su estado y queda libre
    if app.webview_windows().keys().all(|other| other == label) {
        let backend = Arc::clone(&backend);
        tauri::async_runtime::spawn(async move {
            if let Err(error) = backend.calibration_cancel().await {
                tracing::warn!(%error, "no se pudo cancelar la calibración sin ventanas");
            }
        });
    }
}

/// Que se haya cerrado la última ventana no termina la app (sigue en el tray). Se llama con cada
/// evento de `App::run`; salir de verdad llega con un código y no pasa por acá.
pub fn keep_running(event: &tauri::RunEvent) {
    if let tauri::RunEvent::ExitRequested {
        code: None, api, ..
    } = event
    {
        api.prevent_exit();
    }
}

/// Le dice al backend si queda alguna ventana a la vista (sin contar `closing`): con ninguna, deja
/// de mandarle a la UI lo que solo sirve para dibujar (ver [`crate::events::UiGate`]).
pub fn sync_visibility<R: Runtime>(app: &AppHandle<R>, closing: Option<&str>) {
    let Some(backend) = app.try_state::<Arc<Backend>>() else {
        return;
    };
    let visible = app.webview_windows().iter().any(|(label, window)| {
        Some(label.as_str()) != closing
            && on_screen(window.is_visible().ok(), window.is_minimized().ok())
    });
    backend.set_ui_visible(visible);
}

/// Si una ventana está a la vista: ni escondida ni minimizada. Si el sistema no contesta, se
/// asume que sí: es peor dejar la UI sin datos que mandarle de más.
fn on_screen(visible: Option<bool>, minimized: Option<bool>) -> bool {
    visible.unwrap_or(true) && !minimized.unwrap_or(false)
}

/// Un tamaño en píxeles físicos, en la misma unidad que [`physical_position`].
fn physical_size((width, height): (u32, u32), scale: f64) -> Size {
    if cfg!(target_os = "macos") {
        Size::Logical(LogicalSize::new(
            f64::from(width) / scale,
            f64::from(height) / scale,
        ))
    } else {
        Size::Physical(PhysicalSize::new(width, height))
    }
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

/// Posición de la esquina del monitor (ver [`physical_position`]).
fn position_on(monitor: &Monitor) -> Position {
    physical_position(
        (monitor.position().x, monitor.position().y),
        monitor.scale_factor(),
    )
}

/// Una posición en píxeles físicos (de una pantalla de escala `scale`), en la unidad que cada
/// sistema interpreta bien.
///
/// macOS convierte una posición física con el factor de escala del monitor donde está la ventana
/// *ahora*, no el de destino: al pasar de un monitor de escala 1 a uno Retina la ventana termina
/// al doble de distancia. Con coordenadas lógicas (puntos, las de `CGDisplayBounds`) no hay
/// conversión que equivocar. En Windows y Linux, en cambio, lo seguro son los píxeles físicos.
fn physical_position((x, y): (i32, i32), scale: f64) -> Position {
    if cfg!(target_os = "macos") {
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
    fn escondida_o_minimizada_no_esta_a_la_vista() {
        assert!(on_screen(Some(true), Some(false)));
        assert!(!on_screen(Some(false), Some(false)));
        assert!(!on_screen(Some(true), Some(true)));
        assert!(on_screen(None, None));
    }

    #[test]
    fn un_lugar_guardado_solo_vale_si_la_ventana_se_puede_agarrar() {
        let monitores = [((0, 0), (1920, 1080)), ((1920, -200), (2560, 1440))];
        let ventana = (960, 680);
        assert!(reachable((100, 50), ventana, &monitores));
        // En el segundo monitor, aunque empiece más arriba que el primero
        assert!(reachable((3000, -150), ventana, &monitores));
        // Asomando por un costado
        assert!(reachable((-900, 10), ventana, &monitores));
        // Del todo afuera, o con el borde de arriba fuera de la pantalla
        assert!(!reachable((-960, 10), ventana, &monitores));
        assert!(!reachable((100, -100), ventana, &monitores));
        assert!(!reachable((100, 1080), ventana, &monitores));
        // El segundo monitor se desconectó
        assert!(!reachable((3000, 10), ventana, &monitores[..1]));
        assert!(!reachable((100, 50), ventana, &[]));
    }

    #[test]
    fn la_ruta_de_calibracion_es_la_del_router_de_la_ui() {
        // El router de la UI lee `#/calibrate` (app/ui/src/lib/router.svelte.ts).
        assert!(CALIBRATION_ROUTE.ends_with("#/calibrate"));
    }
}
