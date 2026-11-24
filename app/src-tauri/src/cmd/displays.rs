//! Monitores, vista previa y zonas.

use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use openlight_types::ipc::PreviewImage;
use openlight_types::{CaptureError, DeviceId, DisplayId, DisplayInfo, Frame, Zone};

use crate::errors::{CmdResult, capture_error, internal, invalid, unknown_device};
use crate::state::Backend;

/// Cuánto se espera el primer cuadro de la vista previa.
const PREVIEW_WAIT: Duration = Duration::from_secs(4);
/// Máximo de zonas por foco.
const MAX_ZONES: usize = 8;

impl Backend {
    /// Monitores disponibles ahora.
    pub fn displays_list(&self) -> CmdResult<Vec<DisplayInfo>> {
        (self.sources)().displays().map_err(|e| capture_error(&e))
    }

    /// Una captura de `display` como PNG, para elegir las zonas.
    pub async fn display_preview(&self, display: DisplayId) -> CmdResult<PreviewImage> {
        let mut source = (self.sources)();
        let info = source
            .displays()
            .map_err(|e| capture_error(&e))?
            .into_iter()
            .find(|d| d.id == display)
            .ok_or_else(|| capture_error(&CaptureError::DisplayNotFound(display.clone())))?;
        let width = self.options.preview_width.max(16);
        let height = preview_height(&info, width);
        let mut frames = source
            .start(display.clone(), (width, height), 5.0)
            .map_err(|e| capture_error(&e))?;
        let frame = tokio::time::timeout(PREVIEW_WAIT, async {
            loop {
                if let Some(frame) = frames.borrow_and_update().clone() {
                    return Some(frame);
                }
                if frames.changed().await.is_err() {
                    return None;
                }
            }
        })
        .await;
        source.stop();
        let frame = match frame {
            Ok(Some(frame)) => frame,
            // Sin cuadros: monitor dormido o sin permiso de grabación
            _ => return Err(capture_error(&CaptureError::DisplayNotFound(display))),
        };
        let png = frame_to_png(&frame).map_err(internal)?;
        Ok(PreviewImage {
            display,
            width: frame.width,
            height: frame.height,
            png_base64: STANDARD.encode(png),
        })
    }

    /// Define las zonas que sigue un foco. Vale desde la próxima sesión.
    pub fn zones_set(&self, device: DeviceId, zones: Vec<Zone>) -> CmdResult<()> {
        if zones.len() > MAX_ZONES {
            return Err(invalid("zones", format!("como mucho {MAX_ZONES} zonas")));
        }
        if let Some(bad) = zones.iter().find(|z| !z.rect.is_valid()) {
            return Err(invalid(
                "zones",
                format!("la zona {:?} queda fuera del monitor", bad.rect),
            ));
        }
        let mut records = self.records()?;
        let record = records
            .iter_mut()
            .find(|r| r.id == device)
            .ok_or_else(|| unknown_device(&device))?;
        record.zones = zones;
        self.store.save_devices(&records)?;
        Ok(())
    }
}

/// Alto de la vista previa con la proporción del monitor.
fn preview_height(info: &DisplayInfo, width: u32) -> u32 {
    if info.width == 0 || info.height == 0 {
        return width * 9 / 16;
    }
    (u64::from(width) * u64::from(info.height) / u64::from(info.width)).clamp(1, 4096) as u32
}

/// BGRA → PNG RGBA (el cuadro no trae transparencia que importe).
fn frame_to_png(frame: &Frame) -> Result<Vec<u8>, String> {
    let rgba: Vec<u8> = frame
        .bgra
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|px| [px[2], px[1], px[0], 255])
        .collect();
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, frame.width, frame.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(&rgba).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_png_conserva_los_colores_y_el_tamano() {
        // Un píxel azul en BGRA (b=255) tiene que salir azul en RGBA
        let frame = Frame::new(2, 1, vec![255, 0, 0, 255, 0, 0, 255, 255]).unwrap();
        let bytes = frame_to_png(&frame).unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        let mut reader = decoder.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut buf).unwrap();
        assert_eq!((info.width, info.height), (2, 1));
        assert_eq!(buf[..8], [0, 0, 255, 255, 255, 0, 0, 255]);
    }

    #[test]
    fn la_vista_previa_respeta_la_proporcion_del_monitor() {
        let info = |w, h| DisplayInfo {
            id: DisplayId("1".into()),
            name: "m".into(),
            width: w,
            height: h,
            builtin: false,
            primary: false,
        };
        assert_eq!(preview_height(&info(1920, 1080), 480), 270);
        assert_eq!(preview_height(&info(3024, 1964), 480), 311);
        assert_eq!(preview_height(&info(0, 0), 480), 270);
    }
}
