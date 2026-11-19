//! A small JSON Schema checker for the arguments of a command or an
//! action: the subset that says what a call may carry, nothing more.
//!
//! Checked keywords: `type` (a name or a list; `integer` is a number
//! without a fraction), `enum`, `const`, `properties`, `required`,
//! `additionalProperties` (`false` or a schema), `items` (one schema),
//! `minItems`, `maxItems`, `minimum`, `maximum`, `exclusiveMinimum`,
//! `exclusiveMaximum` (numbers), `minLength`, `maxLength` (in
//! characters), `allOf`, `anyOf`, `oneOf` and `not`. Every other keyword
//! (`pattern`, `format`, `$ref`, `title`, `description`, `default`...)
//! is read as a note and not checked: a schema using them still checks
//! what it can.
//!
//! [`check`] answers the first mismatch, with where it is (`/factor`,
//! `/items/2`) and what was expected, in plain words.

use serde_json::Value as Json;

/// Whether `value` matches `schema`: `Err` names the first place it
/// does not (`/factor: expected a number, got a string`). A schema that
/// is not an object (`true`, `null`) accepts everything; `false` accepts
/// nothing.
pub fn check(schema: &Json, value: &Json) -> Result<(), String> {
    check_at(schema, value, "")
}

fn place(path: &str) -> &str {
    if path.is_empty() {
        "the arguments"
    } else {
        path
    }
}

fn type_name(v: &Json) -> &'static str {
    match v {
        Json::Null => "null",
        Json::Bool(_) => "a boolean",
        Json::Number(n) if is_integer(n) => "an integer",
        Json::Number(_) => "a number",
        Json::String(_) => "a string",
        Json::Array(_) => "an array",
        Json::Object(_) => "an object",
    }
}

fn is_integer(n: &serde_json::Number) -> bool {
    n.is_i64() || n.is_u64() || n.as_f64().is_some_and(|f| f.fract() == 0.0)
}

fn has_type(v: &Json, t: &str) -> bool {
    match (t, v) {
        ("null", Json::Null)
        | ("boolean", Json::Bool(_))
        | ("number", Json::Number(_))
        | ("string", Json::String(_))
        | ("array", Json::Array(_))
        | ("object", Json::Object(_)) => true,
        ("integer", Json::Number(n)) => is_integer(n),
        _ => false,
    }
}

fn article(t: &str) -> String {
    match t {
        "null" => "null".into(),
        "integer" | "array" | "object" => format!("an {t}"),
        _ => format!("a {t}"),
    }
}

fn short(v: &Json) -> String {
    let s = v.to_string();
    if s.chars().count() > 60 {
        format!("{}…", s.chars().take(60).collect::<String>())
    } else {
        s
    }
}

