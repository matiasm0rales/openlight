//! Focos: agregar, actualizar, quitar, probar, control manual y renovar la clave.

use std::net::IpAddr;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use openlight_core::{DeviceGuard, DeviceRecord, Holder};
use openlight_types::ipc::{ConfiguredDevice, DeviceAddRequest, ManualSet, TestPattern};
use openlight_types::{
    Capabilities, DeviceError, DeviceId, Dps, ErrorCode, FixAction, HelpTopic, Hsv,
    ProtocolVersion, SendChannel, UserFacingError, Zone, ZonePreset,
};
use secrets::SecretKey;
use serde_json::{Value, json};
use tokio::sync::Mutex as AsyncMutex;
use tuya_device::CapabilitiesExt;
use tuya_device::colour::colour_write;

use crate::errors::{CmdResult, device_error, invalid, unknown_device};
use crate::state::{Backend, ManualSlot, resolved_capabilities};

/// Una conexión de control manual lista para escribir.
pub(crate) struct ManualConn {
    id: DeviceId,
    slot: Arc<ManualSlot>,
    caps: Capabilities,
    channel: SendChannel,
    /// Mientras se usa, ni la sesión ni la calibración pueden tomar el foco.
    _guard: DeviceGuard,
}

impl Backend {
    /// Focos configurados.
    pub fn devices_list(&self) -> CmdResult<Vec<ConfiguredDevice>> {
        Ok(self.records()?.iter().map(|r| self.configured(r)).collect())
    }

    /// Agrega un foco de la cuenta: su clave va al llavero y su mapeo define las capacidades.
    ///
    /// La versión del protocolo y la IP salen, en este orden, de lo que se vio de la red, de una
    /// búsqueda nueva y, al volver a agregar un foco, de lo que ya estaba guardado. Si nada sirve
    /// es un error: nunca se adivina una versión (un foco 3.5 guardado como 3.3 no responde).
    pub async fn device_add(&self, request: DeviceAddRequest) -> CmdResult<ConfiguredDevice> {
        let id = request.id.clone();
        let fixed_ip = parse_ip(request.ip.as_deref())?;
        let entry = self.cloud_entry(&id).await?;
        let mapping = match entry.mapping {
            Some(mapping) => mapping,
            None => self
                .cloud_client()?
                .mapping(&id)
                .await
                .map_err(|e| crate::cmd::cloud::cloud_error(&e))?,
        };
        let capabilities = Capabilities::from_mapping(&mapping)
            .filter(Capabilities::supports_colour)
            .ok_or_else(|| device_error(&id, &DeviceError::Unsupported("color".into())))?;

        let previous_record = self.records()?.into_iter().find(|r| r.id == id);
        let (version, found_ip) = self.resolve_link(&id, previous_record.as_ref()).await?;
        let ip = fixed_ip.map(|ip| ip.to_string()).or(found_ip);

        let _edit = self.edit_lock();
        let mut records = self.records()?;
        let previous = records.iter().position(|r| r.id == id);
        let name = match request.name.trim() {
            "" => entry.device.name.clone(),
            name => name.to_owned(),
        };
        let record = DeviceRecord {
            id: id.clone(),
            name,
            ip,
            auto_ip: fixed_ip.is_none(),
            version,
            mapping: Some(mapping),
            capabilities: Some(capabilities),
            // Un foco nuevo sigue todo el monitor, como el script; uno que se vuelve a agregar
            // conserva lo que tenía.
            zones: previous.map_or_else(|| self.default_zones(), |i| records[i].zones.clone()),
            overrides: previous.map_or_else(Default::default, |i| records[i].overrides.clone()),
        };
        self.secrets.set(
            &SecretKey::DeviceLocalKey(id.clone()),
            &entry.device.local_key,
        )?;
        match previous {
            Some(i) => records[i] = record.clone(),
            None => records.push(record.clone()),
        }
        self.store.save_devices(&records)?;
        self.release_manual(&id);
        self.notify_changed();
        Ok(self.configured(&record))
    }

