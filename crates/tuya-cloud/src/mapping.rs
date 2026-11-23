//! Armado del mapeo `dp → esquema` desde `GET /v1.1/devices/{id}/specifications`.
//!
//! Port de `tinytuya/Cloud.py::_build_mapping` y `getmapping`: primero se procesan los DPs de
//! `result.status` y después los de `result.functions`; si un DP ya está, gana el primero.

use openlight_types::{DpMapping, DpSchema};
use serde_json::Value;

/// Agrega a `dst` los DPs de una lista (`status` o `functions`), como `_build_mapping`.
///
/// Un elemento sin `code`, `type` o `values` se saltea (en tinytuya, la excepción se traga con
/// `log.debug('Parse mapping item failed!')`).
pub fn build_mapping(src: &[Value], dst: &mut DpMapping) {
    for item in src {
        let Some(schema) = parse_item(item) else {
            continue;
        };
        let dp_id = match item.get("dp_id") {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Number(n)) => n.to_string(),
            Some(_) => continue,
            None => schema.code.clone(),
        };
        dst.0.entry(dp_id).or_insert(schema);
    }
}

/// Mapeo completo desde el `result` de `specifications`: `status` primero y `functions` después.
pub fn mapping_from_specifications(result: &Value) -> DpMapping {
    let mut dps = DpMapping::default();
    for key in ["status", "functions"] {
        if let Some(list) = result.get(key).and_then(Value::as_array) {
            build_mapping(list, &mut dps);
        }
    }
    dps
}

fn parse_item(item: &Value) -> Option<DpSchema> {
    let code = item.get("code")?.as_str()?.to_owned();
    let kind = item.get("type")?.as_str()?.to_owned();
    let raw = item.get("values")?;
    let kind_lower = kind.to_lowercase();

    let mut values = if kind_lower == "string" {
        raw.clone()
    } else {
        // `json.loads(mapp['values'])`, y si falla, el valor tal cual.
        raw.as_str()
            .and_then(|s| serde_json::from_str::<Value>(s).ok())
            .unwrap_or_else(|| raw.clone())
    };
    if let Some(unit) = values.get_mut("unit")
        && let Some(text) = unit.as_str()
        && !text.is_empty()
    {
        *unit = Value::String(normalize_unit(text));
    }
    // "Tuya's 'JSON' mapping type is an ordered dict, but python's dict is not! so, save the raw
    // value as well".
    let raw_values = (kind_lower == "json").then(|| match raw {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    });
    Some(DpSchema {
        code,
        kind,
        values,
        raw_values,
    })
}

/// Normaliza unidades igual que tinytuya, con los mismos reemplazos y en el mismo orden:
/// `℉→°F`, `℃→°C`, `f→°F`, `c→°C`, `秒→s`.
pub fn normalize_unit(unit: &str) -> String {
    unit.replace('℉', "°F")
        .replace('℃', "°C")
        .replace('f', "°F")
        .replace('c', "°C")
        .replace('秒', "s")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn status_gana_sobre_functions() {
        let result = json!({
            "status": [{"code": "switch_led", "dp_id": 20, "type": "Boolean", "values": "{}"}],
            "functions": [
                {"code": "otro", "dp_id": 20, "type": "Enum", "values": "{}"},
                {"code": "work_mode", "dp_id": "21", "type": "Enum", "values": "{\"range\":[\"white\"]}"}
            ]
        });
        let m = mapping_from_specifications(&result);
        assert_eq!(m.0["20"].code, "switch_led");
        assert_eq!(m.0["21"].values, json!({"range": ["white"]}));
    }

    #[test]
    fn sin_dp_id_usa_el_codigo() {
        let mut m = DpMapping::default();
        build_mapping(
            &[json!({"code": "remote_name", "type": "String", "values": "{\"maxlen\":32}"})],
            &mut m,
        );
        // Los `String` no se parsean: quedan como texto.
        assert_eq!(m.0["remote_name"].values, json!("{\"maxlen\":32}"));
        assert_eq!(m.0["remote_name"].raw_values, None);
    }

    #[test]
    fn json_guarda_el_texto_original() {
        let mut m = DpMapping::default();
        let raw = "{\"h\":{\"min\":0,\"max\":360},\"s\":{\"min\":0,\"max\":1000}}";
        build_mapping(
            &[json!({"code": "colour_data_v2", "dp_id": 24, "type": "Json", "values": raw})],
            &mut m,
        );
        assert_eq!(m.0["24"].raw_values.as_deref(), Some(raw));
        assert_eq!(m.0["24"].values["h"]["max"], 360);
    }

    #[test]
    fn valores_que_no_son_json_quedan_como_vienen() {
        let mut m = DpMapping::default();
        build_mapping(
            &[json!({"code": "x", "dp_id": 1, "type": "Raw", "values": "no es json"})],
            &mut m,
        );
        assert_eq!(m.0["1"].values, json!("no es json"));
    }

    #[test]
    fn elementos_incompletos_se_saltean() {
        let mut m = DpMapping::default();
        build_mapping(
            &[
                json!({"dp_id": 1, "type": "Raw", "values": "{}"}),
                json!({"code": "x", "dp_id": 2, "values": "{}"}),
                json!({"code": "y", "dp_id": 3, "type": "Raw"}),
            ],
            &mut m,
        );
        assert!(m.0.is_empty());
    }

    #[test]
    fn unidades_como_tinytuya() {
        assert_eq!(normalize_unit("℃"), "°C");
        assert_eq!(normalize_unit("℉"), "°F");
        assert_eq!(normalize_unit("秒"), "s");
        assert_eq!(normalize_unit("c"), "°C");
        assert_eq!(normalize_unit("%"), "%");
        let mut m = DpMapping::default();
        build_mapping(
            &[
                json!({"code": "t", "dp_id": 7, "type": "Integer", "values": "{\"unit\":\"℃\",\"min\":0}"}),
            ],
            &mut m,
        );
        assert_eq!(m.0["7"].values["unit"], "°C");
    }
}
