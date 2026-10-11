//! Token values: the DTCG types this crate reads, from their JSON form.

use serde_json::{Value as Json, json};

use crate::Color;

/// Pixels in one `rem`, for dimensions given in `rem`.
pub const PX_PER_REM: f32 = 16.0;

/// A token's value, resolved and typed.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Value {
    /// `color`.
    Color(Color),
    /// `dimension`, in logical pixels.
    Dimension(f32),
    /// `number`.
    Number(f32),
    /// `duration`, in milliseconds.
    Duration(f32),
    /// `fontFamily`: families, the first preferred.
    FontFamily(Vec<String>),
    /// A type this crate does not interpret (`shadow`, `typography`, a
    /// tool's own): the raw JSON value, kept as written.
    Other(Json),
}

impl Value {
    /// Reads a raw value (aliases already followed) as the given DTCG type.
    pub fn parse(ty: &str, raw: &Json) -> Result<Value, String> {
        match ty {
            "color" => parse_color(raw).map(Value::Color),
            "dimension" => parse_dimension(raw).map(Value::Dimension),
            "number" => raw
                .as_f64()
                .map(|v| Value::Number(v as f32))
                .ok_or_else(|| format!("`{raw}` is not a number")),
            "duration" => parse_duration(raw).map(Value::Duration),
            "fontFamily" => match raw {
                Json::String(s) => Ok(Value::FontFamily(vec![s.clone()])),
                Json::Array(a) => a
                    .iter()
                    .map(|f| {
                        f.as_str()
                            .map(String::from)
                            .ok_or_else(|| format!("`{f}` is not a font family name"))
                    })
                    .collect::<Result<_, _>>()
                    .map(Value::FontFamily),
                _ => Err(format!("`{raw}` is not a font family or a list of them")),
            },
            _ => Ok(Value::Other(raw.clone())),
        }
    }

    /// The value back in DTCG form (colours as hex strings, dimensions and
    /// durations as objects).
    pub fn to_json(&self) -> Json {
        match self {
            Value::Color(c) => json!(c.to_hex()),
            Value::Dimension(d) => json!({ "value": d, "unit": "px" }),
            Value::Number(n) => json!(n),
            Value::Duration(ms) => json!({ "value": ms, "unit": "ms" }),
            Value::FontFamily(f) => json!(f),
            Value::Other(raw) => raw.clone(),
        }
    }

    /// A short human form: `#f0c060`, `4px`, `120ms`.
    pub fn display(&self) -> String {
        match self {
            Value::Color(c) => c.to_hex(),
            Value::Dimension(d) => format!("{d}px"),
            Value::Number(n) => format!("{n}"),
            Value::Duration(ms) => format!("{ms}ms"),
            Value::FontFamily(f) => f.join(", "),
            Value::Other(raw) => raw.to_string(),
        }
    }
}

fn parse_color(raw: &Json) -> Result<Color, String> {
    let bad = |why: &str| format!("`{raw}` is not a color: {why}");
    match raw {
        // Hex strings: the format before DTCG 2025.10, still widely used.
        Json::String(s) => Color::from_hex(s).ok_or_else(|| bad("expected #rrggbb or #rrggbbaa")),
        Json::Object(o) => {
            let alpha = match o.get("alpha") {
                None => 1.0,
                Some(a) => a.as_f64().ok_or_else(|| bad("alpha is not a number"))? as f32,
            };
            let space = o.get("colorSpace").and_then(Json::as_str);
            let components = o.get("components").and_then(Json::as_array);
            let read = || -> Result<[f32; 3], String> {
                let c = components
                    .filter(|c| c.len() == 3)
                    .ok_or_else(|| bad("expected three components"))?;
                let mut out = [0.0; 3];
                for (slot, v) in out.iter_mut().zip(c) {
                    // `none` (a missing component) counts as zero.
                    *slot = match v {
                        Json::String(s) if s == "none" => 0.0,
                        _ => v
                            .as_f64()
                            .ok_or_else(|| bad("a component is not a number"))?
                            as f32,
                    };
                }
                Ok(out)
            };
            let from_space = match space {
                Some("srgb") => Some(read().map(|[r, g, b]| Color { r, g, b, a: alpha })),
                Some("srgb-linear") => {
                    Some(read().map(|[r, g, b]| Color::from_linear(r, g, b, alpha)))
                }
                Some("oklch") => Some(read().map(|[l, c, h]| Color::from_oklch(l, c, h, alpha))),
                Some("oklab") => Some(read().map(|[l, a, b]| {
                    Color::from_oklab(f64::from(l), f64::from(a), f64::from(b), alpha)
                })),
                _ => None,
            };
            match from_space {
                Some(color) => color,
                // A colour space not read here: the `hex` fallback the
                // format provides for such readers.
                None => o
                    .get("hex")
                    .and_then(Json::as_str)
                    .and_then(Color::from_hex)
                    .map(|c| Color { a: alpha, ..c })
                    .ok_or_else(|| {
                        bad(&format!(
                            "color space {} is not read and no hex fallback is given",
                            space.unwrap_or("(none)")
                        ))
                    }),
            }
        }
        _ => Err(bad("expected a string or an object")),
    }
}