    /// Versión del protocolo e IP de un foco: la caché de la red, una búsqueda, y si no apareció,
    /// lo guardado de antes. Un error de la búsqueda solo se informa si no hay de dónde más sacarlo.
    async fn resolve_link(
        &self,
        id: &DeviceId,
        previous: Option<&DeviceRecord>,
    ) -> CmdResult<(ProtocolVersion, Option<String>)> {
        let cached = self.cache().lan.get(id).cloned();
        if let Some(lan) = cached {
            return Ok((lan.version, Some(lan.ip)));
        }
        let scan = self.scan_lan(self.options.lookup_scan, Some(id)).await;
        let scan_error = match scan {
            Ok(found) => {
                if let Some(lan) = found.into_iter().find(|d| &d.id == id) {
                    return Ok((lan.version, Some(lan.ip)));
                }
                None
            }
            Err(error) => {
                tracing::warn!(device = %id, code = ?error.code, "falló la búsqueda del foco en la red");
                Some(error)
            }
        };
        if let Some(previous) = previous {
            return Ok((previous.version, previous.ip.clone()));
        }
        Err(match scan_error {
            Some(error) => lan_scan_failed(error),
            None => UserFacingError::new(ErrorCode::DeviceNotFoundOnLan)
                .with_param("device", id.0.clone())
                .with_action(FixAction::Retry)
                .with_action(FixAction::ShowHelp(HelpTopic::DeviceTroubleshooting)),
        })
    }

    /// Todo el monitor elegido por defecto (el primero externo, y si no hay, el integrado).
    /// Sin monitores a mano (permiso de pantalla), sin zonas hasta que se elijan.
    pub(crate) fn default_zones(&self) -> Vec<Zone> {
        let source = (self.sources)();
        let Ok(displays) = source.displays() else {
            return Vec::new();
        };
        capture::common::pick_display(None, &displays)
            .map(|display| vec![Zone::from_preset(display, ZonePreset::Full)])
            .unwrap_or_default()
    }

    /// Cambia el nombre o la IP (`None` = buscarla sola). Vale desde la próxima conexión.
    pub fn device_update(&self, request: DeviceAddRequest) -> CmdResult<ConfiguredDevice> {
        let fixed_ip = parse_ip(request.ip.as_deref())?;
        let _edit = self.edit_lock();
        let mut records = self.records()?;
        let record = records
            .iter_mut()
            .find(|r| r.id == request.id)
            .ok_or_else(|| unknown_device(&request.id))?;
        if !request.name.trim().is_empty() {
            record.name = request.name.trim().to_owned();
        }
        match fixed_ip {
            Some(ip) => {
                record.ip = Some(ip.to_string());
                record.auto_ip = false;
            }
            None => record.auto_ip = true,
        }
        let record = record.clone();
        self.store.save_devices(&records)?;
        self.release_manual(&record.id);
        self.notify_changed();
        Ok(self.configured(&record))
    }

    /// Quita un foco con su clave, su calibración y lo que se recordaba de él. Falla si la
    /// sesión o la calibración lo están usando.
    pub async fn device_remove(&self, device: DeviceId) -> CmdResult<()> {
        let _guard = self.locks.try_acquire(&device, Holder::Manual)?;
        let _edit = self.edit_lock();
        let mut records = self.records()?;
        if !records.iter().any(|r| r.id == device) {
            return Err(unknown_device(&device));
        }
        self.release_manual(&device);
        records.retain(|r| r.id != device);
        self.store.save_devices(&records)?;
        self.secrets
            .delete(&SecretKey::DeviceLocalKey(device.clone()))?;
        self.store.delete_calibration(&device)?;
        self.store.update_state(|state| {
            state.devices.remove(&device);
        })?;
        self.notify_changed();
        Ok(())
    }

    // ---------- control manual y pruebas ----------

