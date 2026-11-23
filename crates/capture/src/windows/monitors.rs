//! Enumeración de monitores: GDI (`EnumDisplayMonitors`) para el `HMONITOR`, el nombre de
//! dispositivo y si es el principal; DisplayConfig para el nombre comercial, si es la pantalla
//! integrada, si tiene HDR ("color avanzado") activo y su nivel de blanco SDR.

use std::mem::size_of;

use ::windows::Win32::Devices::Display::{
    DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO, DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL,
    DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
    DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_DEVICE_INFO_TYPE,
    DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO, DISPLAYCONFIG_MODE_INFO,
    DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EMBEDDED, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL,
    DISPLAYCONFIG_OUTPUT_TECHNOLOGY_UDI_EMBEDDED, DISPLAYCONFIG_PATH_INFO,
    DISPLAYCONFIG_SDR_WHITE_LEVEL, DISPLAYCONFIG_SOURCE_DEVICE_NAME,
    DISPLAYCONFIG_TARGET_DEVICE_NAME, DISPLAYCONFIG_VIDEO_OUTPUT_TECHNOLOGY,
    DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QDC_ONLY_ACTIVE_PATHS,
    QueryDisplayConfig,
};
use ::windows::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS, LPARAM, LUID, RECT};
use ::windows::Win32::Graphics::Gdi::{
    DEVMODEW, ENUM_CURRENT_SETTINGS, EnumDisplayMonitors, EnumDisplaySettingsW, GetMonitorInfoW,
    HDC, HMONITOR, MONITORINFO, MONITORINFOEXW,
};
use ::windows::Win32::UI::WindowsAndMessaging::MONITORINFOF_PRIMARY;
use ::windows::core::{BOOL, PCWSTR};
use openlight_types::{CaptureError, DisplayId, DisplayInfo};

/// Un monitor con lo que hace falta para capturarlo.
#[derive(Debug, Clone)]
pub(crate) struct Monitor {
    pub info: DisplayInfo,
    pub hmonitor: HMONITOR,
    /// "Color avanzado" activo (HDR, o WCG/ACM): el escritorio se compone en scRGB.
    pub advanced_color: bool,
    /// Blanco SDR en unidades de scRGB (`1.0` = 80 nits), para convertir a SDR.
    pub sdr_white_scale: f32,
}

/// Datos de DisplayConfig de un monitor activo, indexados por su nombre GDI (`\\.\DISPLAY1`).
struct TargetInfo {
    gdi_name: String,
    friendly_name: String,
    builtin: bool,
    advanced_color: bool,
    sdr_white_scale: f32,
}

/// Monitores conectados y encendidos ahora, en el orden de GDI.
pub(crate) fn enumerate() -> Result<Vec<Monitor>, CaptureError> {
    let mut handles: Vec<HMONITOR> = Vec::new();
    // SAFETY: el callback solo empuja al `Vec` que le pasamos por `LPARAM`, que vive durante
    // toda la llamada (EnumDisplayMonitors es sincrónica).
    let ok = unsafe {
        EnumDisplayMonitors(
            None,
            None,
            Some(collect_monitor),
            LPARAM(&mut handles as *mut Vec<HMONITOR> as isize),
        )
    };
    if !ok.as_bool() {
        return Err(CaptureError::Backend(format!(
            "EnumDisplayMonitors falló: {}",
            ::windows::core::Error::from_thread()
        )));
    }
    // Si DisplayConfig falla (pasa en sesiones remotas), igual se listan los monitores de GDI.
    let targets = display_config_targets().unwrap_or_default();
    Ok(handles
        .into_iter()
        .filter_map(|h| describe(h, &targets))
        .collect())
}

/// Busca un monitor por id.
pub(crate) fn find(id: &DisplayId) -> Result<Option<Monitor>, CaptureError> {
    Ok(enumerate()?.into_iter().find(|m| &m.info.id == id))
}

unsafe extern "system" fn collect_monitor(
    hmonitor: HMONITOR,
    _hdc: HDC,
    _rect: *mut RECT,
    data: LPARAM,
) -> BOOL {
    // SAFETY: `data` es el `&mut Vec<HMONITOR>` que pasó `enumerate`.
    let handles = unsafe { &mut *(data.0 as *mut Vec<HMONITOR>) };
    handles.push(hmonitor);
    true.into()
}

