//! Monitores de Wayland: `wl_output` (nombre, modo en píxeles) y `xdg-output` (posición y
//! tamaño lógicos, que son los que informa el portal para cada stream).
//!
//! El portal ScreenCast no lista monitores: elige el usuario en el diálogo. Esta lista sirve para
//! `displays()`, para saber si un monitor volvió antes de reabrir la sesión y para avisar si el
//! usuario eligió otro monitor que el pedido.

use openlight_types::{CaptureError, DisplayId, DisplayInfo};
use wayland_client::protocol::{wl_output, wl_registry};
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum, delegate_noop};
use wayland_protocols::xdg::xdg_output::zv1::client::{zxdg_output_manager_v1, zxdg_output_v1};

use crate::linux::detect::looks_builtin;

/// Lo que se sabe de una salida.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct OutputInfo {
    /// Nombre del global en el registro (cambia entre sesiones).
    pub global: u32,
    /// Nombre del conector (`DP-1`), de `wl_output` v4 o `xdg_output` v2.
    pub name: Option<String>,
    /// Descripción legible (marca y modelo).
    pub description: Option<String>,
    /// Marca y modelo de `wl_output.geometry`.
    pub make_model: Option<String>,
    /// Modo actual en píxeles.
    pub pixels: Option<(i32, i32)>,
    /// Posición en el espacio del compositor (`wl_output.geometry`).
    pub position: Option<(i32, i32)>,
    /// Posición lógica (`xdg_output`).
    pub logical_position: Option<(i32, i32)>,
    /// Tamaño lógico (`xdg_output`).
    pub logical_size: Option<(i32, i32)>,
}

impl OutputInfo {
    /// Identificador estable: el nombre del conector, o el global si el compositor no lo da.
    pub(crate) fn id(&self) -> DisplayId {
        DisplayId(
            self.name
                .clone()
                .unwrap_or_else(|| format!("wl-output-{}", self.global)),
        )
    }

    /// Posición a comparar con la que informa el portal.
    pub(crate) fn origin(&self) -> Option<(i32, i32)> {
        self.logical_position.or(self.position)
    }
}

/// Pasa la lista de salidas al contrato. El monitor primario es el que está en (0, 0), o el
/// primero si ninguno está ahí (Wayland no tiene la idea de "primario").
pub(crate) fn to_display_infos(outputs: &[OutputInfo]) -> Vec<DisplayInfo> {
    let primary = outputs
        .iter()
        .position(|o| o.origin() == Some((0, 0)))
        .unwrap_or(0);
    outputs
        .iter()
        .enumerate()
        .map(|(i, o)| {
            let id = o.id();
            let (w, h) = o.pixels.or(o.logical_size).unwrap_or((0, 0));
            DisplayInfo {
                name: o
                    .description
                    .clone()
                    .or_else(|| o.make_model.clone())
                    .unwrap_or_else(|| id.0.clone()),
                builtin: o.name.as_deref().is_some_and(looks_builtin),
                id,
                width: w.max(0) as u32,
                height: h.max(0) as u32,
                primary: i == primary,
            }
        })
        .collect()
}

#[derive(Default)]
struct State {
    outputs: Vec<(wl_output::WlOutput, OutputInfo)>,
    manager: Option<zxdg_output_manager_v1::ZxdgOutputManagerV1>,
}

