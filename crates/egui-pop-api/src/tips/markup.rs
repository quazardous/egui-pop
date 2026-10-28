//! A tip's text: `{key}` placeholders resolved to the key bindings in
//! force, then `**bold**` and `` `code` `` drawn.

use egui::text::LayoutJob;
use egui::{Align, FontSelection, RichText, Style};

/// The commands a text names in `{key:<command>}` placeholders, in
/// order, each once.
pub fn named_commands(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (inner, _) in placeholders(text) {
        if let Some(c) = inner.strip_prefix("key:") {
            let c = c.trim().to_string();
            if !out.contains(&c) {
                out.push(c);
            }
        }
    }
    out
}

/// Whether a text has a bare `{key}` (its own command's binding).
pub fn has_own_key(text: &str) -> bool {
    placeholders(text).iter().any(|(inner, _)| inner == "key")
}

/// The `{...}` placeholders of a text: what is inside, and where the
/// whole placeholder is.
fn placeholders(text: &str) -> Vec<(String, std::ops::Range<usize>)> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(open) = text[from..].find('{').map(|i| i + from) {
        let Some(close) = text[open..].find('}').map(|i| i + open) else {
            break;
        };
        let inner = text[open + 1..close].trim();
        if inner == "key" || inner.starts_with("key:") {
            out.push((inner.to_string(), open..close + 1));
        }
        from = close + 1;
    }
    out
}

/// Replaces `{key}` with `own`'s text and `{key:<command>}` with
/// `other(<command>)`'s; a placeholder without an answer stays as it
/// is.
pub fn resolve(
    text: &str,
    own: Option<&str>,
    mut other: impl FnMut(&str) -> Option<String>,
) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for (inner, range) in placeholders(text) {
        let with = if inner == "key" {
            own.map(str::to_string)
        } else {
            inner.strip_prefix("key:").and_then(|c| other(c.trim()))
        };
        if let Some(with) = with {
            out.push_str(&text[last..range.start]);
            out.push_str(&with);
            last = range.end;
        }
    }
    out.push_str(&text[last..]);
    out
}

/// A run of text: plain, bold or code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Run {
    Plain,
    Bold,
    Code,
}

/// The text cut into runs (`**bold**`, `` `code` ``; an unclosed mark
/// is plain text).
pub fn runs(text: &str) -> Vec<(Run, String)> {
    let mut out: Vec<(Run, String)> = Vec::new();
    let mut plain = String::new();
    let mut rest = text;
    while !rest.is_empty() {
        let (mark, kind) = if rest.starts_with("**") {
            ("**", Run::Bold)
        } else if rest.starts_with('`') {
            ("`", Run::Code)
        } else {
            let c = rest.chars().next().unwrap_or_default();
            plain.push(c);
            rest = &rest[c.len_utf8()..];
            continue;
        };
        let after = &rest[mark.len()..];
        match after.find(mark) {
            Some(end) if end > 0 => {
                if !plain.is_empty() {
                    out.push((Run::Plain, std::mem::take(&mut plain)));
                }
                out.push((kind, after[..end].to_string()));
                rest = &after[end + mark.len()..];
            }
            _ => {
                plain.push_str(mark);
                rest = after;
            }
        }
    }
    if !plain.is_empty() {
        out.push((Run::Plain, plain));
    }
    out
}

/// The text laid out in a style: bold strong, code in the code style,
/// wrapping in the width given.
pub fn job(text: &str, style: &Style, width: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    for (kind, run) in runs(text) {
        let rich = RichText::new(run);
        let rich = match kind {
            Run::Plain => rich,
            Run::Bold => rich.strong(),
            Run::Code => rich.code(),
        };
        rich.append_to(&mut job, style, FontSelection::Default, Align::Center);
    }
    job.wrap.max_width = width;
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_take_the_keys_in_force() {
        let text = "Press {key}, or {key:app.palette} then {key:none}.";
        let out = resolve(text, Some("`R`"), |c| {
            (c == "app.palette").then(|| "`Ctrl+Shift+P`".to_string())
        });
        assert_eq!(out, "Press `R`, or `Ctrl+Shift+P` then {key:none}.");
        assert_eq!(named_commands(text), ["app.palette", "none"]);
        assert!(has_own_key(text));
        assert_eq!(resolve("{key}", None, |_| None), "{key}");
    }

    #[test]
    fn bold_and_code_are_runs_and_an_unclosed_mark_is_text() {
        assert_eq!(
            runs("Press **0** or `R`, a * and a ` stay."),
            [
                (Run::Plain, "Press ".to_string()),
                (Run::Bold, "0".to_string()),
                (Run::Plain, " or ".to_string()),
                (Run::Code, "R".to_string()),
                (Run::Plain, ", a * and a ` stay.".to_string()),
            ]
        );
    }
}
