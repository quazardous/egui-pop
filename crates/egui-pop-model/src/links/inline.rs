//! The inline subset for short texts (ADR 0017): tips, toasts and
//! settings descriptions may carry emphasis, code and links without a
//! markdown renderer. A text is cut into [`Run`]s, each drawn as it says;
//! nothing else of markdown is read (no headings, lists, images or
//! HTML: they stay as they are written).
//!
//! | Written | Drawn |
//! |---|---|
//! | `*text*`, `_text_` | emphasis |
//! | `**text**`, `__text__` | strong |
//! | `` `code` `` | code |
//! | `[label](target)` | a link showing `label` |
//! | `<https://…>`, a bare `https://…` | a link showing its address |
//! | `\*` | a literal `*` (any ASCII punctuation) |
//!
//! A delimiter opens only when a closing one follows (`2 * 3` stays as
//! it is), `_` never inside a word (`snake_case_name` stays as it is),
//! and a code span's text is taken verbatim.
//!
//! ```
//! use egui_pop_model::links::inline::{runs, Run};
//!
//! let r = runs("Press `Ctrl+K`, see [the guide](docs:guide).");
//! assert_eq!(r[1], Run { text: "Ctrl+K".into(), code: true, ..Run::default() });
//! assert_eq!(r[3].link.as_deref(), Some("docs:guide"));
//! assert_eq!(runs("snake_case_name").len(), 1);
//! ```

/// A piece of a short text, drawn one way.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Run {
    pub text: String,
    pub strong: bool,
    pub emphasis: bool,
    pub code: bool,
    /// A link's target: the run is its label.
    pub link: Option<String>,
}

/// Whether a text uses the subset at all: one without it is drawn as a
/// plain label.
pub fn has_markup(text: &str) -> bool {
    text.contains(['*', '_', '`', '[', '<', '\\'])
        || text.contains("https://")
        || text.contains("http://")
}

/// The runs of a short text, in order; neighbours drawn the same way are
/// merged.
pub fn runs(text: &str) -> Vec<Run> {
    let mut out = Vec::new();
    parse(text, false, false, &mut out);
    merge(out)
}

/// The text without its markup (a link's label, a code span's text), for
/// a screen reader or a search.
pub fn plain(text: &str) -> String {
    runs(text).into_iter().map(|r| r.text).collect()
}

fn merge(runs: Vec<Run>) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::with_capacity(runs.len());
    for r in runs {
        if r.text.is_empty() {
            continue;
        }
        match out.last_mut() {
            Some(last)
                if last.link.is_none()
                    && r.link.is_none()
                    && (last.strong, last.emphasis, last.code)
                        == (r.strong, r.emphasis, r.code) =>
            {
                last.text.push_str(&r.text);
            }
            _ => out.push(r),
        }
    }
    out
}

struct Builder<'a> {
    out: &'a mut Vec<Run>,
    buf: String,
    strong: bool,
    emphasis: bool,
}

impl Builder<'_> {
    fn flush(&mut self) {
        if !self.buf.is_empty() {
            self.out.push(Run {
                text: std::mem::take(&mut self.buf),
                strong: self.strong,
                emphasis: self.emphasis,
                ..Run::default()
            });
        }
    }

    fn push(&mut self, run: Run) {
        self.flush();
        self.out.push(run);
    }
}

fn parse(text: &str, strong: bool, emphasis: bool, out: &mut Vec<Run>) {
    let mut b = Builder {
        out,
        buf: String::new(),
        strong,
        emphasis,
    };
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < text.len() {
        let rest = &text[i..];
        let c = rest.chars().next().unwrap_or_default();
        match c {
            '\\' => {
                let next = rest[1..].chars().next();
                match next {
                    Some(n) if n.is_ascii_punctuation() => {
                        b.buf.push(n);
                        i += 2;
                    }
                    _ => {
                        b.buf.push('\\');
                        i += 1;
                    }
                }
                continue;
            }
            '`' => {
                if let Some(end) = rest[1..].find('`') {
                    let code = &rest[1..1 + end];
                    if !code.is_empty() {
                        b.push(Run {
                            text: code.to_string(),
                            code: true,
                            ..Run::default()
                        });
                        i += end + 2;
                        continue;
                    }
                }
            }
            '[' => {
                if let Some((label, target, used)) = markdown_link(rest) {
                    b.push(Run {
                        text: label.to_string(),
                        strong,
                        emphasis,
                        link: Some(target.to_string()),
                        ..Run::default()
                    });
                    i += used;
                    continue;
                }
            }
            '<' => {
                if let Some(end) = rest.find('>') {
                    let inner = &rest[1..end];
                    if is_autolink(inner) {
                        b.push(Run {
                            text: inner.to_string(),
                            strong,
                            emphasis,
                            link: Some(inner.to_string()),
                            ..Run::default()
                        });
                        i += end + 1;
                        continue;
                    }
                }
            }
            'h' | 'H' => {
                let before = text[..i].chars().next_back();
                if before.is_none_or(|p| !p.is_alphanumeric())
                    && let Some(len) = bare_url(rest)
                {
                    let url = &rest[..len];
                    b.push(Run {
                        text: url.to_string(),
                        strong,
                        emphasis,
                        link: Some(url.to_string()),
                        ..Run::default()
                    });
                    i += len;
                    continue;
                }
            }
            '*' | '_' => {
                let double = bytes.get(i + 1) == Some(&(c as u8));
                let width = if double { 2 } else { 1 };
                let before = text[..i].chars().next_back();
                if opens(c, before, &rest[width..])
                    && let Some(close) = closing(&rest[width..], c, double)
                {
                    let inner = &rest[width..width + close];
                    b.flush();
                    if double {
                        parse(inner, true, b.emphasis, b.out);
                    } else {
                        parse(inner, b.strong, true, b.out);
                    }
                    i += width + close + width;
                    continue;
                }
                // Not emphasis: the delimiters as written.
                b.buf.push_str(&rest[..width]);
                i += width;
                continue;
            }
            _ => {}
        }
        b.buf.push(c);
        i += c.len_utf8();
    }
    b.flush();
}