fn check_at(schema: &Json, v: &Json, path: &str) -> Result<(), String> {
    let s = match schema {
        Json::Bool(false) => return Err(format!("{}: nothing is allowed here", place(path))),
        Json::Object(s) => s,
        _ => return Ok(()),
    };
    let at = place(path);
    match s.get("type") {
        Some(Json::String(t)) if !has_type(v, t) => {
            return Err(format!(
                "{at}: expected {}, got {}",
                article(t),
                type_name(v)
            ));
        }
        Some(Json::Array(ts)) => {
            let names: Vec<&str> = ts.iter().filter_map(Json::as_str).collect();
            if !names.iter().any(|t| has_type(v, t)) {
                let list: Vec<String> = names.iter().map(|t| article(t)).collect();
                return Err(format!(
                    "{at}: expected {}, got {}",
                    list.join(" or "),
                    type_name(v)
                ));
            }
        }
        _ => {}
    }
    if let Some(Json::Array(allowed)) = s.get("enum")
        && !allowed.contains(v)
    {
        let list: Vec<String> = allowed.iter().map(short).collect();
        return Err(format!(
            "{at}: {} is not one of {}",
            short(v),
            list.join(", ")
        ));
    }
    if let Some(c) = s.get("const")
        && c != v
    {
        return Err(format!("{at}: expected {}, got {}", short(c), short(v)));
    }
    match v {
        Json::Number(n) => {
            let x = n.as_f64().unwrap_or(f64::NAN);
            let bound = |k: &str| s.get(k).and_then(Json::as_f64);
            if let Some(m) = bound("minimum")
                && x < m
            {
                return Err(format!("{at}: {n} is below the minimum {m}"));
            }
            if let Some(m) = bound("maximum")
                && x > m
            {
                return Err(format!("{at}: {n} is above the maximum {m}"));
            }
            if let Some(m) = bound("exclusiveMinimum")
                && x <= m
            {
                return Err(format!("{at}: {n} must be above {m}"));
            }
            if let Some(m) = bound("exclusiveMaximum")
                && x >= m
            {
                return Err(format!("{at}: {n} must be below {m}"));
            }
        }
        Json::String(text) => {
            let len = text.chars().count() as u64;
            if let Some(m) = s.get("minLength").and_then(Json::as_u64)
                && len < m
            {
                return Err(format!("{at}: shorter than {m} characters"));
            }
            if let Some(m) = s.get("maxLength").and_then(Json::as_u64)
                && len > m
            {
                return Err(format!("{at}: longer than {m} characters"));
            }
        }
        Json::Array(items) => {
            let len = items.len() as u64;
            if let Some(m) = s.get("minItems").and_then(Json::as_u64)
                && len < m
            {
                return Err(format!("{at}: fewer than {m} items"));
            }
            if let Some(m) = s.get("maxItems").and_then(Json::as_u64)
                && len > m
            {
                return Err(format!("{at}: more than {m} items"));
            }
            if let Some(item) = s.get("items") {
                for (i, x) in items.iter().enumerate() {
                    check_at(item, x, &format!("{path}/{i}"))?;
                }
            }
        }
        Json::Object(o) => {
            if let Some(Json::Array(req)) = s.get("required") {
                for k in req.iter().filter_map(Json::as_str) {
                    if !o.contains_key(k) {
                        return Err(format!("{at}: `{k}` is required"));
                    }
                }
            }
            let props = s.get("properties").and_then(Json::as_object);
            for (k, x) in o {
                let sub = format!("{path}/{k}");
                match props.and_then(|p| p.get(k)) {
                    Some(ps) => check_at(ps, x, &sub)?,
                    None => match s.get("additionalProperties") {
                        Some(Json::Bool(false)) => {
                            return Err(format!("{at}: `{k}` is not expected"));
                        }
                        Some(extra) => check_at(extra, x, &sub)?,
                        None => {}
                    },
                }
            }
        }
        _ => {}
    }
    if let Some(Json::Array(all)) = s.get("allOf") {
        for sub in all {
            check_at(sub, v, path)?;
        }
    }
    if let Some(Json::Array(any)) = s.get("anyOf") {
        let mut first = None;
        if !any.iter().any(|sub| match check_at(sub, v, path) {
            Ok(()) => true,
            Err(e) => {
                first.get_or_insert(e);
                false
            }
        }) {
            return Err(first.unwrap_or_else(|| format!("{at}: matches none of `anyOf`")));
        }
    }
    if let Some(Json::Array(one)) = s.get("oneOf") {
        let n = one
            .iter()
            .filter(|sub| check_at(sub, v, path).is_ok())
            .count();
        if n != 1 {
            return Err(format!(
                "{at}: matches {n} of `oneOf`'s schemas, exactly one expected"
            ));
        }
    }
    if let Some(not) = s.get("not")
        && check_at(not, v, path).is_ok()
    {
        return Err(format!("{at}: matches the schema it must not"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::check;
    use serde_json::json;

    fn zoom() -> serde_json::Value {
        json!({
            "type": "object",
            "required": ["factor"],
            "additionalProperties": false,
            "properties": {
                "factor": { "type": "number", "minimum": 0.1, "maximum": 8 },
                "mode": { "enum": ["fit", "fill"] },
                "tags": { "type": "array", "items": { "type": "string", "maxLength": 4 }, "maxItems": 2 },
                "count": { "type": "integer" },
                "name": { "type": ["string", "null"], "minLength": 1 }
            }
        })
    }

    #[test]
    fn a_matching_value_passes() {
        let s = zoom();
        assert_eq!(check(&s, &json!({ "factor": 2 })), Ok(()));
        assert_eq!(
            check(
                &s,
                &json!({ "factor": 0.5, "mode": "fit", "tags": ["a"], "count": 3.0, "name": null })
            ),
            Ok(())
        );
    }

    #[test]
    fn each_keyword_refuses_with_its_place() {
        let s = zoom();
        let err = |v| check(&s, &v).unwrap_err();
        assert_eq!(err(json!({})), "the arguments: `factor` is required");
        assert_eq!(
            err(json!(null)),
            "the arguments: expected an object, got null"
        );
        assert_eq!(
            err(json!({ "factor": "2" })),
            "/factor: expected a number, got a string"
        );
        assert_eq!(
            err(json!({ "factor": 9 })),
            "/factor: 9 is above the maximum 8"
        );
        assert_eq!(
            err(json!({ "factor": 0 })),
            "/factor: 0 is below the minimum 0.1"
        );
        assert_eq!(
            err(json!({ "factor": 1, "mode": "zoom" })),
            "/mode: \"zoom\" is not one of \"fit\", \"fill\""
        );
        assert_eq!(
            err(json!({ "factor": 1, "x": 1 })),
            "the arguments: `x` is not expected"
        );
        assert_eq!(
            err(json!({ "factor": 1, "tags": ["abcde"] })),
            "/tags/0: longer than 4 characters"
        );
        assert_eq!(
            err(json!({ "factor": 1, "tags": ["a", "b", "c"] })),
            "/tags: more than 2 items"
        );
        assert_eq!(
            err(json!({ "factor": 1, "count": 1.5 })),
            "/count: expected an integer, got a number"
        );
        assert_eq!(
            err(json!({ "factor": 1, "name": 3 })),
            "/name: expected a string or null, got an integer"
        );
        assert_eq!(
            err(json!({ "factor": 1, "name": "" })),
            "/name: shorter than 1 characters"
        );
    }

    #[test]
    fn combinators_and_unchecked_keywords() {
        let s = json!({ "anyOf": [{ "type": "string" }, { "type": "integer", "exclusiveMinimum": 0 }] });
        assert!(check(&s, &json!("a")).is_ok());
        assert!(check(&s, &json!(1)).is_ok());
        assert!(check(&s, &json!(0)).is_err());
        let one = json!({ "oneOf": [{ "type": "number" }, { "type": "integer" }] });
        assert!(check(&one, &json!(1.5)).is_ok());
        assert!(check(&one, &json!(1)).is_err());
        assert!(check(&json!({ "not": { "const": 3 } }), &json!(3)).is_err());
        // Read as notes: they do not refuse.
        assert!(
            check(
                &json!({ "type": "string", "pattern": "^x$", "format": "uri" }),
                &json!("y")
            )
            .is_ok()
        );
        assert!(check(&json!(true), &json!(1)).is_ok());
        assert!(check(&json!(false), &json!(1)).is_err());
    }
}
