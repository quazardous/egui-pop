//! The two values YAML reads differently from what they look like when
//! left unquoted: `#0a0a0a` (a comment) and `{surface.panel}` (a map).
//! No dependency: `build.rs` includes this file too, so a shipped token
//! file with a trap fails the build.

/// The first unquoted value YAML would misread, with its line (from 1).
pub(crate) fn trap(text: &str) -> Option<(usize, String)> {
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