fn describe(hmonitor: HMONITOR, targets: &[TargetInfo]) -> Option<Monitor> {
    let mut mi = MONITORINFOEXW {
        monitorInfo: MONITORINFO {
            cbSize: size_of::<MONITORINFOEXW>() as u32,
            ..Default::default()
        },
        ..Default::default()
    };
    // SAFETY: `mi` es un MONITORINFOEXW con `cbSize` correcto; la API lo acepta como MONITORINFO.
    let ok = unsafe { GetMonitorInfoW(hmonitor, &mut mi.monitorInfo) };
    if !ok.as_bool() {
        return None; // se desconectó entre la enumeración y la consulta
    }
    let gdi_name = from_wide(&mi.szDevice);
    let primary = mi.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0;

    // Tamaño real del modo de video. `rcMonitor` está escalado si el proceso no es DPI-aware.
    let mut dm = DEVMODEW {
        dmSize: size_of::<DEVMODEW>() as u16,
        ..Default::default()
    };
    // SAFETY: `szDevice` termina en NUL y `dm` tiene `dmSize` correcto.
    let has_mode = unsafe {
        EnumDisplaySettingsW(PCWSTR(mi.szDevice.as_ptr()), ENUM_CURRENT_SETTINGS, &mut dm)
    }
    .as_bool();
    let rc = mi.monitorInfo.rcMonitor;
    let (width, height) = if has_mode && dm.dmPelsWidth > 0 && dm.dmPelsHeight > 0 {
        (dm.dmPelsWidth, dm.dmPelsHeight)
    } else {
        ((rc.right - rc.left) as u32, (rc.bottom - rc.top) as u32)
    };

    let target = targets.iter().find(|t| t.gdi_name == gdi_name);
    let name = match target {
        Some(t) if !t.friendly_name.is_empty() => t.friendly_name.clone(),
        _ => gdi_name.trim_start_matches(r"\\.\").to_owned(),
    };
    Some(Monitor {
        info: DisplayInfo {
            id: DisplayId(gdi_name),
            name,
            width,
            height,
            builtin: target.is_some_and(|t| t.builtin),
            primary,
        },
        hmonitor,
        advanced_color: target.is_some_and(|t| t.advanced_color),
        sdr_white_scale: target.map_or(1.0, |t| t.sdr_white_scale),
    })
}

/// Si la tecnología de salida corresponde a una pantalla integrada (notebook, tablet).
pub(crate) fn is_builtin(tech: DISPLAYCONFIG_VIDEO_OUTPUT_TECHNOLOGY) -> bool {
    tech == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL
        || tech == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EMBEDDED
        || tech == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_UDI_EMBEDDED
}

/// `SDRWhiteLevel` viene en milésimas de 80 nits (1000 = 80 nits = 1.0 en scRGB).
pub(crate) fn sdr_white_scale(level: u32) -> f32 {
    if level == 0 { 1.0 } else { level as f32 / 1000.0 }
}

fn display_config_targets() -> Option<Vec<TargetInfo>> {
    let mut paths: Vec<DISPLAYCONFIG_PATH_INFO>;
    let mut modes: Vec<DISPLAYCONFIG_MODE_INFO>;
    // La configuración puede cambiar entre las dos llamadas: se reintenta si el búfer no alcanza.
    loop {
        let (mut np, mut nm) = (0u32, 0u32);
        // SAFETY: punteros a dos u32 locales.
        let err = unsafe { GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut np, &mut nm) };
        if err != ERROR_SUCCESS {
            return None;
        }
        paths = vec![DISPLAYCONFIG_PATH_INFO::default(); np as usize];
        modes = vec![DISPLAYCONFIG_MODE_INFO::default(); nm as usize];
        // SAFETY: los búferes tienen exactamente `np` y `nm` elementos.
        let err = unsafe {
            QueryDisplayConfig(
                QDC_ONLY_ACTIVE_PATHS,
                &mut np,
                paths.as_mut_ptr(),
                &mut nm,
                modes.as_mut_ptr(),
                None,
            )
        };
        if err == ERROR_INSUFFICIENT_BUFFER {
            continue;
        }
        if err != ERROR_SUCCESS {
            return None;
        }
        paths.truncate(np as usize);
        break;
    }

    let mut out = Vec::with_capacity(paths.len());
    for path in &paths {
        let src = &path.sourceInfo;
        let tgt = &path.targetInfo;

        let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
            header: header::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>(
                DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                src.adapterId,
                src.id,
            ),
            ..Default::default()
        };
        if !get_device_info(&mut source) {
            continue;
        }

        let mut target = DISPLAYCONFIG_TARGET_DEVICE_NAME {
            header: header::<DISPLAYCONFIG_TARGET_DEVICE_NAME>(
                DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
                tgt.adapterId,
                tgt.id,
            ),
            ..Default::default()
        };
        let friendly_name = if get_device_info(&mut target) {
            from_wide(&target.monitorFriendlyDeviceName)
        } else {
            String::new()
        };

        let mut color = DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO {
            header: header::<DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO>(
                DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO,
                tgt.adapterId,
                tgt.id,
            ),
            ..Default::default()
        };
        // Bits: 0 = soportado, 1 = activo, 2 = gama amplia forzada, 3 = desactivado a la fuerza.
        // SAFETY: lectura de la unión como u32 (todas sus variantes son ese mismo u32).
        let advanced_color =
            get_device_info(&mut color) && unsafe { color.Anonymous.value } & 0b10 != 0;

        let mut white = DISPLAYCONFIG_SDR_WHITE_LEVEL {
            header: header::<DISPLAYCONFIG_SDR_WHITE_LEVEL>(
                DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL,
                tgt.adapterId,
                tgt.id,
            ),
            ..Default::default()
        };
        let sdr_white_scale = if get_device_info(&mut white) {
            sdr_white_scale(white.SDRWhiteLevel)
        } else {
            1.0
        };

        out.push(TargetInfo {
            gdi_name: from_wide(&source.viewGdiDeviceName),
            friendly_name,
            builtin: is_builtin(tgt.outputTechnology),
            advanced_color,
            sdr_white_scale,
        });
    }
    Some(out)
}

fn header<T>(
    kind: DISPLAYCONFIG_DEVICE_INFO_TYPE,
    adapter: LUID,
    id: u32,
) -> DISPLAYCONFIG_DEVICE_INFO_HEADER {
    DISPLAYCONFIG_DEVICE_INFO_HEADER {
        r#type: kind,
        size: size_of::<T>() as u32,
        adapterId: adapter,
        id,
    }
}

/// `packet` es una estructura de DisplayConfig cuyo primer campo es un header armado con
/// `header::<T>` (con el tamaño de `T`).
fn get_device_info<T>(packet: &mut T) -> bool {
    // SAFETY: todas las estructuras de DisplayConfig son `#[repr(C)]` y empiezan con el header;
    // el puntero cubre la estructura entera, que es lo que la API escribe.
    unsafe {
        DisplayConfigGetDeviceInfo(packet as *mut T as *mut DISPLAYCONFIG_DEVICE_INFO_HEADER) == 0
    }
}

fn from_wide(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}
