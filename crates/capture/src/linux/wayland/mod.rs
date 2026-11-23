//! Backend Wayland: portal ScreenCast + PipeWire.
//!
//! Cada captura vive en un hilo con su runtime de tokio (de un solo hilo), así `start` se puede
//! llamar desde cualquier lado, incluso desde adentro de otro runtime. El hilo:
//! 1. abre la sesión del portal con el último `restore_token` (sin token, el portal muestra el
//!    diálogo y `start` espera a que el usuario elija);
//! 2. guarda el token nuevo que devuelve el portal;
//! 3. lee los cuadros por PipeWire en otro hilo;
//! 4. si el stream se corta (monitor desconectado, sesión cerrada), publica `None`, espera a que
//!    el monitor vuelva a aparecer entre las salidas de Wayland y reabre la sesión con el token,
//!    que restaura la elección sin diálogo.

pub(crate) mod outputs;
mod portal;
#[cfg(feature = "pipewire")]
mod stream;

use std::future::Future;
use std::sync::Arc;
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use openlight_types::{CaptureError, DisplayId, DisplayInfo, Frame};
use tokio::sync::watch;

use super::detect::PortalStatus;
use super::publish;
use outputs::OutputInfo;
use portal::StreamInfo;

#[cfg(not(feature = "pipewire"))]
mod stream {
    //! Sin la feature `pipewire` no hay cuadros: `spawn` responde `Unsupported`.
    use std::os::fd::OwnedFd;
    use std::sync::Arc;
    use std::thread::JoinHandle;
    use std::time::Duration;

    use openlight_types::{CaptureError, Frame};
    use tokio::sync::{oneshot, watch};

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(crate) enum StreamEnd {
        Stopped,
        Lost(String),
    }

    pub(crate) struct StopHandle;

    impl StopHandle {
        pub(crate) fn stop(&self) {}
    }

    pub(crate) fn spawn(
        _fd: OwnedFd,
        _node_id: u32,
        _target: (u32, u32),
        _interval: Duration,
        _tx: Arc<watch::Sender<Option<Frame>>>,
    ) -> Result<(StopHandle, oneshot::Receiver<StreamEnd>, JoinHandle<()>), CaptureError> {
        Err(CaptureError::Unsupported(
            "OpenLight se compiló sin la feature `pipewire`".into(),
        ))
    }
}

use stream::StreamEnd;

/// Corre un futuro en un runtime propio en otro hilo y espera el resultado.
fn block_on<F, T>(make: impl FnOnce() -> F + Send + 'static) -> Result<T, CaptureError>
where
    F: Future<Output = T>,
    T: Send + 'static,
{
    std::thread::Builder::new()
        .name("openlight-portal".into())
        .spawn(move || runtime().map(|rt| rt.block_on(make())))
        .map_err(|e| CaptureError::Backend(format!("no se pudo crear el hilo: {e}")))?
        .join()
        .map_err(|_| CaptureError::Backend("el hilo del portal entró en pánico".into()))?
}

fn runtime() -> Result<tokio::runtime::Runtime, CaptureError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| CaptureError::Backend(format!("no se pudo crear el runtime: {e}")))
}

/// Sondea el portal ScreenCast (para elegir el backend).
pub(crate) fn probe_portal() -> PortalStatus {
    block_on(portal::probe).unwrap_or_else(|e| PortalStatus::Missing(e.to_string()))
}

/// Monitores según el compositor.
pub(crate) fn displays() -> Result<Vec<DisplayInfo>, CaptureError> {
    Ok(outputs::to_display_infos(&outputs::list_outputs()?))
}

/// Abre y cierra una sesión del portal para que el usuario dé el permiso, y guarda el token.
/// Si falla, el token anterior queda como estaba.
pub(crate) fn request_permission(
    token: &watch::Sender<Option<String>>,
) -> Result<(), CaptureError> {
    let current = token.borrow().clone();
    let new_token = block_on(move || async move {
        let grant = portal::open(current.as_deref(), false).await?;
        let new_token = grant.restore_token.clone();
        grant.close().await;
        Ok::<_, CaptureError>(new_token)
    })??;
    token.send_replace(new_token);
    Ok(())
}

