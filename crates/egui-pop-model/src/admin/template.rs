//! An action's argument template: a JSON value whose strings may name a
//! form's values. A string that is exactly `${name}` becomes that value
//! (any JSON); `${name}` inside a longer string is replaced by its text.
//! `${row.<key>}` and `${selection}` are names like any other: the
//! caller puts them in the values.

use serde_json::Value as Json;

use super::hooks::Values;

/// Fills a template from the values; a name with no value stays as
/// written, and is returned in the second part.
pub fn expand(template: &Json, values: &Values) -> (Json, Vec<String>) {
    let mut missing = Vec::new();
    let out = walk(template, values, &mut missing);
    (out, missing)
}

fn walk(t: &Json, values: &Values, missing: &mut Vec<String>) -> Json {
    match t {
        Json::String(s) => {
            if let Some(name) = s.strip_prefix("${").and_then(|r| r.strip_suffix('}'))
                && !name.contains("${")
            {
                return match values.get(name) {
                    Some(v) => v.clone(),
                    None => {
                        missing.push(name.into());
                        t.clone()
                    }
                };
            }
            let mut out = String::new();
            let mut rest = s.as_str();
            while let Some(start) = rest.find("${") {
                out.push_str(&rest[..start]);
                let after = &rest[start + 2..];
                match after.find('}') {
                    Some(end) => {
                        let name = &after[..end];
                        match values.get(name) {
                            Some(Json::String(v)) => out.push_str(v),
                            Some(v) => out.push_str(&v.to_string()),
                            None => {
                                missing.push(name.into());
                                out.push_str(&rest[start..start + 3 + end]);
                            }
                        }
                        rest = &after[end + 1..];
                    }
                    None => {
                        out.push_str(&rest[start..]);
                        rest = "";
                    }
                }
            }
            out.push_str(rest);
            Json::String(out)
        }
        Json::Array(a) => Json::Array(a.iter().map(|x| walk(x, values, missing)).collect()),
        Json::Object(o) => Json::Object(
            o.iter()
                .map(|(k, v)| (k.clone(), walk(v, values, missing)))
                .collect(),
        ),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn templates_take_values_whole_or_as_text() {
        let mut v = Values::new();
        v.insert("copies".into(), json!(2));
        v.insert("row.id".into(), json!("disk-a"));
        let (out, missing) = expand(
            &json!({"n": "${copies}", "volume": "${row.id}", "label": "Copy ${copies} to ${row.id}", "x": ["${none}", 3]}),
            &v,
        );
        assert_eq!(
            out,
            json!({"n": 2, "volume": "disk-a", "label": "Copy 2 to disk-a", "x": ["${none}", 3]})
        );
        assert_eq!(missing, vec!["none".to_string()]);
    }
}
