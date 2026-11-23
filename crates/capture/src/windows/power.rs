//! Estado de la pantalla (encendida o apagada por ahorro de energía).
//!
//! En Windows, un monitor dormido sigue apareciendo en `EnumDisplayMonitors` y Graphics Capture
//! simplemente deja de mandar cuadros, igual que con una imagen quieta. Para distinguir "no hay
//! pantalla" de "no cambió nada", el hilo de captura crea una ventana de solo mensajes y se
//! suscribe a `GUID_CONSOLE_DISPLAY_STATE`; Windows le avisa enseguida el estado actual y después
//! cada cambio. El aviso vale para toda la consola (cuando Windows apaga las pantallas, las
//! apaga todas).

use std::cell::Cell;
use std::sync::Once;

use ::windows::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, WPARAM};
use ::windows::Win32::System::LibraryLoader::GetModuleHandleW;
use ::windows::Win32::System::Power::{
    HPOWERNOTIFY, POWERBROADCAST_SETTING, RegisterPowerSettingNotification,
    UnregisterPowerSettingNotification,
};
use ::windows::Win32::System::SystemServices::GUID_CONSOLE_DISPLAY_STATE;
use ::windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DEVICE_NOTIFY_WINDOW_HANDLE, DefWindowProcW, DestroyWindow, DispatchMessageW,
    HWND_MESSAGE, MSG, PBT_POWERSETTINGCHANGE, PM_REMOVE, PeekMessageW, RegisterClassExW,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_POWERBROADCAST, WNDCLASSEXW,
};
use ::windows::core::{PCWSTR, w};

const CLASS_NAME: PCWSTR = w!("OpenLightCaptureDisplayState");

thread_local! {
    /// Último estado avisado en este hilo: 0 apagada, 1 encendida, 2 atenuada.
    static DISPLAY_STATE: Cell<u32> = const { Cell::new(1) };
}

/// Si el valor de `GUID_CONSOLE_DISPLAY_STATE` significa que hay algo en pantalla.
/// Atenuada (2) cuenta como encendida: la imagen se sigue viendo.
pub(crate) fn display_state_is_on(state: u32) -> bool {
    state != 0
}

/// Ventana de solo mensajes suscripta al estado de la pantalla. Vive y se consulta en el hilo
/// que la crea.
pub(crate) struct DisplayPower {
    hwnd: HWND,
    notify: HPOWERNOTIFY,
}

impl DisplayPower {
    /// Crea la ventana y se suscribe. Si falla, el llamador asume pantalla siempre encendida.
    pub fn new() -> ::windows::core::Result<Self> {
        static REGISTER: Once = Once::new();
        // SAFETY: ventana de solo mensajes con un `WNDCLASSEXW` válido; el procedimiento de
        // ventana es una función `extern "system"` con la firma esperada.
        unsafe {
            let instance = GetModuleHandleW(None)?;
            REGISTER.call_once(|| {
                let class = WNDCLASSEXW {
                    cbSize: size_of::<WNDCLASSEXW>() as u32,
                    lpfnWndProc: Some(window_proc),
                    hInstance: instance.into(),
                    lpszClassName: CLASS_NAME,
                    ..Default::default()
                };
                // Si falla, CreateWindowExW falla abajo y se informa ahí.
                RegisterClassExW(&class);
            });
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                CLASS_NAME,
                PCWSTR::null(),
                WINDOW_STYLE(0),
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                Some(instance.into()),
                None,
            )?;
            match RegisterPowerSettingNotification(
                HANDLE(hwnd.0),
                &GUID_CONSOLE_DISPLAY_STATE,
                DEVICE_NOTIFY_WINDOW_HANDLE,
            ) {
                Ok(notify) => Ok(Self { hwnd, notify }),
                Err(e) => {
                    let _ = DestroyWindow(hwnd);
                    Err(e)
                }
            }
        }
    }

    /// Procesa los mensajes pendientes y dice si la pantalla está encendida.
    pub fn is_on(&self) -> bool {
        let mut msg = MSG::default();
        // SAFETY: bombeo de mensajes de la ventana propia de este hilo.
        unsafe {
            while PeekMessageW(&mut msg, Some(self.hwnd), 0, 0, PM_REMOVE).as_bool() {
                DispatchMessageW(&msg);
            }
        }
        display_state_is_on(DISPLAY_STATE.with(Cell::get))
    }
}

impl Drop for DisplayPower {
    fn drop(&mut self) {
        // SAFETY: handles creados en `new` en este mismo hilo y todavía no liberados.
        unsafe {
            let _ = UnregisterPowerSettingNotification(self.notify);
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_POWERBROADCAST && wparam.0 as u32 == PBT_POWERSETTINGCHANGE && lparam.0 != 0 {
        // SAFETY: con PBT_POWERSETTINGCHANGE, `lparam` apunta a un POWERBROADCAST_SETTING cuyo
        // `Data` tiene `DataLength` bytes; para GUID_CONSOLE_DISPLAY_STATE es un DWORD.
        // Se lee con punteros crudos derivados de `lparam`: `Data` está declarado como `[u8; 1]`
        // pero el DWORD ocupa 4 bytes.
        unsafe {
            let setting = lparam.0 as *const POWERBROADCAST_SETTING;
            if (*setting).PowerSetting == GUID_CONSOLE_DISPLAY_STATE && (*setting).DataLength >= 4 {
                let data = std::ptr::addr_of!((*setting).Data) as *const u32;
                DISPLAY_STATE.with(|s| s.set(std::ptr::read_unaligned(data)));
            }
        }
        return LRESULT(1);
    }
    // SAFETY: delegación estándar al procedimiento por defecto.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estado_de_pantalla() {
        assert!(!display_state_is_on(0));
        assert!(display_state_is_on(1));
        assert!(display_state_is_on(2));
    }
}
