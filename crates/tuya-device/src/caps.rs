//! `DpMapping` → [`Capabilities`], con heurística de respaldo por DPs presentes.
//!
//! `Capabilities` vive en `openlight-types`, así que `from_mapping` no puede ser un método
//! inherente de otro crate: se expone como el trait [`CapabilitiesExt`] (con el trait importado
//! se escribe igual: `Capabilities::from_mapping(&mapeo)`).
//!
//! Los códigos salen de la nube (`switch_led`, `work_mode`, `bright_value(_v2)`,
//! `temp_value(_v2)`, `colour_data(_v2)`, `control_data`). Si el mapeo no los trae, o si nunca
//! se bajó de la nube, se deduce por los números de DP como hace `BulbDevice.detect_bulb` de
//! tinytuya: 1–5 son los focos viejos (tipo A) y 20–28 los nuevos (tipo B).

use openlight_types::{Capabilities, ColourFormat, DpMapping, DpRange, DpSchema, Dps};
use serde_json::Value;

/// Rango de brillo de los focos tipo A (`DEFAULT_DPSET['A']`: `value_min` 25, `value_max` 255).
const OLD_BRIGHT: (i32, i32) = (25, 255);
/// Rango de temperatura de los focos tipo A.
const OLD_TEMP: (i32, i32) = (0, 255);
/// Rango de brillo de los focos tipo B (`value_min` 10, `value_max` 1000).
const NEW_BRIGHT: (i32, i32) = (10, 1000);
/// Rango de temperatura de los focos tipo B.
const NEW_TEMP: (i32, i32) = (0, 1000);

/// Modos que acepta un foco cuando el esquema no los informa.
fn default_modes() -> Vec<String> {
    ["white", "colour", "scene", "music"]
        .map(str::to_owned)
        .to_vec()
}

/// Construcción de [`Capabilities`] a partir del esquema o de los DPs de un foco.
pub trait CapabilitiesExt: Sized {
    /// Capacidades según los códigos del mapeo de la nube. Si el mapeo no tiene un DP de
    /// encendido, se prueba la heurística por números de DP sobre sus claves.
    ///
    /// Devuelve `None` si ni así se encuentra el DP de encendido (no es una luz).
    fn from_mapping(mapping: &DpMapping) -> Option<Self>;

    /// Capacidades deducidas solo de los DPs presentes (`status()`): 1–5 foco viejo, 20–28 nuevo.
    ///
    /// Devuelve `None` si no es ni uno ni otro (por ejemplo, un enchufe con DP 1 solo se acepta
    /// como "sin color": hace falta al menos el DP de encendido).
    fn from_dps(dps: &Dps) -> Option<Self>;
}

impl CapabilitiesExt for Capabilities {
    fn from_mapping(mapping: &DpMapping) -> Option<Self> {
        let by_code = |codes: &[&str]| -> Option<(&String, &DpSchema)> {
            codes
                .iter()
                .find_map(|code| mapping.0.iter().find(|(_, s)| s.code == *code))
        };
        let Some((switch, _)) = by_code(&["switch_led", "switch_led_1", "switch"]) else {
            return from_numbers(mapping.0.keys().map(String::as_str), None);
        };
        let mode = by_code(&["work_mode"]);
        let bright = by_code(&["bright_value_v2", "bright_value"]);
        let temp = by_code(&["temp_value_v2", "temp_value"]);
        let colour = by_code(&["colour_data_v2", "colour_data"]);
        let control = by_code(&["control_data"]);

        Some(Capabilities {
            dp_switch: switch.clone(),
            dp_mode: mode.map(|(dp, _)| dp.clone()),
            dp_bright: bright.map(|(dp, s)| range(dp, s, NEW_BRIGHT)),
            dp_temp: temp.map(|(dp, s)| range(dp, s, NEW_TEMP)),
            dp_colour: colour.map(|(dp, s)| {
                let format = if s.code.ends_with("_v2") {
                    ColourFormat::V2Hsv12
                } else {
                    ColourFormat::V1Hex14
                };
                (dp.clone(), format)
            }),
            dp_control: control.map(|(dp, _)| dp.clone()),
            modes: mode.map_or_else(Vec::new, |(_, s)| enum_range(s)),
        })
    }

    fn from_dps(dps: &Dps) -> Option<Self> {
        from_numbers(dps.keys().map(String::as_str), Some(dps))
    }
}

/// Rango de un DP numérico desde su esquema (`values.min/max/step`); si no los trae, `fallback`.
fn range(dp: &str, schema: &DpSchema, fallback: (i32, i32)) -> DpRange {
    let int = |key: &str| {
        schema
            .values
            .get(key)
            .and_then(Value::as_i64)
            .and_then(|n| i32::try_from(n).ok())
    };
    DpRange {
        dp: dp.to_owned(),
        min: int("min").unwrap_or(fallback.0),
        max: int("max").unwrap_or(fallback.1),
        step: int("step").unwrap_or(1),
    }
}