impl State {
    fn info(&mut self, global: u32) -> Option<&mut OutputInfo> {
        self.outputs
            .iter_mut()
            .find(|(_, o)| o.global == global)
            .map(|(_, o)| o)
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "wl_output" => {
                    let output =
                        registry.bind::<wl_output::WlOutput, _, _>(name, version.min(4), qh, name);
                    state.outputs.push((
                        output,
                        OutputInfo {
                            global: name,
                            ..OutputInfo::default()
                        },
                    ));
                }
                "zxdg_output_manager_v1" => {
                    state.manager = Some(registry.bind(name, version.min(3), qh, ()));
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<wl_output::WlOutput, u32> for State {
    fn event(
        state: &mut Self,
        _: &wl_output::WlOutput,
        event: wl_output::Event,
        global: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(info) = state.info(*global) else {
            return;
        };
        match event {
            wl_output::Event::Geometry {
                x, y, make, model, ..
            } => {
                info.position = Some((x, y));
                let make_model = format!("{make} {model}").trim().to_string();
                if !make_model.is_empty() {
                    info.make_model = Some(make_model);
                }
            }
            wl_output::Event::Mode {
                flags: WEnum::Value(flags),
                width,
                height,
                ..
            } if flags.contains(wl_output::Mode::Current) => {
                info.pixels = Some((width, height));
            }
            wl_output::Event::Name { name } => info.name = Some(name),
            wl_output::Event::Description { description } => {
                info.description = Some(description);
            }
            _ => {}
        }
    }
}

impl Dispatch<zxdg_output_v1::ZxdgOutputV1, u32> for State {
    fn event(
        state: &mut Self,
        _: &zxdg_output_v1::ZxdgOutputV1,
        event: zxdg_output_v1::Event,
        global: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(info) = state.info(*global) else {
            return;
        };
        match event {
            zxdg_output_v1::Event::LogicalPosition { x, y } => {
                info.logical_position = Some((x, y));
            }
            zxdg_output_v1::Event::LogicalSize { width, height } => {
                info.logical_size = Some((width, height));
            }
            zxdg_output_v1::Event::Name { name } => {
                info.name.get_or_insert(name);
            }
            zxdg_output_v1::Event::Description { description } => {
                info.description.get_or_insert(description);
            }
            _ => {}
        }
    }
}

delegate_noop!(State: ignore zxdg_output_manager_v1::ZxdgOutputManagerV1);

/// Lista las salidas del compositor (conexión corta: abre, pregunta y cierra).
pub(crate) fn list_outputs() -> Result<Vec<OutputInfo>, CaptureError> {
    let err = |what: &str, e: &dyn std::fmt::Display| {
        CaptureError::Backend(format!("Wayland, {what}: {e}"))
    };
    let conn = Connection::connect_to_env().map_err(|e| {
        CaptureError::Unsupported(format!("no se pudo conectar al compositor Wayland: {e}"))
    })?;
    let mut queue = conn.new_event_queue::<State>();
    let qh = queue.handle();
    let _registry = conn.display().get_registry(&qh, ());
    let mut state = State::default();
    // 1: globales; 2: eventos de cada wl_output.
    for _ in 0..2 {
        queue
            .roundtrip(&mut state)
            .map_err(|e| err("roundtrip", &e))?;
    }
    if let Some(manager) = state.manager.clone() {
        for (output, info) in &state.outputs {
            let _ = manager.get_xdg_output(output, &qh, info.global);
        }
        queue
            .roundtrip(&mut state)
            .map_err(|e| err("xdg-output", &e))?;
    }
    Ok(state.outputs.into_iter().map(|(_, info)| info).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output(global: u32, name: Option<&str>, origin: (i32, i32)) -> OutputInfo {
        OutputInfo {
            global,
            name: name.map(Into::into),
            pixels: Some((2560, 1440)),
            logical_position: Some(origin),
            logical_size: Some((1280, 720)),
            ..OutputInfo::default()
        }
    }

    #[test]
    fn arma_los_display_info() {
        let mut laptop = output(40, Some("eDP-1"), (2560, 0));
        laptop.description = Some("Pantalla integrada".into());
        let externo = output(41, Some("DP-1"), (0, 0));
        let viejo = output(42, None, (5000, 0));
        let list = to_display_infos(&[laptop, externo, viejo]);

        assert_eq!(list[0].id, DisplayId("eDP-1".into()));
        assert_eq!(list[0].name, "Pantalla integrada");
        assert!(list[0].builtin && !list[0].primary);
        assert_eq!((list[0].width, list[0].height), (2560, 1440));

        assert!(list[1].primary && !list[1].builtin);
        assert_eq!(list[1].name, "DP-1");

        assert_eq!(list[2].id, DisplayId("wl-output-42".into()));
    }

    #[test]
    fn sin_monitor_en_el_origen_el_primero_es_primario() {
        let list =
            to_display_infos(&[output(1, Some("A"), (10, 0)), output(2, Some("B"), (20, 0))]);
        assert!(list[0].primary && !list[1].primary);
    }

    #[test]
    #[ignore = "necesita una sesión Wayland real (WAYLAND_DISPLAY)"]
    fn lista_las_salidas_del_compositor() {
        let outputs = list_outputs().unwrap();
        assert!(!outputs.is_empty());
        eprintln!("{outputs:#?}");
    }
}
