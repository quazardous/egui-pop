//! Token files written in YAML: the same DTCG tree as JSON, with comments.
//!
//! YAML reads two token values differently from what they look like when
//! they are left unquoted: `#0a0a0a` starts a comment (the value is lost)
//! and `{surface.panel}` is a map (the alias is lost). Both are refused
//! here, with the line, before the file is parsed: every string that
//! starts with `#` or `{` is quoted (`"#0a0a0a"`, `"{surface.panel}"`).

use serde_json::Value as Json;

use crate::Error;

/// Whether a token file's text is JSON (its first character is `{`) or
/// YAML (anything else).
pub(crate) fn is_json(text: &str) -> bool {
    text.trim_start_matches('\u{feff}')
        .trim_start()
        .starts_with('{')
}

/// Reads a YAML token file into the tree a JSON one gives.
pub(crate) fn parse(file: &str, text: &str) -> Result<Json, Error> {
    if let Some((line, message)) = trap(text) {
        return Err(Error::Yaml {
            file: file.to_string(),
            message: format!("line {line}: {message}"),
        });
    }
    serde_saphyr::from_str::<Json>(text).map_err(|e| Error::Yaml {
        file: file.to_string(),
        message: e.to_string(),
    })
}

/// The first unquoted value YAML would misread, with its line (from 1).
fn trap(text: &str) -> Option<(usize, String)> {
    text.lines().enumerate().find_map(|(i, line)| {
        let value = plain_value(line)?;
        let message = match hex_colour(value) {
            Some(hex) => format!("`{hex}` unquoted is a YAML comment: write \"{hex}\""),
            None => {
                let alias = alias(value)?;
                format!("`{alias}` unquoted is a YAML map: write \"{alias}\"")
            }
        };
        Some((i + 1, message))
    })
}

/// What follows `key:` (or `- `) on a line, when it is not quoted and the
/// line is not a comment.
fn plain_value(line: &str) -> Option<&str> {
    let line = line.trim_start();
    if line.starts_with('#') {
        return None;
    }
    let rest = match line.strip_prefix("- ") {
        Some(item) => item.trim_start(),
        None => line,
    };
    let rest = match key_end(rest) {
        Some(at) => rest[at..].trim_start(),
        None if rest.len() < line.len() => rest,
        None => return None,
    };
    (!rest.starts_with(['"', '\''])).then_some(rest)
}

/// Where the value starts after a `key:` (past the colon), skipping a
/// quoted key.
fn key_end(text: &str) -> Option<usize> {
    let start = match text.chars().next()? {
        q @ ('"' | '\'') => text[1..].find(q)? + 2,
        _ => 0,
    };
    let colon = start
        + text[start..].find(": ").or_else(|| {
            let t = text[start..].trim_end();
            t.ends_with(':').then(|| t.len() - 1)
        })?;
    Some(colon + 1)
}

/// `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa` at the start of a value.
fn hex_colour(value: &str) -> Option<&str> {
    let word = value.split_whitespace().next()?;
    let digits = word.strip_prefix('#')?;
    (matches!(digits.len(), 3 | 4 | 6 | 8) && digits.chars().all(|c| c.is_ascii_hexdigit()))
        .then_some(word)
}

/// A whole-value reference such as `{surface.panel}`.
fn alias(value: &str) -> Option<&str> {
    let value = value.trim_end();
    let inner = value.strip_prefix('{')?.strip_suffix('}')?;
    (!inner.is_empty()
        && inner
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_')))
    .then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_or_yaml() {
        assert!(is_json("  {\"a\": 1}"));
        assert!(is_json("\u{feff}{}"));
        assert!(!is_json("# comment\na: 1"));
        assert!(!is_json("a: 1"));
    }

    #[test]
    fn quoted_values_read_as_written() {
        let text = "# a theme\nsurface:\n  panel:\n    $type: color\n    $value: \"#0a0a0a\" # dark\n  raised:\n    $value: '{surface.panel}'\n";
        let tree = parse("t", text).unwrap();
        assert_eq!(tree["surface"]["panel"]["$value"], "#0a0a0a");
        assert_eq!(tree["surface"]["raised"]["$value"], "{surface.panel}");
    }

    #[test]
    fn an_unquoted_colour_is_refused_with_its_line() {
        let err = parse("t", "a:\n  $value: #0a0a0a\n").unwrap_err();
        let text = err.to_string();
        assert!(text.contains("line 2"), "{text}");
        assert!(text.contains("\"#0a0a0a\""), "{text}");
    }

    #[test]
    fn an_unquoted_alias_is_refused_with_its_line() {
        let err = parse("t", "a:\n  b:\n    $value: {surface.panel}\n").unwrap_err();
        assert!(err.to_string().contains("line 3"), "{err}");
        let err = parse("t", "a:\n  - {gray.2}\n").unwrap_err();
        assert!(err.to_string().contains("line 2"), "{err}");
    }

    #[test]
    fn comments_and_maps_are_not_traps() {
        let text = "# #fff on its own line\nbutton: # the button\n  rest: {fill: \"#fff\"}\n";
        assert!(trap(text).is_none());
    }
}