    /// Conecta con un foco para el control manual (reusa la conexión si sigue abierta).
    pub(crate) async fn connect_manual(self: &Arc<Self>, id: &DeviceId) -> CmdResult<ManualConn> {
        let record = self.record(id)?;
        let guard = self.locks.try_acquire(id, Holder::Manual)?;
        let slot = {
            let mut map = self.manual.lock().unwrap_or_else(|e| e.into_inner());
            match map.get(id) {
                Some(slot) => slot.clone(),
                None => {
                    let slot = Arc::new(ManualSlot {
                        handle: self.spawn_device(&record)?,
                        link: AsyncMutex::new(None),
                        last_used: std::sync::Mutex::new(tokio::time::Instant::now()),
                        reaper: false.into(),
                    });
                    map.insert(id.clone(), slot.clone());
                    slot
                }
            }
        };
        let channel = self.settings_for(&record.overrides)?.send_channel;
        let mut link = slot.link.lock().await;
        let caps = match link.as_ref() {
            Some(caps) => caps.clone(),
            None => {
                let started =
                    tokio::time::timeout(self.options.connect_timeout, slot.handle.start()).await;
                let info = match started {
                    Ok(info) => info.map_err(|e| device_error(id, &e))?,
                    Err(_) => return Err(device_error(id, &DeviceError::NotFound)),
                };
                if record.capabilities.is_none() {
                    // Un foco importado sin mapeo: desde ahora se sabe lo que puede hacer
                    let _edit = self.edit_lock();
                    let mut records = self.records()?;
                    if let Some(r) = records.iter_mut().find(|r| &r.id == id) {
                        r.capabilities = Some(info.capabilities.clone());
                        self.store.save_devices(&records)?;
                    }
                }
                let caps = resolved_capabilities(&record).unwrap_or(info.capabilities);
                *link = Some(caps.clone());
                caps
            }
        };
        drop(link);
        Ok(ManualConn {
            id: id.clone(),
            slot,
            caps,
            channel,
            _guard: guard,
        })
    }

    /// Anota el uso y, si hace falta, deja una tarea esperando para soltar la conexión.
    fn finish_manual(self: &Arc<Self>, conn: ManualConn) {
        *conn
            .slot
            .last_used
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = tokio::time::Instant::now();
        if conn.slot.reaper.swap(true, Ordering::AcqRel) {
            return;
        }
        let backend = self.clone();
        // Débil: si alguien suelta la conexión antes (la sesión toma el foco), se cierra ya
        let (id, slot) = (conn.id.clone(), Arc::downgrade(&conn.slot));
        let idle = self.options.manual_idle;
        tokio::spawn(async move {
            loop {
                let Some(slot) = slot.upgrade() else {
                    return;
                };
                let used = *slot.last_used.lock().unwrap_or_else(|e| e.into_inner());
                let wait = idle.saturating_sub(used.elapsed());
                if wait.is_zero() {
                    backend.release_manual_slot(&id, &slot);
                    return;
                }
                drop(slot);
                tokio::time::sleep(wait).await;
            }
        });
    }

    /// Suelta la conexión de control manual de un foco, sea cual sea. El foco queda como lo
    /// dejó el usuario; por eso ya no hay un "original" que devolver.
    pub(crate) fn release_manual(&self, id: &DeviceId) {
        let slot = self
            .manual
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id);
        if slot.is_some() {
            let _ = self.store.clear_original(id);
        }
    }

    fn release_manual_slot(&self, id: &DeviceId, slot: &Arc<ManualSlot>) {
        let mut map = self.manual.lock().unwrap_or_else(|e| e.into_inner());
        if map
            .get(id)
            .is_some_and(|current| Arc::ptr_eq(current, slot))
        {
            map.remove(id);
            let _ = self.store.clear_original(id);
        }
    }

    /// Corre un patrón de prueba y devuelve el foco a como estaba.
    pub async fn device_test(
        self: &Arc<Self>,
        device: DeviceId,
        pattern: TestPattern,
    ) -> CmdResult<()> {
        let conn = self.connect_manual(&device).await?;
        let step = self.options.test_step;
        let result = run_pattern(&conn, pattern, step).await;
        // Pase lo que pase, el foco vuelve a su estado y se cierra la conexión
        let restored = conn.slot.handle.restore().await;
        *conn.slot.link.lock().await = None;
        self.finish_manual(conn);
        result.map_err(|e| device_error(&device, &e))?;
        restored.map_err(|e| device_error(&device, &e))
    }

    /// Control manual: encender, color, blanco o escena.
    pub async fn device_manual(
        self: &Arc<Self>,
        device: DeviceId,
        set: ManualSet,
    ) -> CmdResult<()> {
        let record = self.record(&device)?;
        let conn = self.connect_manual(&device).await?;
        let dps = match &set {
            ManualSet::Power(on) => power_dps(&conn.caps, *on),
            ManualSet::Colour(hsv) => colour_dps(&conn.caps, conn.channel, *hsv),
            ManualSet::White {
                brightness,
                temperature,
            } => white_dps(&conn.caps, *brightness, *temperature),
            ManualSet::Scene(value) => scene_dps(&conn.caps, &record, value),
        };
        let result = match dps {
            Ok(dps) => conn.slot.handle.set_dps(dps, false).await,
            Err(error) => Err(error),
        };
        self.finish_manual(conn);
        result.map_err(|e| device_error(&device, &e))
    }

    /// Descarga la clave local de nuevo con las credenciales guardadas.
    pub async fn renew_key(self: &Arc<Self>, device: DeviceId) -> CmdResult<()> {
        self.record(&device)?;
        // Siempre de la nube: lo que hay en memoria puede ser la clave vieja
        self.cache().cloud_devices.remove(&device);
        let entry = self.cloud_entry(&device).await?;
        self.secrets.set(
            &SecretKey::DeviceLocalKey(device.clone()),
            &entry.device.local_key,
        )?;
        self.release_manual(&device);
        // El actor de la sesión tiene la clave vieja: se reinicia con la nueva
        if self.session_uses(&device).await {
            self.session_stop().await?;
            self.session_start().await?;
        }
        Ok(())
    }
}