/// Valores de un DP `Enum` (`values.range`).
fn enum_range(schema: &DpSchema) -> Vec<String> {
    schema
        .values
        .get("range")
        .and_then(Value::as_array)
        .map(|r| {
            r.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// Heurística por números de DP (`detect_bulb`), con los DPs presentes o las claves del mapeo.
///
/// `dps` (si hay) sirve para distinguir el tipo C (DP 2 numérico = solo brillo) del A.
fn from_numbers<'a>(
    keys: impl Iterator<Item = &'a str>,
    dps: Option<&Dps>,
) -> Option<Capabilities> {
    let keys: Vec<&str> = keys.collect();
    let has = |dp: &str| keys.contains(&dp);
    let range = |dp: &str, (min, max): (i32, i32)| {
        has(dp).then(|| DpRange {
            dp: dp.to_owned(),
            min,
            max,
            step: 1,
        })
    };

    // Con el 1 y el 20 a la vez, `detect_bulb` dice "esto probablemente no es un foco".
    if has("20") && has("1") {
        return None;
    }
    if has("20") {
        // Tipo B: 20 encendido, 21 modo, 22 brillo, 23 temperatura, 24 color, 28 control.
        return Some(Capabilities {
            dp_switch: "20".into(),
            dp_mode: has("21").then(|| "21".to_owned()),
            dp_bright: range("22", NEW_BRIGHT),
            dp_temp: range("23", NEW_TEMP),
            dp_colour: has("24").then(|| ("24".to_owned(), ColourFormat::V2Hsv12)),
            dp_control: has("28").then(|| "28".to_owned()),
            modes: if has("21") {
                default_modes()
            } else {
                Vec::new()
            },
        });
    }
    if has("1") {
        // Tipo A: 1 encendido, 2 modo, 3 brillo, 4 temperatura, 5 color. Tipo C (solo
        // brillo): el DP 2 es un número, no el modo.
        let type_c = dps.is_some_and(|d| d.get("2").is_some_and(Value::is_number));
        if type_c {
            return Some(Capabilities {
                dp_switch: "1".into(),
                dp_mode: None,
                dp_bright: range("2", OLD_BRIGHT),
                dp_temp: range("3", OLD_TEMP),
                dp_colour: None,
                dp_control: None,
                modes: Vec::new(),
            });
        }
        return Some(Capabilities {
            dp_switch: "1".into(),
            dp_mode: has("2").then(|| "2".to_owned()),
            dp_bright: range("3", OLD_BRIGHT),
            dp_temp: range("4", OLD_TEMP),
            dp_colour: has("5").then(|| ("5".to_owned(), ColourFormat::V1Hex14)),
            dp_control: None,
            modes: if has("2") {
                default_modes()
            } else {
                Vec::new()
            },
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn mapping(json: &str) -> DpMapping {
        serde_json::from_str(json).expect("mapeo de prueba")
    }

    /// El mapeo del W509Z2 (DPs 20–28), saneado: el mismo perfil que usa `tuya-sim`.
    fn v2() -> DpMapping {
        mapping(include_str!("../../tuya-sim/profiles/v2.json"))
    }

    fn v1() -> DpMapping {
        mapping(include_str!("../../tuya-sim/profiles/v1.json"))
    }

    #[test]
    fn mapeo_nuevo_usa_los_codigos_de_la_nube() {
        let caps = Capabilities::from_mapping(&v2()).unwrap();
        assert_eq!(caps.dp_switch, "20");
        assert_eq!(caps.dp_mode.as_deref(), Some("21"));
        assert_eq!(
            caps.dp_bright,
            Some(DpRange {
                dp: "22".into(),
                min: 10,
                max: 1000,
                step: 1
            })
        );
        assert_eq!(caps.dp_temp.as_ref().map(|r| r.dp.as_str()), Some("23"));
        assert_eq!(
            caps.dp_colour,
            Some(("24".into(), ColourFormat::V2Hsv12)),
            "colour_data_v2 es hhhhssssvvvv"
        );
        assert_eq!(caps.dp_control.as_deref(), Some("28"));
        assert_eq!(caps.modes, ["white", "colour", "scene", "music"]);
        assert!(caps.supports_colour());
    }

    #[test]
    fn mapeo_viejo_usa_colour_data_hex14() {
        let caps = Capabilities::from_mapping(&v1()).unwrap();
        assert_eq!(caps.dp_switch, "1");
        assert_eq!(caps.dp_mode.as_deref(), Some("2"));
        assert_eq!(
            caps.dp_bright.as_ref().map(|r| (r.min, r.max)),
            Some((25, 255))
        );
        assert_eq!(caps.dp_colour, Some(("5".into(), ColourFormat::V1Hex14)));
        assert_eq!(caps.dp_control, None);
    }

    #[test]
    fn los_codigos_v2_ganan_sobre_los_viejos() {
        let m = mapping(
            r#"{"1": {"code": "bright_value", "type": "Integer", "values": {"min": 25, "max": 255}},
                "2": {"code": "bright_value_v2", "type": "Integer", "values": {"min": 10, "max": 1000}},
                "3": {"code": "switch_led", "type": "Boolean", "values": {}},
                "4": {"code": "colour_data", "type": "String", "values": {}},
                "5": {"code": "colour_data_v2", "type": "Json", "values": {}}}"#,
        );
        let caps = Capabilities::from_mapping(&m).unwrap();
        assert_eq!(caps.dp_bright.unwrap().dp, "2");
        assert_eq!(caps.dp_colour, Some(("5".into(), ColourFormat::V2Hsv12)));
    }

    #[test]
    fn sin_color_no_soporta_color() {
        let m = mapping(
            r#"{"20": {"code": "switch_led", "type": "Boolean", "values": {}},
                "22": {"code": "bright_value_v2", "type": "Integer", "values": {}}}"#,
        );
        let caps = Capabilities::from_mapping(&m).unwrap();
        assert!(!caps.supports_colour());
        // Sin `min`/`max` en el esquema vale el rango por defecto del tipo.
        assert_eq!(
            caps.dp_bright.as_ref().map(|r| (r.min, r.max)),
            Some((10, 1000))
        );
        assert!(caps.modes.is_empty());
    }

    #[test]
    fn sin_codigos_cae_a_la_heuristica_por_numeros() {
        // Un mapeo con los DPs pero sin los códigos conocidos (productos viejos de la nube).
        let m = mapping(
            r#"{"20": {"code": "x1", "type": "Boolean", "values": {}},
                "21": {"code": "x2", "type": "Enum", "values": {}},
                "24": {"code": "x3", "type": "String", "values": {}}}"#,
        );
        let caps = Capabilities::from_mapping(&m).unwrap();
        assert_eq!(caps.dp_switch, "20");
        assert_eq!(caps.dp_colour, Some(("24".into(), ColourFormat::V2Hsv12)));
    }

    #[test]
    fn no_es_una_luz_si_no_hay_encendido() {
        assert_eq!(Capabilities::from_mapping(&DpMapping::default()), None);
        let m = mapping(r#"{"9": {"code": "countdown_1", "type": "Integer", "values": {}}}"#);
        assert_eq!(Capabilities::from_mapping(&m), None);
    }

    fn dps(value: serde_json::Value) -> Dps {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn heuristica_por_dps_del_foco_nuevo() {
        // Estado real del W509Z2 (valores saneados): DPs 20–26 y 34.
        let status = dps(json!({"20": true, "21": "colour", "22": 1000, "23": 1000,
                                "24": "007c013c02d8", "25": "000e0d0000000000000000c80000",
                                "26": 0, "34": false}));
        let caps = Capabilities::from_dps(&status).unwrap();
        assert_eq!(caps.dp_switch, "20");
        assert_eq!(caps.dp_colour, Some(("24".into(), ColourFormat::V2Hsv12)));
        assert_eq!(caps.dp_bright.as_ref().map(|r| r.max), Some(1000));
        assert_eq!(caps.dp_control, None, "el 28 no se reporta en status()");
    }

    #[test]
    fn heuristica_por_dps_del_foco_viejo() {
        let status =
            dps(json!({"1": true, "2": "white", "3": 255, "4": 255, "5": "ff00000000ffff"}));
        let caps = Capabilities::from_dps(&status).unwrap();
        assert_eq!(caps.dp_switch, "1");
        assert_eq!(caps.dp_colour, Some(("5".into(), ColourFormat::V1Hex14)));
        assert_eq!(
            caps.dp_bright.as_ref().map(|r| (r.dp.as_str(), r.max)),
            Some(("3", 255))
        );
    }

    #[test]
    fn heuristica_tipo_c_solo_brillo() {
        // DP 2 numérico: es el brillo, no el modo (`detect_bulb`, tipo C).
        let status = dps(json!({"1": true, "2": 25, "3": 0}));
        let caps = Capabilities::from_dps(&status).unwrap();
        assert_eq!(caps.dp_mode, None);
        assert_eq!(caps.dp_bright.as_ref().map(|r| r.dp.as_str()), Some("2"));
        assert!(!caps.supports_colour());
    }

    #[test]
    fn heuristica_rechaza_lo_que_no_es_un_foco() {
        // El 1 y el 20 a la vez: `detect_bulb` lo marca como "no es un foco".
        assert_eq!(
            Capabilities::from_dps(&dps(json!({"1": true, "20": true}))),
            None
        );
        assert_eq!(Capabilities::from_dps(&dps(json!({"101": 5}))), None);
        assert_eq!(Capabilities::from_dps(&Dps::new()), None);
    }
}