/// Qué monitor eligió el usuario en el diálogo, comparando la posición del stream con la de
/// cada salida. `None` si el portal no informa posición o no coincide con ninguna.
fn chosen_output(stream: &StreamInfo, outputs: &[OutputInfo]) -> Option<DisplayId> {
    let position = stream.position?;
    let mut candidates = outputs.iter().filter(|o| o.origin() == Some(position));
    let first = candidates.next()?;
    // Con salidas espejadas en la misma posición, el tamaño desempata.
    let best = match stream.size {
        Some(size) => std::iter::once(first)
            .chain(candidates)
            .find(|o| o.logical_size == Some(size))
            .unwrap_or(first),
        None => first,
    };
    Some(best.id())
}

/// Configuración de un hilo de captura.
struct Config {
    display: DisplayId,
    target: (u32, u32),
    interval: Duration,
    recheck: Duration,
    frames: Arc<watch::Sender<Option<Frame>>>,
    token: Arc<watch::Sender<Option<String>>>,
}

/// Captura en curso.
pub(crate) struct WaylandCapture {
    stop: watch::Sender<bool>,
    thread: Option<JoinHandle<()>>,
}

impl WaylandCapture {
    /// Abre la sesión del portal (puede esperar al diálogo) y arranca la lectura.
    pub(crate) fn start(
        display: DisplayId,
        target: (u32, u32),
        interval: Duration,
        recheck: Duration,
        token: Arc<watch::Sender<Option<String>>>,
    ) -> Result<(Self, watch::Receiver<Option<Frame>>), CaptureError> {
        // Si se puede saber qué salidas hay, el monitor pedido tiene que estar.
        if let Ok(list) = outputs::list_outputs()
            && !list.is_empty()
            && !list.iter().any(|o| o.id() == display)
        {
            return Err(CaptureError::DisplayNotFound(display));
        }
        let (frames, rx) = watch::channel(None);
        let (stop, stop_rx) = watch::channel(false);
        let (ready_tx, ready_rx) = mpsc::channel();
        let config = Config {
            display,
            target,
            interval,
            recheck,
            frames: Arc::new(frames),
            token,
        };
        let thread = std::thread::Builder::new()
            .name("openlight-wayland".into())
            .spawn(move || match runtime() {
                Ok(rt) => rt.block_on(session_loop(config, stop_rx, ready_tx)),
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                }
            })
            .map_err(|e| CaptureError::Backend(format!("no se pudo crear el hilo: {e}")))?;
        let ready = ready_rx.recv().unwrap_or_else(|_| {
            Err(CaptureError::Backend(
                "el hilo de Wayland terminó antes de arrancar".into(),
            ))
        });
        match ready {
            Ok(()) => Ok((
                Self {
                    stop,
                    thread: Some(thread),
                },
                rx,
            )),
            Err(e) => {
                let _ = thread.join();
                Err(e)
            }
        }
    }

    /// Frena la captura, cierra la sesión y espera al hilo.
    pub(crate) fn stop(&mut self) {
        let _ = self.stop.send(true);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for WaylandCapture {
    fn drop(&mut self) {
        self.stop();
    }
}

async fn stopped(stop: &mut watch::Receiver<bool>) {
    // Si el emisor se fue, también hay que terminar.
    let _ = stop.wait_for(|s| *s).await;
}

async fn session_loop(
    config: Config,
    mut stop: watch::Receiver<bool>,
    ready: mpsc::Sender<Result<(), CaptureError>>,
) {
    let mut ready = Some(ready);
    loop {
        let token = config.token.borrow().clone();
        let opened = tokio::select! {
            result = portal::open(token.as_deref(), true) => result,
            () = stopped(&mut stop) => return,
        };
        let mut grant = match opened {
            Ok(grant) => grant,
            Err(e) => {
                if let Some(ready) = ready.take() {
                    let _ = ready.send(Err(e));
                    return;
                }
                tracing::warn!("no se pudo reabrir la sesión del portal: {e}");
                publish(&config.frames, None);
                // Si el usuario canceló el diálogo, no se lo vuelve a molestar.
                if matches!(
                    e,
                    CaptureError::PermissionDenied | CaptureError::Unsupported(_)
                ) || !wait_for_output(&config, &mut stop).await
                {
                    return;
                }
                continue;
            }
        };
        config.token.send_replace(grant.restore_token.clone());

        let stream_info = grant.streams[0].clone();
        if let Ok(list) = outputs::list_outputs()
            && let Some(chosen) = chosen_output(&stream_info, &list)
            && chosen != config.display
        {
            tracing::warn!(
                "se pidió el monitor {} pero en el diálogo se eligió {chosen}; se captura ese",
                config.display
            );
        }

        let Some(fd) = grant.pipewire_fd.take() else {
            grant.close().await;
            if let Some(ready) = ready.take() {
                let _ = ready.send(Err(CaptureError::Backend(
                    "el portal no dio la conexión a PipeWire".into(),
                )));
            }
            return;
        };
        let spawned = stream::spawn(
            fd,
            stream_info.node_id,
            config.target,
            config.interval,
            Arc::clone(&config.frames),
        );
        let (handle, end_rx, thread) = match spawned {
            Ok(parts) => parts,
            Err(e) => {
                grant.close().await;
                if let Some(ready) = ready.take() {
                    let _ = ready.send(Err(e));
                } else {
                    tracing::warn!("no se pudo arrancar PipeWire: {e}");
                    publish(&config.frames, None);
                }
                return;
            }
        };
        if let Some(ready) = ready.take() {
            let _ = ready.send(Ok(()));
        }

        let end = tokio::select! {
            end = end_rx => end.unwrap_or_else(|_| StreamEnd::Lost("terminó el hilo de PipeWire".into())),
            () = stopped(&mut stop) => {
                handle.stop();
                StreamEnd::Stopped
            }
        };
        handle.stop();
        let _ = thread.join();
        grant.close().await;
        publish(&config.frames, None);

        match end {
            StreamEnd::Stopped => return,
            StreamEnd::Lost(why) => {
                tracing::info!(
                    "se cortó la captura de {} ({why}); esperando que vuelva",
                    config.display
                );
                if !wait_for_output(&config, &mut stop).await {
                    return;
                }
            }
        }
    }
}

/// Espera a que el monitor esté entre las salidas de Wayland. `false` si hay que terminar.
async fn wait_for_output(config: &Config, stop: &mut watch::Receiver<bool>) -> bool {
    loop {
        tokio::select! {
            () = tokio::time::sleep(config.recheck) => {}
            () = stopped(stop) => return false,
        }
        if config.frames.is_closed() {
            return false;
        }
        let display = config.display.clone();
        let present = tokio::task::spawn_blocking(move || {
            outputs::list_outputs().map(|list| list.iter().any(|o| o.id() == display))
        })
        .await;
        match present {
            Ok(Ok(false)) => continue,
            // Está, o no se puede saber: se reintenta con el portal.
            _ => return true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output(name: &str, origin: (i32, i32), size: (i32, i32)) -> OutputInfo {
        OutputInfo {
            name: Some(name.into()),
            logical_position: Some(origin),
            logical_size: Some(size),
            ..OutputInfo::default()
        }
    }

    #[test]
    fn reconoce_el_monitor_elegido() {
        let list = [
            output("eDP-1", (0, 0), (1280, 800)),
            output("DP-1", (1280, 0), (2560, 1440)),
        ];
        let stream = |position, size| StreamInfo {
            node_id: 1,
            position,
            size,
        };
        assert_eq!(
            chosen_output(&stream(Some((1280, 0)), None), &list),
            Some(DisplayId("DP-1".into()))
        );
        assert_eq!(chosen_output(&stream(None, None), &list), None);
        assert_eq!(chosen_output(&stream(Some((9, 9)), None), &list), None);

        let espejo = [
            output("A", (0, 0), (1920, 1080)),
            output("B", (0, 0), (1280, 720)),
        ];
        assert_eq!(
            chosen_output(&stream(Some((0, 0)), Some((1280, 720))), &espejo),
            Some(DisplayId("B".into()))
        );
    }
}
