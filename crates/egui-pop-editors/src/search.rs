//! Search and replace over a buffer: plain text or a regular
//! expression, with or without case, whole words or not. A match never
//! spans a line break.

use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

use crate::buffer::{Buffer, Pos, Span, col_at};

/// What to look for.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Query {
    pub text: String,
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default)]
    pub whole_word: bool,
    #[serde(default)]
    pub regex: bool,
}

impl Query {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }

    /// The query as a regular expression.
    pub fn compile(&self) -> Result<Regex, String> {
        let pattern = if self.regex {
            self.text.clone()
        } else {
            regex::escape(&self.text)
        };
        let pattern = if self.whole_word {
            format!(r"\b(?:{pattern})\b")
        } else {
            pattern
        };
        RegexBuilder::new(&pattern)
            .case_insensitive(!self.case_sensitive)
            .build()
            .map_err(|e| e.to_string())
    }
}

/// At most this many matches are listed.
pub const MAX_MATCHES: usize = 100_000;

/// Every match, in order; an empty query finds nothing.
pub fn find_all(buffer: &Buffer, query: &Query) -> Result<Vec<Span>, String> {
    if query.text.is_empty() {
        return Ok(Vec::new());
    }
    let re = query.compile()?;
    let mut out = Vec::new();
    for (i, line) in buffer.lines().iter().enumerate() {
        for m in re.find_iter(line) {
            if m.start() == m.end() {
                continue;
            }
            out.push(Span::new(
                Pos::new(i, col_at(line, m.start())),
                Pos::new(i, col_at(line, m.end())),
            ));
            if out.len() >= MAX_MATCHES {
                return Ok(out);
            }
        }
    }
    Ok(out)
}

/// What replaces a match: the replacement, with `$1` and `${name}`
/// expanded for a regular expression.
pub fn replacement_for(buffer: &Buffer, query: &Query, m: Span, with: &str) -> String {
    if !query.regex {
        return with.to_string();
    }
    let Ok(re) = query.compile() else {
        return with.to_string();
    };
    let text = buffer.slice(m);
    match re.captures(&text) {
        Some(caps) => {
            let mut out = String::new();
            caps.expand(with, &mut out);
            out
        }
        None => with.to_string(),
    }
}

/// The first match at or after `from` (wrapping around), its index.
pub fn next_from(matches: &[Span], from: Pos) -> Option<usize> {
    if matches.is_empty() {
        return None;
    }
    Some(matches.iter().position(|m| m.start >= from).unwrap_or(0))
}

/// The last match before `from` (wrapping around), its index.
pub fn previous_from(matches: &[Span], from: Pos) -> Option<usize> {
    if matches.is_empty() {
        return None;
    }
    Some(
        matches
            .iter()
            .rposition(|m| m.start < from)
            .unwrap_or(matches.len() - 1),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_case_words_and_regex() {
        let b = Buffer::from_text("Foo foo food\nfoo");
        assert_eq!(find_all(&b, &Query::new("foo")).unwrap().len(), 4);
        let q = Query {
            case_sensitive: true,
            ..Query::new("foo")
        };
        assert_eq!(find_all(&b, &q).unwrap().len(), 3);
        let q = Query {
            whole_word: true,
            ..Query::new("foo")
        };
        assert_eq!(find_all(&b, &q).unwrap().len(), 3);
        let q = Query {
            regex: true,
            ..Query::new(r"f(o+)d")
        };
        let m = find_all(&b, &q).unwrap();
        assert_eq!(m, vec![Span::new(Pos::new(0, 8), Pos::new(0, 12))]);
        assert_eq!(replacement_for(&b, &q, m[0], "[$1]"), "[oo]");
        assert!(
            find_all(
                &b,
                &Query {
                    regex: true,
                    ..Query::new("(")
                }
            )
            .is_err()
        );
        assert_eq!(next_from(&[], Pos::default()), None);
    }
}