/// A delimiter opens when text follows it at once, and `_` only at the
/// start of a word.
fn opens(c: char, before: Option<char>, after: &str) -> bool {
    let next = after.chars().next();
    if next.is_none_or(char::is_whitespace) {
        return false;
    }
    !(c == '_' && before.is_some_and(char::is_alphanumeric))
}

/// Where the closing delimiter starts in `rest`: preceded by text, and
/// for `_` not followed by a letter or digit; a single one is not part
/// of a double.
fn closing(rest: &str, c: char, double: bool) -> Option<usize> {
    let bytes = rest.as_bytes();
    let d = c as u8;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == b'`'
            && let Some(end) = rest[i + 1..].find('`')
        {
            i += end + 2;
            continue;
        }
        if bytes[i] == d && i > 0 {
            let run = bytes[i..].iter().take_while(|b| **b == d).count();
            let fits = if double { run >= 2 } else { run == 1 };
            let before = rest[..i].chars().next_back();
            let after = rest[i + if double { 2 } else { 1 }..].chars().next();
            if fits
                && before.is_some_and(|p| !p.is_whitespace())
                && !(c == '_' && after.is_some_and(char::is_alphanumeric))
            {
                return Some(i);
            }
            i += run;
            continue;
        }
        i += 1;
    }
    None
}

/// `[label](target)` at the start of `text`: the label, the target and
/// the bytes used.
fn markdown_link(text: &str) -> Option<(&str, &str, usize)> {
    let close = text.find("](")?;
    let label = &text[1..close];
    if label.is_empty() || label.contains(['[', ']', '\n']) {
        return None;
    }
    let after = &text[close + 2..];
    let end = after.find(')')?;
    let target = after[..end].trim().trim_matches(['<', '>']);
    if target.is_empty() || target.contains(char::is_whitespace) {
        return None;
    }
    Some((label, target, close + 2 + end + 1))
}

fn is_autolink(inner: &str) -> bool {
    let Some((scheme, rest)) = inner.split_once(':') else {
        return false;
    };
    !rest.is_empty()
        && !inner.contains(char::is_whitespace)
        && scheme.len() >= 2
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// The length of a bare `http(s)://` address at the start of `text`,
/// without the punctuation that ends a sentence.
fn bare_url(text: &str) -> Option<usize> {
    let lower = text.get(..8).unwrap_or(text).to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        return None;
    }
    let end = text.find(char::is_whitespace).unwrap_or(text.len());
    let url = text[..end].trim_end_matches(['.', ',', ';', ':', '!', '?', ')', '"', '\'', '>']);
    (url.len() > "http://".len()).then_some(url.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(text: &str) -> Run {
        Run {
            text: text.into(),
            ..Run::default()
        }
    }

    #[test]
    fn emphasis_strong_and_code() {
        assert_eq!(
            runs("a *b* **c** `d*e`"),
            vec![
                r("a "),
                Run {
                    emphasis: true,
                    ..r("b")
                },
                r(" "),
                Run {
                    strong: true,
                    ..r("c")
                },
                r(" "),
                Run {
                    code: true,
                    ..r("d*e")
                },
            ]
        );
        assert_eq!(
            runs("**bold _and_ more**")[1],
            Run {
                strong: true,
                emphasis: true,
                ..r("and")
            }
        );
    }

    #[test]
    fn delimiters_without_a_pair_or_inside_words_stay() {
        assert_eq!(runs("2 * 3 = 6"), vec![r("2 * 3 = 6")]);
        assert_eq!(runs("snake_case_name"), vec![r("snake_case_name")]);
        assert_eq!(runs("a ** b"), vec![r("a ** b")]);
        assert_eq!(runs(r"\*not\*"), vec![r("*not*")]);
        assert_eq!(runs("an `unclosed code"), vec![r("an `unclosed code")]);
    }

    #[test]
    fn links_of_every_form() {
        let found =
            runs("See [the guide](docs:guide), <mailto:a@b.example> or https://example.org/x.");
        let links: Vec<(&str, &str)> = found
            .iter()
            .filter_map(|r| Some((r.text.as_str(), r.link.as_deref()?)))
            .collect();
        assert_eq!(
            links,
            [
                ("the guide", "docs:guide"),
                ("mailto:a@b.example", "mailto:a@b.example"),
                ("https://example.org/x", "https://example.org/x"),
            ]
        );
        assert_eq!(found.last().unwrap(), &r("."));
        assert_eq!(plain("See [the guide](docs:guide)."), "See the guide.");
        assert!(!has_markup("Plain words."));
    }
}
