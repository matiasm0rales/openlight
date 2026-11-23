//! Perfiles de DPs: qué DPs tiene el foco simulado, qué valores acepta y con qué arranca.

use openlight_types::{DpMapping, DpSchema, Dps};
use serde_json::{Value, json};

/// Esquema de DPs del foco simulado.
#[derive(Debug, Clone, PartialEq)]
pub enum Profile {
    /// Foco viejo: DPs 1–5 (`switch_led`, `work_mode`, `bright_value`, `temp_value`,
    /// `colour_data` en formato `rrggbb0hhhssvv`).
    V1,
    /// Foco nuevo: DPs 20–34, el mapeo del W509Z2 de `devices.json` (saneado).
    V2,
    /// Un mapeo cualquiera (por ejemplo, el que devuelve la nube).
    Custom(DpMapping),
}

impl Profile {
    /// El mapeo `dp → esquema`.
    pub fn mapping(&self) -> DpMapping {
        match self {
            Self::V1 => parse(include_str!("../profiles/v1.json")),
            Self::V2 => parse(include_str!("../profiles/v2.json")),
            Self::Custom(m) => m.clone(),
        }
    }

    /// Los DPs con los que arranca el foco.
    ///
    /// V2 arranca como `ESTADO_FOCO` de `test_ambilight.py` (en modo color); V1, en blanco.
    /// Un mapeo propio arranca con valores neutros derivados del esquema.
    pub fn default_dps(&self) -> Dps {
        let value = match self {
            Self::V1 => json!({"1": true, "2": "white", "3": 255, "4": 255, "5": "ff00000000ffff"}),
            Self::V2 => json!({"20": true, "21": "colour", "22": 1000, "23": 1000,
                               "24": "007c013c02d8", "25": "000e0d0000000000000000c80000",
                               "26": 0, "34": false}),
            Self::Custom(m) => {
                return m
                    .0
                    .iter()
                    .filter_map(|(dp, s)| neutral(s).map(|v| (dp.clone(), v)))
                    .collect();
            }
        };
        serde_json::from_value(value).expect("DPs por defecto válidos")
    }
}

fn parse(text: &str) -> DpMapping {
    serde_json::from_str(text).expect("perfil incluido válido")
}

/// Valor neutro para un DP de un mapeo propio. `None` para los que no se reportan (`Raw`).
fn neutral(schema: &DpSchema) -> Option<Value> {
    Some(match schema.kind.as_str() {
        "Boolean" => Value::Bool(false),
        "Enum" => schema
            .values
            .get("range")
            .and_then(|r| r.get(0))
            .cloned()
            .unwrap_or(Value::String(String::new())),
        "Integer" => schema.values.get("min").cloned().unwrap_or(Value::from(0)),
        "Json" | "String" => Value::String(String::new()),
        _ => return None,
    })
}

/// Si el foco acepta `value` para un DP con este esquema. Los focos reales ignoran en silencio
/// los valores que no entienden; el simulador hace lo mismo.
pub fn accepts(schema: &DpSchema, value: &Value) -> bool {
    match schema.kind.as_str() {
        "Boolean" => value.is_boolean(),
        "Enum" => match (value.as_str(), schema.values.get("range")) {
            (Some(v), Some(Value::Array(range))) => range.iter().any(|r| r.as_str() == Some(v)),
            (Some(_), None) => true,
            _ => false,
        },
        "Integer" => {
            let Some(n) = value.as_i64() else {
                return false;
            };
            let min = schema.values.get("min").and_then(Value::as_i64);
            let max = schema.values.get("max").and_then(Value::as_i64);
            min.is_none_or(|m| n >= m) && max.is_none_or(|m| n <= m)
        }
        "Json" | "String" | "Raw" | "Bitmap" => value.is_string() || value.is_number(),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perfiles_incluidos() {
        let v2 = Profile::V2.mapping();
        assert_eq!(v2.0["20"].code, "switch_led");
        assert_eq!(v2.0["24"].code, "colour_data_v2");
        assert_eq!(v2.0["28"].code, "control_data");
        let v1 = Profile::V1.mapping();
        assert_eq!(v1.0["1"].code, "switch_led");
        assert_eq!(v1.0["5"].code, "colour_data");
        for p in [Profile::V1, Profile::V2] {
            let m = p.mapping();
            for (dp, v) in p.default_dps() {
                assert!(accepts(&m.0[&dp], &v), "{dp}={v} no entra en su esquema");
            }
        }
    }

    #[test]
    fn valida_rangos_y_enums() {
        let m = Profile::V2.mapping();
        assert!(accepts(&m.0["22"], &json!(10)));
        assert!(!accepts(&m.0["22"], &json!(0)));
        assert!(!accepts(&m.0["22"], &json!(1001)));
        assert!(accepts(&m.0["21"], &json!("music")));
        assert!(!accepts(&m.0["21"], &json!("disco")));
        assert!(!accepts(&m.0["20"], &json!("true")));
    }

    #[test]
    fn mapeo_propio_arranca_neutro() {
        let p = Profile::Custom(Profile::V2.mapping());
        let dps = p.default_dps();
        assert_eq!(dps["20"], json!(false));
        assert_eq!(dps["21"], json!("white"));
        assert_eq!(dps["22"], json!(10));
        assert!(!dps.contains_key("30"), "los Raw no se reportan");
    }
}
