//! Lista de monitores con CoreGraphics (no necesita el permiso de Grabación de pantalla).

use std::collections::HashMap;
use std::sync::{Mutex, mpsc};
use std::time::Duration;

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::NSScreen;
use objc2_core_graphics::{
    CGDirectDisplayID, CGDisplayCopyDisplayMode, CGDisplayIsAsleep, CGDisplayIsBuiltin,
    CGDisplayMode, CGDisplayPixelsHigh, CGDisplayPixelsWide, CGError, CGGetActiveDisplayList,
    CGMainDisplayID,
};
use objc2_foundation::{NSNumber, NSString};
use openlight_types::{DisplayId, DisplayInfo};

/// Más monitores que esto no se listan (el script usaba 16).
const MAX_DISPLAYS: usize = 32;

/// Cuánto se espera al hilo principal para leer los nombres de `NSScreen`.
const ESPERA_NOMBRES: Duration = Duration::from_millis(250);

/// Nombres ya leídos, para cuando el hilo principal está ocupado.
static NOMBRES: Mutex<Option<HashMap<u32, String>>> = Mutex::new(None);

/// IDs de los monitores activos (conectados, despiertos y dibujables), en el orden de
/// CoreGraphics: el principal primero. Vacío si no hay ninguno o si falla la consulta.
pub(crate) fn active_ids() -> Vec<CGDirectDisplayID> {
    let mut ids = [0 as CGDirectDisplayID; MAX_DISPLAYS];
    let mut count = 0u32;
    // SAFETY: `ids` tiene lugar para MAX_DISPLAYS elementos y `count` es un u32 válido; la
    // función escribe a lo sumo `max_displays` IDs y la cantidad.
    let err = unsafe { CGGetActiveDisplayList(MAX_DISPLAYS as u32, ids.as_mut_ptr(), &mut count) };
    if err != CGError::Success {
        return Vec::new();
    }
    ids[..(count as usize).min(MAX_DISPLAYS)].to_vec()
}

/// Si el monitor está para capturar: activo y no dormido.
pub(crate) fn is_available(id: CGDirectDisplayID) -> bool {
    active_ids().contains(&id) && !CGDisplayIsAsleep(id)
}

/// Monitores activos, en el orden de CoreGraphics.
pub fn active_displays() -> Vec<DisplayInfo> {
    let ids = active_ids();
    if ids.is_empty() {
        return Vec::new();
    }
    let nombres = screen_names(&ids);
    let principal = CGMainDisplayID();
    ids.iter()
        .map(|&id| {
            let (width, height) = pixel_size(id);
            DisplayInfo {
                id: DisplayId(id.to_string()),
                // Sin nombre (hilo principal ocupado), queda la resolución: no es un texto
                // traducible y la UI puede mostrarlo tal cual.
                name: nombres
                    .get(&id)
                    .cloned()
                    .unwrap_or_else(|| format!("{width}×{height}")),
                width,
                height,
                builtin: CGDisplayIsBuiltin(id),
                primary: id == principal,
            }
        })
        .collect()
}

/// Tamaño en píxeles reales (en Retina, el doble que en puntos).
fn pixel_size(id: CGDirectDisplayID) -> (u32, u32) {
    let modo = CGDisplayCopyDisplayMode(id);
    let (w, h) = match modo.as_deref() {
        Some(m) => (
            CGDisplayMode::pixel_width(Some(m)),
            CGDisplayMode::pixel_height(Some(m)),
        ),
        None => (CGDisplayPixelsWide(id), CGDisplayPixelsHigh(id)),
    };
    (
        u32::try_from(w).unwrap_or(u32::MAX),
        u32::try_from(h).unwrap_or(u32::MAX),
    )
}

/// Nombres de los monitores (`NSScreen.localizedName`). AppKit solo deja leerlos desde el hilo
/// principal: si estamos ahí se leen directo; si no, se le pide al hilo principal y se espera
/// un poco. Si no contesta (por ejemplo, un programa de consola con el hilo principal
/// bloqueado), se usan los últimos nombres conocidos.
fn screen_names(ids: &[CGDirectDisplayID]) -> HashMap<u32, String> {
    let leidos = if let Some(mtm) = MainThreadMarker::new() {
        Some(read_names(mtm))
    } else {
        let (tx, rx) = mpsc::channel();
        dispatch2::DispatchQueue::main().exec_async(move || {
            if let Some(mtm) = MainThreadMarker::new() {
                let _ = tx.send(read_names(mtm));
            }
        });
        rx.recv_timeout(ESPERA_NOMBRES).ok()
    };
    let mut cache = NOMBRES.lock().unwrap_or_else(|e| e.into_inner());
    let cache = cache.get_or_insert_with(HashMap::new);
    if let Some(leidos) = leidos {
        cache.extend(leidos);
    }
    ids.iter()
        .filter_map(|id| cache.get(id).map(|n| (*id, n.clone())))
        .collect()
}

fn read_names(mtm: MainThreadMarker) -> HashMap<u32, String> {
    let clave = NSString::from_str("NSScreenNumber");
    NSScreen::screens(mtm)
        .iter()
        .filter_map(|screen| {
            let numero = screen.deviceDescription().objectForKey(&clave)?;
            let numero: Retained<NSNumber> = numero.downcast().ok()?;
            Some((
                numero.unsignedIntValue(),
                screen.localizedName().to_string(),
            ))
        })
        .collect()
}
