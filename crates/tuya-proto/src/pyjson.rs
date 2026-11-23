//! JSON con la misma salida que `json.dumps(obj).replace(" ", "")` de Python.
//!
//! - `ensure_ascii`: todo lo que no sea ASCII imprimible se escapa como `\uXXXX` (con pares
//!   sustitutos fuera del plano básico).
//! - Floats con `repr` de Python (`1.0`, `1e+16`, `1.5e-05`, `NaN`, `Infinity`).
//! - Los objetos se escriben en el orden en que se armaron. Los `serde_json::Value::Object`
//!   anidados salen en el orden de su `Map` (alfabético, sin `preserve_order`).
//! - Como en tinytuya, al final se borran **todos** los espacios, también los de adentro de los
//!   textos.

use serde_json::Value;

/// Nodo JSON con orden de claves explícito.
#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    /// Un valor cualquiera de `serde_json`.
    Value(Value),
    /// Objeto con las claves en este orden.
    Object(Vec<(String, Json)>),
}

impl From<Value> for Json {
    fn from(value: Value) -> Self {
        Self::Value(value)
    }
}

/// Serializa como `json.dumps(obj).replace(" ", "")`.
pub fn dumps_compact(node: &Json) -> String {
    let mut out = String::new();
    write_node(node, &mut out);
    out.retain(|c| c != ' ');
    out
}

/// Serializa como `json.dumps(obj)` (separadores `", "` y `": "`), sin borrar espacios.
pub fn dumps_spaced(node: &Json) -> String {
    let mut out = String::new();
    write_node_spaced(node, &mut out);
    out
}

fn write_node(node: &Json, out: &mut String) {
    match node {
        Json::Value(v) => write_value(v, out, ",", ":"),
        Json::Object(entries) => write_object(entries, out, ",", ":"),
    }
}

fn write_node_spaced(node: &Json, out: &mut String) {
    match node {
        Json::Value(v) => write_value(v, out, ", ", ": "),
        Json::Object(entries) => write_object(entries, out, ", ", ": "),
    }
}

fn write_object(entries: &[(String, Json)], out: &mut String, item_sep: &str, key_sep: &str) {
    out.push('{');
    for (i, (k, v)) in entries.iter().enumerate() {
        if i > 0 {
            out.push_str(item_sep);
        }
        write_str(k, out);
        out.push_str(key_sep);
        match v {
            Json::Value(v) => write_value(v, out, item_sep, key_sep),
            Json::Object(e) => write_object(e, out, item_sep, key_sep),
        }
    }
    out.push('}');
}

fn write_value(v: &Value, out: &mut String, item_sep: &str, key_sep: &str) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                out.push_str(&i.to_string());
            } else if let Some(u) = n.as_u64() {
                out.push_str(&u.to_string());
            } else if let Some(f) = n.as_f64() {
                out.push_str(&float_repr(f));
            }
        }
        Value::String(s) => write_str(s, out),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(item_sep);
                }
                write_value(item, out, item_sep, key_sep);
            }
            out.push(']');
        }
        Value::Object(map) => {
            out.push('{');
            for (i, (k, item)) in map.iter().enumerate() {
                if i > 0 {
                    out.push_str(item_sep);
                }
                write_str(k, out);
                out.push_str(key_sep);
                write_value(item, out, item_sep, key_sep);
            }
            out.push('}');
        }
    }
}

fn write_str(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            ' '..='~' => out.push(c),
            _ => {
                let mut buf = [0u16; 2];
                for unit in c.encode_utf16(&mut buf) {
                    out.push_str(&format!("\\u{unit:04x}"));
                }
            }
        }
    }
    out.push('"');
}

/// `repr(float)` de Python, que es lo que usa `json.dumps`.
pub fn float_repr(f: f64) -> String {
    if f.is_nan() {
        return "NaN".into();
    }
    if f.is_infinite() {
        return if f > 0.0 {
            "Infinity".into()
        } else {
            "-Infinity".into()
        };
    }
    if f == 0.0 {
        return if f.is_sign_negative() {
            "-0.0".into()
        } else {
            "0.0".into()
        };
    }
    // `{:e}` da los dígitos más cortos que reconstruyen el valor, igual que Python.
    let sci = format!("{f:e}");
    let (mantissa, exp) = sci.split_once('e').unwrap_or((&sci, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let (neg, mantissa) = match mantissa.strip_prefix('-') {
        Some(m) => (true, m),
        None => (false, mantissa),
    };
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let mut out = String::new();
    if neg {
        out.push('-');
    }
    if (-4..16).contains(&exp) {
        // Notación fija.
        if exp >= 0 {
            let int_len = (exp + 1) as usize;
            if digits.len() <= int_len {
                out.push_str(&digits);
                out.extend(std::iter::repeat_n('0', int_len - digits.len()));
                out.push_str(".0");
            } else {
                out.push_str(&digits[..int_len]);
                out.push('.');
                out.push_str(&digits[int_len..]);
            }
        } else {
            out.push_str("0.");
            out.extend(std::iter::repeat_n('0', (-exp - 1) as usize));
            out.push_str(&digits);
        }
    } else {
        out.push_str(&digits[..1]);
        if digits.len() > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        out.push('e');
        out.push(if exp < 0 { '-' } else { '+' });
        out.push_str(&format!("{:02}", exp.abs()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn floats_como_repr_de_python() {
        let casos = [
            (1.0, "1.0"),
            (0.5, "0.5"),
            (-2.25, "-2.25"),
            (100.0, "100.0"),
            (0.0001, "0.0001"),
            (0.00001, "1e-05"),
            (1.5e-7, "1.5e-07"),
            (1e16, "1e+16"),
            (1.2345e16, "1.2345e+16"),
            (9999999999999998.0, "9999999999999998.0"),
            (0.1 + 0.2, "0.30000000000000004"),
            (123456.789, "123456.789"),
            (1e300, "1e+300"),
            (-0.0, "-0.0"),
            (0.0, "0.0"),
        ];
        for (f, esperado) in casos {
            assert_eq!(float_repr(f), esperado, "{f}");
        }
    }

    #[test]
    fn textos_con_ensure_ascii_y_sin_espacios() {
        let node = Json::Object(vec![
            ("a b".into(), json!("ñ é \"x\" \\ \n 😀 \u{7f}").into()),
            ("n".into(), json!([1, -2, 1.5, null, true]).into()),
        ]);
        assert_eq!(
            dumps_compact(&node),
            r#"{"ab":"\u00f1\u00e9\"x\"\\\n\ud83d\ude00\u007f","n":[1,-2,1.5,null,true]}"#
        );
        assert_eq!(
            dumps_spaced(&Json::Object(vec![
                ("from".into(), json!("app").into()),
                ("ip".into(), json!("1.2.3.4").into())
            ])),
            r#"{"from": "app", "ip": "1.2.3.4"}"#
        );
    }
}
