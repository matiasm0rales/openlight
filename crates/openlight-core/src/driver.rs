//! Lo que el pipeline necesita de un foco.
//!
//! [`openlight_types::LightDevice`] es el contrato de bajo nivel (DPs crudos). El pipeline
//! trabaja con colores y con "dejar el foco como estaba", que es lo que arma el actor de
//! `tuya-device` encima de `LightDevice` (codificar el color según el formato del foco,
//! guardar y devolver el estado original, volver al modo color al reconectar). Este trait
//! es esa vista alta: así el core se prueba con focos falsos y no depende de `tuya-device`.

use async_trait::async_trait;
use openlight_types::{DeviceError, Hsv, LinkHealth};

/// Un foco al que se le pueden mandar colores.
#[async_trait]
pub trait LightDriver: Send {
    /// Deja el foco listo para recibir colores: guarda su estado original y lo pone en modo
    /// color (`preparar` del script). Se llama al arrancar y al volver de una pausa.
    async fn prepare(&mut self) -> Result<(), DeviceError>;

    /// Manda un color sin esperar la respuesta (`enviar_hsv`). Nunca pide brillo 0.
    async fn send_hsv(&mut self, color: Hsv) -> Result<(), DeviceError>;

    /// Devuelve el foco a su estado original (`devolver_estado`). `true` si lo restauró.
    async fn restore(&mut self) -> Result<bool, DeviceError>;

    /// Salud del enlace: de ahí salen `connected` y `reconnects`, que el pipeline mira en
    /// cada paso (un foco desconectado no recibe envíos; tras reconectar se reenvía el color).
    fn health(&self) -> LinkHealth;
}
