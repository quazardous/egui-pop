//! Token files: the DTCG JSON tree read into a flat map of tokens by path.

use std::collections::BTreeMap;

use serde_json::{Map, Value as Json};

use crate::Error;

/// A token as written in a file: its value may still be an alias.
#[derive(Clone, Debug, PartialEq)]
pub struct RawToken {
    /// `$type`, given on the token or inherited from a group.
    pub ty: Option<String>,
    /// `$value`, as written.
    pub value: Json,
    /// `$description`.
    pub description: Option<String>,
    /// `$extensions`, kept as written.
    pub extensions: Option<Json>,
}

impl RawToken {
    /// The path this token aliases, when its value is a whole-value
    /// reference such as `"{gray.2}"`.
    pub fn alias(&self) -> Option<&str> {
        alias_target(&self.value)
    }
}

/// The path a whole-value reference (`"{gray.2}"`) names.
pub fn alias_target(value: &Json) -> Option<&str> {
    let target = value.as_str()?.strip_prefix('{')?.strip_suffix('}')?;
    (!target.is_empty() && !target.contains(['{', '}'])).then_some(target)
}

/// A token file: a theme, a density overlay or any other layer.
///
/// Its tokens are keyed by their dotted path (`surface.panel`). At the
/// root, `$extends` names the theme this file builds on (every token not
/// given here comes from it), `$description` describes the file and
/// `$extensions` is kept as written.
#[derive(Clone, Debug, PartialEq)]
pub struct TokenFile {
    pub name: String,
    pub description: Option<String>,
    pub extends: Option<String>,
    /// The root `$extensions`, kept as written.
    pub extensions: Option<Json>,
    pub tokens: BTreeMap<String, RawToken>,
}

impl TokenFile {
    /// An empty file, to fill by hand.
    pub fn new(name: impl Into<String>) -> TokenFile {
        TokenFile {
            name: name.into(),
            description: None,
            extends: None,
            extensions: None,
            tokens: BTreeMap::new(),
        }
    }

    /// Reads a token file under the given name.
    pub fn parse(name: &str, text: &str) -> Result<TokenFile, Error> {
        let root: Json = serde_json::from_str(text).map_err(|e| Error::Json {
            file: name.to_string(),
            message: e.to_string(),
        })?;
        let obj = root.as_object().ok_or_else(|| Error::Malformed {
            file: name.to_string(),
            path: String::new(),
            reason: "the root is not a JSON object".into(),
        })?;
        let mut tokens = BTreeMap::new();
        walk(name, obj, "", None, &mut tokens)?;
        let extends = match obj.get("$extends") {
            None => None,
            Some(Json::String(s)) => Some(s.clone()),
            Some(other) => {
                return Err(Error::Malformed {
                    file: name.to_string(),
                    path: "$extends".into(),
                    reason: format!("`{other}` is not a theme name"),
                });
            }
        };
        Ok(TokenFile {
            name: name.to_string(),
            description: obj
                .get("$description")
                .and_then(Json::as_str)
                .map(String::from),
            extends,
            extensions: obj.get("$extensions").cloned(),
            tokens,
        })
    }
}

fn walk(
    file: &str,
    obj: &Map<String, Json>,
    prefix: &str,
    group_type: Option<&str>,
    out: &mut BTreeMap<String, RawToken>,
) -> Result<(), Error> {
    let own_type = obj.get("$type").and_then(Json::as_str).or(group_type);
    for (key, v) in obj {
        if key.starts_with('$') {
            continue;
        }
        let path = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        let malformed = |reason: &str| Error::Malformed {
            file: file.to_string(),
            path: path.clone(),
            reason: reason.to_string(),
        };
        if key.contains(['.', '{', '}']) {
            return Err(malformed("names may not contain `.`, `{` or `}`"));
        }
        let Some(child) = v.as_object() else {
            return Err(malformed("is neither a token (with $value) nor a group"));
        };
        if let Some(value) = child.get("$value") {
            out.insert(
                path,
                RawToken {
                    ty: child
                        .get("$type")
                        .and_then(Json::as_str)
                        .or(own_type)
                        .map(String::from),
                    value: value.clone(),
                    description: child
                        .get("$description")
                        .and_then(Json::as_str)
                        .map(String::from),
                    extensions: child.get("$extensions").cloned(),
                },
            );
        } else {
            walk(file, child, &path, own_type, out)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn groups_flatten_and_pass_their_type_down() {
        let f = TokenFile::parse(
            "t",
            r##"{
              "$description": "a test",
              "$extensions": { "org.example": { "kept": true } },
              "gray": { "$type": "color", "$description": "a scale",
                        "1": { "$value": "#111111", "$extensions": { "org.example": 1 } } },
              "space": { "s": { "$type": "dimension", "$value": "4px", "$description": "small" } },
              "alias": { "$value": "{gray.1}" }
            }"##,
        )
        .unwrap();
        assert_eq!(f.description.as_deref(), Some("a test"));
        assert_eq!(
            f.extensions,
            Some(json!({ "org.example": { "kept": true } }))
        );
        let gray = &f.tokens["gray.1"];
        assert_eq!(gray.ty.as_deref(), Some("color"));
        assert_eq!(gray.extensions, Some(json!({ "org.example": 1 })));
        assert_eq!(f.tokens["space.s"].description.as_deref(), Some("small"));
        assert_eq!(f.tokens["alias"].alias(), Some("gray.1"));
        assert_eq!(f.tokens["alias"].ty, None);
        assert_eq!(f.tokens.len(), 3);
    }

    #[test]
    fn malformed_files_say_where() {
        let err = TokenFile::parse("t", r#"{ "a": { "b": 3 } }"#).unwrap_err();
        assert!(
            matches!(err, Error::Malformed { ref path, .. } if path == "a.b"),
            "{err}"
        );
        assert!(matches!(
            TokenFile::parse("t", "[1]"),
            Err(Error::Malformed { .. })
        ));
        assert!(matches!(
            TokenFile::parse("t", "{"),
            Err(Error::Json { .. })
        ));
        assert!(matches!(
            TokenFile::parse("t", r#"{ "a.b": { "$value": 1 } }"#),
            Err(Error::Malformed { .. })
        ));
    }

    #[test]
    fn only_whole_value_references_are_aliases() {
        assert_eq!(alias_target(&json!("{a.b}")), Some("a.b"));
        assert_eq!(alias_target(&json!("{}")), None);
        assert_eq!(alias_target(&json!("x{a}")), None);
        assert_eq!(alias_target(&json!(3)), None);
    }
}
