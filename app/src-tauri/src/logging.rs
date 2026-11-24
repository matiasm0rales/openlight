//! El log de la app: un subscriber de `tracing` que escribe a un archivo con rotación diaria
//! (y a stderr en desarrollo).
//!
//! Qué se registra lo decide `RUST_LOG`; sin él, `info` en general y `debug` para los crates
//! propios que explican por qué un foco no responde. **Los secretos no entran al log**: los
//! `Secret` no implementan `Display` ni `Serialize`, y ningún `tracing::*!` recibe claves, el
//! Access Secret ni tokens (hay una prueba en `tests/diagnostico.rs` que lo comprueba).

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use openlight_types::ipc::{LogLevel, LogLine};
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::Layer;
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::util::SubscriberInitExt;

/// Filtro cuando no hay `RUST_LOG`.
pub const DEFAULT_FILTER: &str = "info,tuya_device=debug,openlight_core=debug,openlight_lib=debug";

/// Cuántos archivos diarios se guardan.
pub const KEEP_DAYS: usize = 7;

/// Prefijo de los archivos: `openlight.2026-11-24.log`.
pub const FILE_PREFIX: &str = "openlight";
/// Sufijo de los archivos de log.
pub const FILE_SUFFIX: &str = "log";

/// Cuántas líneas recientes guarda el anillo en memoria (alimenta el visor del diagnóstico).
pub const RING_CAPACITY: usize = 2000;

/// El filtro de `RUST_LOG`, o [`DEFAULT_FILTER`] si no está o no se entiende.
pub fn env_filter() -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER))
}

/// La carpeta de logs: `<datos>/logs` si `OPENLIGHT_DATA_DIR` redirige la carpeta de datos (las
/// pruebas a mano no escriben en la carpeta real) y la de logs del sistema si no.
pub fn log_dir<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    data_dir: &Path,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    use tauri::Manager;
    if std::env::var_os("OPENLIGHT_DATA_DIR").is_some() {
        Ok(data_dir.join("logs"))
    } else {
        Ok(app.path().app_log_dir()?)
    }
}

/// Lo que el backend necesita para los comandos de diagnóstico: las últimas líneas y la carpeta
/// donde están los archivos.
#[derive(Clone)]
pub struct LogHandle {
    /// Anillo con las últimas líneas.
    pub ring: Arc<LogRing>,
    /// Carpeta de los archivos de log.
    pub dir: PathBuf,
}

/// Mantiene vivo el hilo que escribe el archivo: al soltarlo se vacía el búfer.
pub struct LogGuard {
    _file: Option<WorkerGuard>,
    handle: LogHandle,
}

impl LogGuard {
    /// El acceso al log para el backend.
    pub fn handle(&self) -> LogHandle {
        self.handle.clone()
    }
}

/// Las últimas [`RING_CAPACITY`] líneas del log, en orden de llegada. Al llenarse descarta las
/// más viejas.
pub struct LogRing {
    capacity: usize,
    lines: Mutex<VecDeque<LogLine>>,
}

impl LogRing {
    /// Un anillo vacío que guarda hasta `capacity` líneas.
    pub fn new(capacity: usize) -> Arc<Self> {
        Arc::new(Self {
            capacity: capacity.max(1),
            lines: Mutex::new(VecDeque::new()),
        })
    }

    /// Agrega una línea; si el anillo está lleno, sale la más vieja.
    pub fn push(&self, line: LogLine) {
        let mut lines = self.lines.lock().unwrap_or_else(PoisonError::into_inner);
        if lines.len() == self.capacity {
            lines.pop_front();
        }
        lines.push_back(line);
    }

    /// Las últimas `limit` líneas, las más nuevas al final.
    pub fn last(&self, limit: usize) -> Vec<LogLine> {
        let lines = self.lines.lock().unwrap_or_else(PoisonError::into_inner);
        lines
            .iter()
            .skip(lines.len().saturating_sub(limit))
            .cloned()
            .collect()
    }
}

/// Capa de `tracing` que copia cada evento al [`LogRing`].
pub struct RingLayer(pub Arc<LogRing>);

impl<S: Subscriber> Layer<S> for RingLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut fields = Fields::default();
        event.record(&mut fields);
        let meta = event.metadata();
        self.0.push(LogLine {
            at_ms: now_ms(),
            level: level_of(*meta.level()),
            target: meta.target().to_owned(),
            message: fields.into_message(),
        });
    }
}

fn level_of(level: Level) -> LogLevel {
    match level {
        Level::ERROR => LogLevel::Error,
        Level::WARN => LogLevel::Warn,
        Level::INFO => LogLevel::Info,
        // El contrato no tiene `trace`: para el visor es lo mismo que `debug`.
        Level::DEBUG | Level::TRACE => LogLevel::Debug,
    }
}

fn now_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64() * 1000.0)
}

/// El mensaje de un evento más sus campos (`mensaje clave=valor`).
#[derive(Default)]
struct Fields {
    message: String,
    rest: String,
}

impl Fields {
    fn into_message(self) -> String {
        if self.message.is_empty() {
            self.rest.trim_start().to_owned()
        } else {
            format!("{}{}", self.message, self.rest)
        }
    }
}

