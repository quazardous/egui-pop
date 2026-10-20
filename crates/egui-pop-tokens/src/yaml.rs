//! Token files written in YAML: the same DTCG tree as JSON, with comments.
//!
//! YAML reads two token values differently from what they look like when
//! they are left unquoted: `#0a0a0a` starts a comment (the value is lost)
//! and `{surface.panel}` is a map (the alias is lost). Both are refused
//! here, with the line, before the file is parsed: every string that
//! starts with `#` or `{` is quoted (`"#0a0a0a"`, `"{surface.panel}"`).

use serde_json::Value as Json;

use crate::Error;
use crate::yaml_trap::trap;

/// Whether a token file's text is JSON (its first character is `{`) or
/// YAML (anything else).
pub(crate) fn is_json(text: &str) -> bool {
    text.trim_start_matches('\u{feff}')
        .trim_start()
        .starts_with('{')
}

#[cfg(test)]
thread_local! {
    static PARSES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// How many YAML files this thread parsed (to count cache hits in tests).
#[cfg(test)]
pub(crate) fn parses() -> usize {
    PARSES.with(std::cell::Cell::get)
}

/// Reads a YAML token file into the tree a JSON one gives.
pub(crate) fn parse(file: &str, text: &str) -> Result<Json, Error> {
    #[cfg(test)]
    PARSES.with(|n| n.set(n.get() + 1));
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
