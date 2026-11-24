//! Diagnóstico: el visor del log y el zip para pedir ayuda.
//!
//! El zip **no lleva secretos por construcción**: no se le pasa nada del llavero. Trae los
//! archivos de log, `config.json`, `devices.json` (que ya no tienen claves) y un resumen de
//! salud armado con datos que no son secretos. Además, `config.json` y `devices.json` pasan por
//! [`redact`], que tapa cualquier campo con nombre de secreto por si un día se cuela uno.

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use openlight_types::ipc::{LogLevel, LogLine};
use serde_json::{Value, json};
use zip::write::SimpleFileOptions;

use crate::errors::{CmdResult, internal};
use crate::logging::{FILE_PREFIX, FILE_SUFFIX, LogHandle, RING_CAPACITY};
use crate::state::Backend;

/// Cuántas líneas devuelve `diagnostics_log` si la UI no pide una cantidad.
const DEFAULT_LINES: usize = 500;

impl Backend {
    /// Conecta el log de la app (el anillo y la carpeta de archivos).
    pub fn set_logs(&self, logs: LogHandle) {
        *self
            .logs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(logs);
    }

    pub(crate) fn logs(&self) -> Option<LogHandle> {
        self.logs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Las últimas `limit` líneas del log (500 si no se pide una cantidad), las más nuevas al
    /// final. Sin log instalado (pruebas) no hay líneas.
    pub fn diagnostics_log(&self, limit: Option<u32>) -> Vec<LogLine> {
        let limit = limit
            .map_or(DEFAULT_LINES, |n| n as usize)
            .min(RING_CAPACITY);
        self.logs()
            .map(|logs| logs.ring.last(limit))
            .unwrap_or_default()
    }
}

impl Backend {
    /// Arma el zip de diagnóstico y devuelve su ruta.
    ///
    /// `destination` puede ser un archivo `.zip` o una carpeta; sin él, el zip va a la carpeta de
    /// exportación del log (descargas del usuario en la app real).
    pub async fn diagnostics_export(&self, destination: Option<String>) -> CmdResult<String> {
        let logs = self.logs();
        let target = export_path(
            destination.as_deref(),
            logs.as_ref().map(|l| l.export_dir.as_path()),
            self.data_dir(),
        );
        let data_dir = self.data_dir().to_owned();
        let health = self.health_summary();
        let recent = logs
            .as_ref()
            .map(|l| format_lines(&l.ring.last(RING_CAPACITY)))
            .unwrap_or_default();
        let log_dir = logs.map(|l| l.dir);
        let path = target.clone();
        tokio::task::spawn_blocking(move || {
            write_zip(&path, &data_dir, log_dir.as_deref(), &health, &recent)
        })
        .await
        .map_err(|e| internal(e.to_string()))?
        .map_err(|e| internal(format!("no se pudo escribir {}: {e}", target.display())))?;
        tracing::info!(path = %target.display(), "diagnóstico exportado");
        Ok(target.to_string_lossy().into_owned())
    }

    /// Resumen de salud: versión, sistema, estado de la sesión y los focos (sin claves ni
    /// credenciales: no se consulta el llavero).
    fn health_summary(&self) -> Value {
        let devices: Vec<Value> = self
            .records()
            .unwrap_or_default()
            .iter()
            .map(|d| {
                json!({
                    "id": d.id.0,
                    "name": d.name,
                    "version": d.version,
                    "ip": d.ip,
                    "auto_ip": d.auto_ip,
                    "zones": d.zones.len(),
                    "calibrated": matches!(self.store.load_calibration(&d.id), Ok(Some(_))),
                })
            })
            .collect();
        json!({
            "app_version": env!("CARGO_PKG_VERSION"),
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "generated_at": utc_stamp(now_secs()),
            "session_state": *self
                .session_state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            "devices": devices,
        })
    }
}

/// Dónde se escribe el zip.
fn export_path(destination: Option<&str>, default_dir: Option<&Path>, data_dir: &Path) -> PathBuf {
    let name = format!(
        "openlight-diagnostico-{}.zip",
        utc_stamp(now_secs()).replace([':', '-'], "")
    );
    match destination.map(str::trim).filter(|d| !d.is_empty()) {
        Some(d) if d.to_ascii_lowercase().ends_with(".zip") => PathBuf::from(d),
        Some(dir) => Path::new(dir).join(name),
        None => default_dir.unwrap_or(data_dir).join(name),
    }
}

fn write_zip(
    path: &Path,
    data_dir: &Path,
    log_dir: Option<&Path>,
    health: &Value,
    recent: &str,
) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path)?);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut add = |name: &str, bytes: &[u8]| -> std::io::Result<()> {
        zip.start_file(name, options)
            .map_err(std::io::Error::other)?;
        zip.write_all(bytes)
    };

    add("health.json", &serde_json::to_vec_pretty(health)?)?;
    for name in ["config.json", "devices.json"] {
        // Si el archivo no existe todavía (app recién instalada), no hay nada que agregar
        let Ok(text) = std::fs::read_to_string(data_dir.join(name)) else {
            continue;
        };
        let bytes = match serde_json::from_str::<Value>(&text) {
            Ok(mut value) => {
                redact(&mut value);
                serde_json::to_vec_pretty(&value)?
            }
            // Ilegible: mejor no incluirlo que mandarlo sin revisar
            Err(_) => b"\"[ilegible: no se incluye]\"".to_vec(),
        };
        add(name, &bytes)?;
    }
    add("logs/recientes.txt", recent.as_bytes())?;
    if let Some(dir) = log_dir {
        let mut files: Vec<_> = std::fs::read_dir(dir)?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| is_log_file(p))
            .collect();
        files.sort();
        for file in files {
            let Some(name) = file.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            add(&format!("logs/{name}"), &std::fs::read(&file)?)?;
        }
    }
    zip.finish().map_err(std::io::Error::other)?;
    Ok(())
}

