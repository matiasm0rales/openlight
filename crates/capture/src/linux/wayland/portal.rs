//! Portal ScreenCast (`org.freedesktop.portal.ScreenCast`) con `persist_mode = 2`.
//!
//! Con `persist_mode = 2` (persistir hasta que el usuario lo revoque) el portal devuelve un
//! `restore_token`. Pasándolo en la próxima sesión, el portal restaura la misma elección sin
//! mostrar el diálogo. El token es de un solo uso: cada `Start` devuelve uno nuevo y el anterior
//! deja de servir, así que siempre hay que guardar el último.

use std::os::fd::OwnedFd;
use std::time::Duration;

use ashpd::desktop::screencast::{
    CursorMode, Screencast, SelectSourcesOptions, SourceType, StartCastOptions,
};
use ashpd::desktop::{PersistMode, ResponseError, Session};
use openlight_types::CaptureError;

use crate::linux::detect::PortalStatus;

/// Lo que importa de un stream del portal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StreamInfo {
    /// Nodo de PipeWire.
    pub node_id: u32,
    /// Posición lógica del monitor, si el compositor la informa.
    pub position: Option<(i32, i32)>,
    /// Tamaño lógico del monitor, si el compositor lo informa.
    pub size: Option<(i32, i32)>,
}

/// Una sesión de captura aceptada. La sesión sigue abierta mientras viva este valor.
pub(crate) struct Grant {
    /// Proxy del portal (hace falta para cerrar la sesión).
    pub _screencast: Screencast,
    /// La sesión: el portal la cierra si se cierra la conexión D-Bus.
    pub session: Session<Screencast>,
    /// Streams elegidos (con `multiple = false`, uno).
    pub streams: Vec<StreamInfo>,
    /// Token nuevo para la próxima sesión.
    pub restore_token: Option<String>,
    /// Conexión a PipeWire que habilita el portal, si se pidió.
    pub pipewire_fd: Option<OwnedFd>,
}

impl Grant {
    /// Cierra la sesión (el compositor deja de mostrar el indicador de captura).
    pub(crate) async fn close(self) {
        if let Err(e) = self.session.close().await {
            tracing::debug!("no se pudo cerrar la sesión del portal: {e}");
        }
    }
}

/// Traduce los errores del portal al contrato.
pub(crate) fn map_error(e: ashpd::Error) -> CaptureError {
    match e {
        ashpd::Error::Response(ResponseError::Cancelled) => CaptureError::PermissionDenied,
        ashpd::Error::PortalNotFound(_) => {
            CaptureError::Unsupported(format!("no hay portal ScreenCast: {e}"))
        }
        other => CaptureError::Backend(format!("portal ScreenCast: {other}")),
    }
}

/// Se fija si el portal ScreenCast está y ofrece capturar monitores.
pub(crate) async fn probe() -> PortalStatus {
    if !cfg!(feature = "pipewire") {
        return PortalStatus::Missing("compilado sin la feature `pipewire`".into());
    }
    let check = async {
        let proxy = Screencast::new().await.map_err(|e| e.to_string())?;
        let types = proxy
            .available_source_types()
            .await
            .map_err(|e| e.to_string())?;
        if types.contains(SourceType::MONITOR) {
            Ok(())
        } else {
            Err(format!("el portal solo ofrece {types:?}"))
        }
    };
    match tokio::time::timeout(Duration::from_secs(5), check).await {
        Ok(Ok(())) => PortalStatus::Available,
        Ok(Err(why)) => PortalStatus::Missing(why),
        Err(_) => PortalStatus::Missing("el portal no respondió en 5 s".into()),
    }
}

/// Abre una sesión: crea, elige fuentes (con el token si hay) y arranca. Si no hay token o no
/// sirve, el portal muestra el diálogo y esto espera a que el usuario elija.
pub(crate) async fn open(
    restore_token: Option<&str>,
    want_pipewire: bool,
) -> Result<Grant, CaptureError> {
    let screencast = Screencast::new().await.map_err(map_error)?;
    let session = screencast
        .create_session(Default::default())
        .await
        .map_err(map_error)?;
    let request = screencast
        .select_sources(
            &session,
            SelectSourcesOptions::default()
                .set_cursor_mode(CursorMode::HIDDEN)
                .set_sources(SourceType::MONITOR)
                .set_multiple(false)
                .set_restore_token(restore_token)
                .set_persist_mode(PersistMode::ExplicitlyRevoked),
        )
        .await
        .map_err(map_error)?;
    request.response().map_err(map_error)?;

    let response = screencast
        .start(&session, None, StartCastOptions::default())
        .await
        .map_err(map_error)?
        .response()
        .map_err(map_error)?;
    let streams: Vec<StreamInfo> = response
        .streams()
        .iter()
        .map(|s| StreamInfo {
            node_id: s.pipe_wire_node_id(),
            position: s.position(),
            size: s.size(),
        })
        .collect();
    let restore_token = response.restore_token().map(str::to_owned);
    if streams.is_empty() {
        let _ = session.close().await;
        return Err(CaptureError::Backend(
            "el portal no devolvió streams".into(),
        ));
    }

    let pipewire_fd = if want_pipewire {
        match screencast
            .open_pipe_wire_remote(&session, Default::default())
            .await
        {
            Ok(fd) => Some(fd),
            Err(e) => {
                let _ = session.close().await;
                return Err(map_error(e));
            }
        }
    } else {
        None
    };

    Ok(Grant {
        _screencast: screencast,
        session,
        streams,
        restore_token,
        pipewire_fd,
    })
}
