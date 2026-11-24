//! El log de la app: un subscriber de `tracing` que escribe a un archivo con rotación diaria
//! (y a stderr en desarrollo).
//!
//! Qué se registra lo decide `RUST_LOG`; sin él, `info` en general y `debug` para los crates
//! propios que explican por qué un foco no responde. **Los secretos no entran al log**: los
//! `Secret` no implementan `Display` ni `Serialize`, y ningún `tracing::*!` recibe claves, el
//! Access Secret ni tokens (hay una prueba en `tests/diagnostico.rs` que lo comprueba).

use std::path::{Path, PathBuf};

use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// Filtro cuando no hay `RUST_LOG`.
pub const DEFAULT_FILTER: &str = "info,tuya_device=debug,openlight_core=debug,openlight_lib=debug";

/// Cuántos archivos diarios se guardan.
pub const KEEP_DAYS: usize = 7;

/// Prefijo de los archivos: `openlight.2026-11-24.log`.
pub const FILE_PREFIX: &str = "openlight";
/// Sufijo de los archivos de log.
pub const FILE_SUFFIX: &str = "log";

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

/// Mantiene vivo el hilo que escribe el archivo: al soltarlo se vacía el búfer.
pub struct LogGuard {
    _file: Option<WorkerGuard>,
}

/// Instala el subscriber global. Si no se puede abrir el archivo, la app sigue sin él (y en
/// desarrollo con stderr): un log roto nunca impide arrancar.
pub fn init(dir: &Path) -> LogGuard {
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
        .try_init();
    install_panic_hook();
    tracing::info!(dir = %dir.display(), "log iniciado");
    LogGuard { _file: guard }
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