/// La búsqueda en la red no pudo ni escuchar: "buscar de nuevo" primero, y lo que arregla cada
/// causa (cerrar otros programas, el permiso de Red local o el firewall) después.
fn lan_scan_failed(cause: UserFacingError) -> UserFacingError {
    let mut error = UserFacingError::new(ErrorCode::LanScanFailed)
        .with_param("cause", cause.code.as_str())
        .with_action(FixAction::Retry);
    if let Some(detail) = cause.params.get("detail") {
        error = error.with_param("detail", detail.clone());
    }
    for action in cause.actions {
        if action != FixAction::Retry {
            error = error.with_action(action);
        }
    }
    if !error
        .actions
        .contains(&FixAction::ShowHelp(HelpTopic::DeviceTroubleshooting))
    {
        error = error.with_action(FixAction::ShowHelp(HelpTopic::DeviceTroubleshooting));
    }
    error
}

fn parse_ip(ip: Option<&str>) -> CmdResult<Option<IpAddr>> {
    match ip.map(str::trim).filter(|ip| !ip.is_empty()) {
        None => Ok(None),
        Some(text) => text
            .parse::<IpAddr>()
            .map(Some)
            .map_err(|_| invalid("ip", format!("{text} no es una IP"))),
    }
}

// ---------- los DPs de cada acción ----------

fn dps(items: impl IntoIterator<Item = (String, Value)>) -> Dps {
    items.into_iter().collect()
}

fn power_dps(caps: &Capabilities, on: bool) -> Result<Dps, DeviceError> {
    Ok(dps([(caps.dp_switch.clone(), json!(on))]))
}

/// Prende el foco, lo pone en modo color y le pide `hsv` por el canal elegido.
fn colour_dps(caps: &Capabilities, channel: SendChannel, hsv: Hsv) -> Result<Dps, DeviceError> {
    let write = colour_write(caps, channel, hsv)?;
    let mut out = dps([(caps.dp_switch.clone(), json!(true))]);
    if let Some(mode) = &caps.dp_mode {
        out.insert(mode.clone(), json!("colour"));
    }
    out.insert(write.dp, json!(write.value));
    Ok(out)
}

/// Blanco con brillo y temperatura en `0..1` llevados al rango de cada DP.
fn white_dps(caps: &Capabilities, brightness: f32, temperature: f32) -> Result<Dps, DeviceError> {
    let mut out = dps([(caps.dp_switch.clone(), json!(true))]);
    if let Some(mode) = &caps.dp_mode {
        out.insert(mode.clone(), json!("white"));
    }
    let scale = |range: &openlight_types::DpRange, unit: f32| {
        let unit = if unit.is_nan() {
            0.0
        } else {
            unit.clamp(0.0, 1.0)
        };
        let span = f64::from(range.max - range.min);
        let raw = f64::from(range.min) + (f64::from(unit) * span).round();
        raw.clamp(f64::from(range.min), f64::from(range.max)) as i64
    };
    if let Some(range) = &caps.dp_bright {
        out.insert(range.dp.clone(), json!(scale(range, brightness)));
    }
    if let Some(range) = &caps.dp_temp {
        out.insert(range.dp.clone(), json!(scale(range, temperature)));
    }
    Ok(out)
}