fn is_log_file(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    name.starts_with(FILE_PREFIX) && name.ends_with(FILE_SUFFIX)
}

/// Tapa los campos cuyo nombre es de un secreto (`local_key`, `access_secret`, `token`...).
pub(crate) fn redact(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (name, field) in map.iter_mut() {
                if is_secret_name(name) {
                    *field = Value::String("[tapado]".into());
                } else {
                    redact(field);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(redact),
        _ => {}
    }
}

fn is_secret_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.ends_with("key")
        || ["secret", "token", "password", "passwd"]
            .iter()
            .any(|word| n.contains(word))
}

/// Las líneas en texto plano, una por línea.
fn format_lines(lines: &[LogLine]) -> String {
    let mut out = String::new();
    for l in lines {
        let level = match l.level {
            LogLevel::Error => "ERROR",
            LogLevel::Warn => "WARN",
            LogLevel::Info => "INFO",
            LogLevel::Debug => "DEBUG",
        };
        let _ = writeln!(
            out,
            "{} {level:5} {}: {}",
            utc_stamp((l.at_ms / 1000.0) as u64),
            l.target,
            l.message
        );
    }
    out
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// `2026-11-24T18:30:05Z` (UTC) a partir de segundos Unix.
fn utc_stamp(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rest = secs % 86_400;
    // Fecha civil desde días (algoritmo de Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_fecha_utc_sale_bien() {
        assert_eq!(utc_stamp(0), "1970-01-01T00:00:00Z");
        assert_eq!(utc_stamp(1_795_545_005), "2026-11-24T18:30:05Z");
        assert_eq!(utc_stamp(951_782_400), "2000-02-29T00:00:00Z");
    }

    #[test]
    fn redact_tapa_los_campos_con_nombre_de_secreto_en_cualquier_nivel() {
        let mut v = json!({
            "devices": [{"id": "a", "local_key": "SECRETA", "mapping": {"dp": 20}}],
            "cloud": {"access_secret": "S2", "access_id": "ok", "token": "T"},
            "global": {"capture_fps": 20}
        });
        redact(&mut v);
        let text = v.to_string();
        for leaked in ["SECRETA", "S2", "\"T\""] {
            assert!(!text.contains(leaked), "{leaked} quedó en {text}");
        }
        assert_eq!(v["devices"][0]["id"], "a");
        assert_eq!(v["global"]["capture_fps"], 20);
        assert_eq!(v["cloud"]["access_id"], "ok");
    }

    #[test]
    fn el_destino_acepta_un_zip_una_carpeta_o_nada() {
        let data = Path::new("/datos");
        let default = Path::new("/descargas");
        let zip = export_path(Some("/tmp/x/mi.ZIP"), Some(default), data);
        assert_eq!(zip, Path::new("/tmp/x/mi.ZIP"));
        let in_dir = export_path(Some("/tmp/x"), Some(default), data);
        assert!(in_dir.starts_with("/tmp/x") && in_dir.extension().is_some_and(|e| e == "zip"));
        let by_default = export_path(None, Some(default), data);
        assert!(by_default.starts_with("/descargas"));
        let no_log = export_path(Some("  "), None, data);
        assert!(no_log.starts_with("/datos"));
    }
}
