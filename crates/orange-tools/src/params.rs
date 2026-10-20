//! Typed access to the JSON params with defaults and clear errors.

use orange_core::{Error, Result};
use serde_json::Value;

pub fn f64_or(p: &Value, key: &str, default: f64) -> f64 {
    match p.get(key) {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(default),
        Some(Value::String(s)) => s.trim().parse().unwrap_or(default),
        _ => default,
    }
}

pub fn i64_or(p: &Value, key: &str, default: i64) -> i64 {
    f64_or(p, key, default as f64).round() as i64
}

pub fn bool_or(p: &Value, key: &str, default: bool) -> bool {
    match p.get(key) {
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => matches!(s.as_str(), "true" | "1" | "yes"),
        _ => default,
    }
}

pub fn str_or<'a>(p: &'a Value, key: &str, default: &'a str) -> &'a str {
    p.get(key).and_then(|v| v.as_str()).unwrap_or(default)
}

pub fn require_f64(p: &Value, key: &str) -> Result<f64> {
    match p.get(key) {
        Some(Value::Number(n)) => n.as_f64().ok_or_else(|| Error::BadParams(format!("'{key}' is not a number"))),
        Some(Value::String(s)) => s.trim().parse().map_err(|_| Error::BadParams(format!("'{key}' is not a number"))),
        _ => Err(Error::BadParams(format!("missing '{key}'"))),
    }
}

/// Accepts `#rgb`, `#rrggbb`, `#rrggbbaa`, `rgb(...)`/`rgba(...)` and plain names. Rejects anything that
/// could be mistaken for an option or contains shell/ImageMagick-special characters.
pub fn color(p: &Value, key: &str, default: &str) -> Result<String> {
    let c = str_or(p, key, default);
    valid_color(c)
}

pub fn valid_color(c: &str) -> Result<String> {
    let c = c.trim();
    let ok = !c.is_empty()
        && c.len() <= 40
        && !c.starts_with('-')
        && c.chars().all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '#' | '(' | ')' | ',' | '.' | '%' | ' '));
    if ok {
        Ok(c.to_string())
    } else {
        Err(Error::BadParams(format!("invalid colour: {c:?}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn colors() {
        assert!(valid_color("#ff3b30").is_ok());
        assert!(valid_color("rgba(0,0,0,0.5)").is_ok());
        assert!(valid_color("-write").is_err());
        assert!(valid_color("a;b").is_err());
    }
    #[test]
    fn numbers() {
        let p = json!({"a": 3, "b": "4.5"});
        assert_eq!(f64_or(&p, "a", 0.0), 3.0);
        assert_eq!(f64_or(&p, "b", 0.0), 4.5);
        assert_eq!(f64_or(&p, "c", 7.0), 7.0);
    }
}