impl Visit for Fields {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            value.clone_into(&mut self.message);
        } else {
            let _ = write!(self.rest, " {}={value}", field.name());
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message.clear();
            let _ = write!(self.message, "{value:?}");
        } else {
            let _ = write!(self.rest, " {}={value:?}", field.name());
        }
    }
}

/// Instala el subscriber global. Si no se puede abrir el archivo, la app sigue sin él (y en
/// desarrollo con stderr): un log roto nunca impide arrancar.
pub fn init(dir: &Path) -> LogGuard {
    let ring = LogRing::new(RING_CAPACITY);
    let (file, guard) = match open_file(dir) {
        Ok(file) => {
            let (writer, guard) = tracing_appender::non_blocking(file);
            let layer = tracing_subscriber::fmt::layer()
                .with_writer(writer)
                .with_ansi(false);
            (Some(layer), Some(guard))
        }
        Err(error) => {
            eprintln!("no se pudo abrir el log en {}: {error}", dir.display());
            (None, None)
        }
    };
    let stderr = cfg!(debug_assertions).then(|| {
        tracing_subscriber::fmt::layer()
            .with_writer(std::io::stderr)
            .with_ansi(true)
    });
    // Si ya había un subscriber (pruebas), se deja el que estaba
    let _ = tracing_subscriber::registry()
        .with(env_filter())
        .with(file)
        .with(stderr)
        .with(RingLayer(ring.clone()))
        .try_init();
    install_panic_hook();
    tracing::info!(dir = %dir.display(), "log iniciado");
    LogGuard {
        _file: guard,
        handle: LogHandle {
            ring,
            dir: dir.to_owned(),
        },
    }
}

fn open_file(dir: &Path) -> Result<RollingFileAppender, Box<dyn std::error::Error>> {
    std::fs::create_dir_all(dir)?;
    Ok(RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(FILE_PREFIX)
        .filename_suffix(FILE_SUFFIX)
        .max_log_files(KEEP_DAYS)
        .build(dir)?)
}

/// Un pánico queda en el log antes de que el proceso siga su curso.
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!(%info, "pánico");
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capture(f: impl FnOnce()) -> Vec<LogLine> {
        let ring = LogRing::new(RING_CAPACITY);
        let subscriber = tracing_subscriber::registry().with(RingLayer(ring.clone()));
        tracing::subscriber::with_default(subscriber, f);
        ring.last(usize::MAX)
    }

    fn line(n: usize) -> LogLine {
        LogLine {
            at_ms: n as f64,
            level: LogLevel::Info,
            target: "t".into(),
            message: n.to_string(),
        }
    }

    #[test]
    fn el_anillo_devuelve_en_orden_y_las_ultimas_n() {
        let ring = LogRing::new(10);
        for n in 0..5 {
            ring.push(line(n));
        }
        let all: Vec<_> = ring.last(100).iter().map(|l| l.message.clone()).collect();
        assert_eq!(all, ["0", "1", "2", "3", "4"]);
        let last2: Vec<_> = ring.last(2).iter().map(|l| l.message.clone()).collect();
        assert_eq!(last2, ["3", "4"]);
        assert!(ring.last(0).is_empty());
    }

    #[test]
    fn el_anillo_lleno_descarta_las_mas_viejas() {
        let ring = LogRing::new(3);
        for n in 0..7 {
            ring.push(line(n));
        }
        let kept: Vec<_> = ring.last(100).iter().map(|l| l.message.clone()).collect();
        assert_eq!(kept, ["4", "5", "6"]);
    }

    #[test]
    fn la_capa_guarda_nivel_modulo_mensaje_y_campos() {
        let lines = capture(|| {
            tracing::warn!(device = "abc", "se perdió la conexión");
            tracing::debug!(error = %"timeout", "no se pudo reconectar");
            tracing::info!("sin campos");
        });
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].level, LogLevel::Warn);
        assert_eq!(lines[0].message, "se perdió la conexión device=abc");
        assert!(lines[0].target.starts_with("openlight_lib::logging"));
        assert_eq!(lines[1].level, LogLevel::Debug);
        assert_eq!(lines[1].message, "no se pudo reconectar error=timeout");
        assert_eq!(lines[2].message, "sin campos");
        assert!(lines[0].at_ms > 0.0 && lines[0].at_ms <= lines[2].at_ms);
    }

    #[test]
    fn el_filtro_por_defecto_es_valido_y_abre_los_crates_propios() {
        let filter = EnvFilter::new(DEFAULT_FILTER).to_string();
        for target in [
            "tuya_device=debug",
            "openlight_core=debug",
            "openlight_lib=debug",
        ] {
            assert!(filter.contains(target), "falta {target} en {filter}");
        }
    }

    #[test]
    fn el_archivo_se_crea_en_una_carpeta_que_no_existia() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("a").join("logs");
        let mut file = open_file(&dir).unwrap();
        std::io::Write::write_all(&mut file, b"hola\n").unwrap();
        let name = std::fs::read_dir(&dir)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .file_name();
        let name = name.to_string_lossy();
        assert!(
            name.starts_with(FILE_PREFIX) && name.ends_with(FILE_SUFFIX),
            "{name}"
        );
    }
}