/// Una escena: el DP sale del mapeo de la nube (`scene_data` / `scene_data_v2`).
fn scene_dps(caps: &Capabilities, record: &DeviceRecord, value: &str) -> Result<Dps, DeviceError> {
    let dp = record
        .mapping
        .as_ref()
        .and_then(|m| {
            m.0.iter()
                .find(|(_, s)| s.code.starts_with("scene_data"))
                .map(|(dp, _)| dp.clone())
        })
        .ok_or_else(|| DeviceError::Unsupported("escenas".into()))?;
    let mut out = dps([(caps.dp_switch.clone(), json!(true))]);
    if let Some(mode) = &caps.dp_mode {
        out.insert(mode.clone(), json!("scene"));
    }
    out.insert(dp, json!(value));
    Ok(out)
}

/// Rojo, verde y azul (como `--probar`), o un parpadeo para saber cuál es cuál.
async fn run_pattern(
    conn: &ManualConn,
    pattern: TestPattern,
    step: Duration,
) -> Result<(), DeviceError> {
    let set = |dps: Result<Dps, DeviceError>| async { conn.slot.handle.set_dps(dps?, false).await };
    match pattern {
        TestPattern::RedGreenBlue => {
            for hue in [0.0, 1.0 / 3.0, 2.0 / 3.0] {
                set(colour_dps(
                    &conn.caps,
                    conn.channel,
                    Hsv::new(hue, 1.0, 1.0),
                ))
                .await?;
                tokio::time::sleep(step).await;
            }
        }
        TestPattern::Blink => {
            for _ in 0..3 {
                set(colour_dps(
                    &conn.caps,
                    conn.channel,
                    Hsv::new(0.0, 0.0, 1.0),
                ))
                .await?;
                tokio::time::sleep(step / 2).await;
                set(power_dps(&conn.caps, false)).await?;
                tokio::time::sleep(step / 2).await;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use openlight_types::{ColourFormat, DpRange};

    use super::*;

    fn caps() -> Capabilities {
        Capabilities {
            dp_switch: "20".into(),
            dp_mode: Some("21".into()),
            dp_bright: Some(DpRange {
                dp: "22".into(),
                min: 10,
                max: 1000,
                step: 1,
            }),
            dp_temp: Some(DpRange {
                dp: "23".into(),
                min: 0,
                max: 1000,
                step: 1,
            }),
            dp_colour: Some(("24".into(), ColourFormat::V2Hsv12)),
            dp_control: None,
            modes: vec![],
        }
    }

    #[test]
    fn el_blanco_lleva_brillo_y_temperatura_al_rango_de_cada_dp() {
        let out = white_dps(&caps(), 1.0, 0.5).unwrap();
        assert_eq!(out["21"], json!("white"));
        assert_eq!(out["22"], json!(1000));
        assert_eq!(out["23"], json!(500));
        let out = white_dps(&caps(), -3.0, f32::NAN).unwrap();
        assert_eq!(out["22"], json!(10), "nunca baja del mínimo del foco");
        assert_eq!(out["23"], json!(0));
    }

    #[test]
    fn el_color_prende_el_foco_y_lo_pone_en_modo_color() {
        let out = colour_dps(&caps(), SendChannel::Colour, Hsv::new(0.0, 1.0, 1.0)).unwrap();
        assert_eq!(out["20"], json!(true));
        assert_eq!(out["21"], json!("colour"));
        assert_eq!(out["24"], json!("000003e803e8"));
    }

    #[test]
    fn una_ip_mal_escrita_es_un_error_y_vacia_es_automatica() {
        assert_eq!(parse_ip(None).unwrap(), None);
        assert_eq!(parse_ip(Some("  ")).unwrap(), None);
        assert!(parse_ip(Some("192.168.1.20")).unwrap().is_some());
        assert!(parse_ip(Some("no-es-ip")).is_err());
    }
}
