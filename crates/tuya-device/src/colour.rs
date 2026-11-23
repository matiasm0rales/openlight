//! Codificación del color que se le manda al foco. Port de `valor_color()` de `ambilight.py`.
//!
//! | formato | valor |
//! |---|---|
//! | [`ColourFormat::V2Hsv12`] | `%04x%04x%04x` con h 0–360 y s/v 0–1000 (DP 24 del W509Z2) |
//! | [`ColourFormat::V1Hex14`] | `rrggbb` + `%04x` (h 0–360) + `%02x` s + `%02x` v (0–255): `BulbDevice` tipo A |
//! | `control_data` | transición (`0` directo / `1` gradual) + hsv de 12 + `00000000` |
//!
//! Igual que el script, `round` es el de Python (`round_ties_even`) y el brillo nunca baja de
//! [`MIN_BRIGHTNESS`]: pedirle brillo 0 al foco lo apaga y después tarda en volver.

use openlight_types::{Capabilities, ColourFormat, DeviceError, Hsv, SendChannel};

/// Brillo mínimo que se le pide al foco (`max(v, 0.01)` en `Foco.enviar_hsv`).
pub const MIN_BRIGHTNESS: f32 = 0.01;

/// Qué DP escribir y con qué valor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DpWrite {
    /// Número de DP.
    pub dp: String,
    /// Valor en texto hexadecimal, ya codificado.
    pub value: String,
}

/// Lleva `x` a `0..1` (los NaN a 0).
fn unit(x: f32) -> f64 {
    if x.is_nan() {
        0.0
    } else {
        f64::from(x.clamp(0.0, 1.0))
    }
}

/// `round(x * scale)` como Python (los empates van al par).
fn scaled(x: f64, scale: f64) -> u32 {
    // `x` está en 0..1, así que el resultado entra en u32 de sobra.
    (x * scale).round_ties_even() as u32
}

/// `hhhhssssvvvv`: h 0–360, s y v 0–1000. Es `valor_color` del script, sin el DP.
pub fn hsv12(hsv: Hsv) -> String {
    format!(
        "{:04x}{:04x}{:04x}",
        scaled(unit(hsv.h), 360.0),
        scaled(unit(hsv.s), 1000.0),
        scaled(unit(hsv.v), 1000.0)
    )
}

/// `rrggbb` + `hhhh` + `ss` + `vv` de los focos viejos (`colour_data`, DP 5).
pub fn hex14(hsv: Hsv) -> String {
    let (h, s, v) = (unit(hsv.h), unit(hsv.s), unit(hsv.v));
    let (r, g, b) = hsv_to_rgb(h, s, v);
    format!(
        "{:02x}{:02x}{:02x}{:04x}{:02x}{:02x}",
        scaled(r, 255.0),
        scaled(g, 255.0),
        scaled(b, 255.0),
        scaled(h, 360.0),
        scaled(s, 255.0),
        scaled(v, 255.0)
    )
}

/// `colorsys.hsv_to_rgb` de Python.
fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (f64, f64, f64) {
    if s == 0.0 {
        return (v, v, v);
    }
    let sector = (h * 6.0).floor();
    let f = h * 6.0 - sector;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    match (sector as i64).rem_euclid(6) {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    }
}

/// El canal que de verdad se usa: si el foco no tiene `control_data`, el color normal.
///
/// El script siempre tenía el DP 28 a mano; acá depende del mapeo, y un foco sin él no puede
/// recibir `directo` ni `gradual`.
pub fn effective_channel(caps: &Capabilities, requested: SendChannel) -> SendChannel {
    match requested {
        SendChannel::Colour => SendChannel::Colour,
        _ if caps.dp_control.is_some() => requested,
        _ => SendChannel::Colour,
    }
}