fn parse_dimension(raw: &Json) -> Result<f32, String> {
    let unit = |value: f64, unit: &str| match unit {
        "px" => Ok(value as f32),
        "rem" => Ok(value as f32 * PX_PER_REM),
        other => Err(format!("`{raw}`: unit `{other}` is not px or rem")),
    };
    match raw {
        Json::Object(o) => {
            let v = o
                .get("value")
                .and_then(Json::as_f64)
                .ok_or_else(|| format!("`{raw}` is not a dimension"))?;
            unit(v, o.get("unit").and_then(Json::as_str).unwrap_or(""))
        }
        // `"4px"`: the format before DTCG 2025.10.
        Json::String(s) => {
            let split = s
                .find(|c: char| c.is_ascii_alphabetic())
                .ok_or_else(|| format!("`{raw}` has no unit"))?;
            let v: f64 = s[..split]
                .trim()
                .parse()
                .map_err(|_| format!("`{raw}` is not a dimension"))?;
            unit(v, &s[split..])
        }
        _ => Err(format!("`{raw}` is not a dimension")),
    }
}

fn parse_duration(raw: &Json) -> Result<f32, String> {
    let unit = |value: f64, unit: &str| match unit {
        "ms" => Ok(value as f32),
        "s" => Ok(value as f32 * 1000.0),
        other => Err(format!("`{raw}`: unit `{other}` is not ms or s")),
    };
    match raw {
        Json::Object(o) => {
            let v = o
                .get("value")
                .and_then(Json::as_f64)
                .ok_or_else(|| format!("`{raw}` is not a duration"))?;
            unit(v, o.get("unit").and_then(Json::as_str).unwrap_or(""))
        }
        Json::String(s) => {
            let split = s
                .find(|c: char| c.is_ascii_alphabetic())
                .ok_or_else(|| format!("`{raw}` has no unit"))?;
            let v: f64 = s[..split]
                .trim()
                .parse()
                .map_err(|_| format!("`{raw}` is not a duration"))?;
            unit(v, &s[split..])
        }
        _ => Err(format!("`{raw}` is not a duration")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_in_every_accepted_form() {
        let c = |raw: Json| Value::parse("color", &raw);
        let gold = Value::Color(Color::from_hex("#f0c060").unwrap());
        assert_eq!(c(json!("#f0c060")), Ok(gold.clone()));
        assert_eq!(
            c(json!({"colorSpace": "srgb", "components": [1, 0, 0], "alpha": 0.5})),
            Ok(Value::Color(Color {
                a: 0.5,
                ..Color::rgb(1.0, 0.0, 0.0)
            }))
        );
        let Ok(Value::Color(oklch)) =
            c(json!({"colorSpace": "oklch", "components": [0.832, 0.126, 83]}))
        else {
            panic!("oklch is read");
        };
        assert_eq!(oklch.to_hex(), "#f0c060");
        assert_eq!(
            c(json!({"colorSpace": "display-p3", "components": [1, 0, 0], "hex": "#f0c060"})),
            Ok(gold)
        );
        assert!(c(json!({"colorSpace": "display-p3", "components": [1, 0, 0]})).is_err());
        assert!(c(json!(12)).is_err());
    }

    #[test]
    fn dimensions_and_durations() {
        assert_eq!(
            Value::parse("dimension", &json!({"value": 4, "unit": "px"})),
            Ok(Value::Dimension(4.0))
        );
        assert_eq!(
            Value::parse("dimension", &json!({"value": 0.5, "unit": "rem"})),
            Ok(Value::Dimension(8.0))
        );
        assert_eq!(
            Value::parse("dimension", &json!("6px")),
            Ok(Value::Dimension(6.0))
        );
        assert!(Value::parse("dimension", &json!("6em")).is_err());
        assert_eq!(
            Value::parse("duration", &json!({"value": 0.2, "unit": "s"})),
            Ok(Value::Duration(200.0))
        );
        assert_eq!(
            Value::parse("duration", &json!("120ms")),
            Ok(Value::Duration(120.0))
        );
    }

    #[test]
    fn unknown_types_are_kept_raw() {
        let shadow = json!({"color": "#000000", "offsetX": "1px"});
        assert_eq!(Value::parse("shadow", &shadow), Ok(Value::Other(shadow)));
    }
}