/// `(dp, valor)` para pedirle un color al foco por el canal elegido (`valor_color`).
///
/// El brillo se lleva al menos a [`MIN_BRIGHTNESS`].
///
/// # Errors
/// [`DeviceError::Unsupported`] si el foco no tiene DP de color.
pub fn colour_write(
    caps: &Capabilities,
    channel: SendChannel,
    hsv: Hsv,
) -> Result<DpWrite, DeviceError> {
    let hsv = Hsv {
        v: hsv.v.max(MIN_BRIGHTNESS),
        ..hsv
    };
    let transition = match effective_channel(caps, channel) {
        SendChannel::Colour => None,
        SendChannel::ControlDirect => Some("0"),
        SendChannel::ControlGradual => Some("1"),
    };
    match (transition, &caps.dp_colour, &caps.dp_control) {
        (Some(transition), _, Some(dp)) => Ok(DpWrite {
            dp: dp.clone(),
            // DP de control en tiempo real: transición + color + brillo y temperatura del blanco.
            value: format!("{transition}{}00000000", hsv12(hsv)),
        }),
        (None, Some((dp, format)), _) => Ok(DpWrite {
            dp: dp.clone(),
            value: match format {
                ColourFormat::V2Hsv12 => hsv12(hsv),
                ColourFormat::V1Hex14 => hex14(hsv),
            },
        }),
        _ => Err(DeviceError::Unsupported("color".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CapabilitiesExt;

    fn caps_v2() -> Capabilities {
        let m = serde_json::from_str(include_str!("../../tuya-sim/profiles/v2.json")).unwrap();
        Capabilities::from_mapping(&m).unwrap()
    }

    fn caps_v1() -> Capabilities {
        let m = serde_json::from_str(include_str!("../../tuya-sim/profiles/v1.json")).unwrap();
        Capabilities::from_mapping(&m).unwrap()
    }

    fn write(caps: &Capabilities, channel: SendChannel, hsv: Hsv) -> (String, String) {
        let w = colour_write(caps, channel, hsv).unwrap();
        (w.dp, w.value)
    }

    // port: test_ambilight.py::ValorColor::test_formatos
    #[test]
    fn valor_color_formatos() {
        let hsv = Hsv::new(124.0 / 360.0, 0.316, 0.728);
        let casos = [
            (SendChannel::Colour, "24", "007c013c02d8"),
            (SendChannel::ControlDirect, "28", "0007c013c02d800000000"),
            (SendChannel::ControlGradual, "28", "1007c013c02d800000000"),
        ];
        for (canal, dp, valor) in casos {
            assert_eq!(
                write(&caps_v2(), canal, hsv),
                (dp.to_owned(), valor.to_owned()),
                "{canal:?}"
            );
        }
    }

    // port: test_ambilight.py::ValorColor::test_valores_maximos
    #[test]
    fn valor_color_valores_maximos() {
        assert_eq!(
            write(&caps_v2(), SendChannel::Colour, Hsv::new(1.0, 1.0, 1.0)),
            ("24".to_owned(), "016803e803e8".to_owned())
        );
    }

    // port: test_ambilight.py::FocoConexion::test_nunca_pide_brillo_cero
    #[test]
    fn nunca_pide_brillo_cero() {
        assert_eq!(
            write(&caps_v2(), SendChannel::Colour, Hsv::new(0.0, 1.0, 0.0)),
            ("24".to_owned(), "000003e8000a".to_owned())
        );
        // También por el DP de control.
        assert_eq!(
            write(
                &caps_v2(),
                SendChannel::ControlDirect,
                Hsv::new(0.0, 1.0, 0.0)
            ),
            ("28".to_owned(), "0000003e8000a00000000".to_owned())
        );
    }

    #[test]
    fn redondea_como_python_los_empates_van_al_par() {
        // round(0.5) = 0, round(1.5) = 2, round(2.5) = 2 en Python 3.
        assert_eq!(scaled(0.5, 1.0), 0);
        assert_eq!(scaled(1.5, 1.0), 2);
        assert_eq!(scaled(2.5, 1.0), 2);
    }

    #[test]
    fn valores_fuera_de_rango_se_recortan() {
        assert_eq!(hsv12(Hsv::new(1.7, -0.3, f32::NAN)), "016800000000");
    }

    #[test]
    fn foco_viejo_usa_rrggbb_hhhh_ss_vv() {
        // h=124°, s=.316, v=.728 → rgb (127, 186, 131) → "7fba83", h 0x7c, s 81=0x51, v 186=0xba.
        let hsv = Hsv::new(124.0 / 360.0, 0.316, 0.728);
        assert_eq!(hex14(hsv), "7fba83007c51ba");
        assert_eq!(hex14(hsv).len(), 14);
        assert_eq!(
            write(&caps_v1(), SendChannel::Colour, hsv),
            ("5".to_owned(), "7fba83007c51ba".to_owned())
        );
        // Rojo puro a pleno brillo.
        assert_eq!(hex14(Hsv::new(0.0, 1.0, 1.0)), "ff00000000ffff");
        // Gris: sin saturación, r=g=b.
        assert_eq!(hex14(Hsv::new(0.0, 0.0, 0.5)), "80808000000080");
    }

    #[test]
    fn el_canal_de_control_cae_al_color_si_el_foco_no_lo_tiene() {
        assert_eq!(
            effective_channel(&caps_v1(), SendChannel::ControlDirect),
            SendChannel::Colour
        );
        assert_eq!(
            write(
                &caps_v1(),
                SendChannel::ControlGradual,
                Hsv::new(0.0, 1.0, 1.0)
            )
            .0,
            "5"
        );
        assert_eq!(
            effective_channel(&caps_v2(), SendChannel::ControlGradual),
            SendChannel::ControlGradual
        );
    }

    #[test]
    fn sin_dp_de_color_no_se_puede() {
        let mut caps = caps_v2();
        caps.dp_colour = None;
        caps.dp_control = None;
        assert!(matches!(
            colour_write(&caps, SendChannel::Colour, Hsv::new(0.0, 1.0, 1.0)),
            Err(DeviceError::Unsupported(_))
        ));
    }
}
