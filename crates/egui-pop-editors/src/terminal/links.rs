//! Links in a terminal's text: `http://`, `https://` and `file://`
//! addresses, found in a row's characters.

use std::ops::Range;

/// The schemes a link starts with.
const SCHEMES: [&str; 3] = ["https://", "http://", "file://"];

/// A link in a row: the characters it covers and its address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    /// Indexes in the row's characters.
    pub chars: Range<usize>,
    pub url: String,
}

/// Whether a character may be part of an address.
fn in_url(c: char) -> bool {
    !c.is_whitespace() && !matches!(c, '"' | '\'' | '<' | '>' | '`' | '|')
}

/// The links of a row of characters. Trailing punctuation (`.`, `,`,
/// `;`, `:`, `!`, `?`) and a closing bracket without its opening one
/// are left out, as in `see (https://example.org).`
pub fn find(chars: &[char]) -> Vec<Link> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let rest: String = chars[i..chars.len().min(i + 8)].iter().collect();
        let Some(scheme) = SCHEMES.iter().find(|s| rest.starts_with(**s)) else {
            i += 1;
            continue;
        };
        let start = i;
        let mut end = i + scheme.len();
        while end < chars.len() && in_url(chars[end]) {
            end += 1;
        }
        while let Some(&last) = chars[start..end].last() {
            let unbalanced = |open: char, close: char| {
                let span = &chars[start..end];
                last == close
                    && span.iter().filter(|c| **c == close).count()
                        > span.iter().filter(|c| **c == open).count()
            };
            if matches!(last, '.' | ',' | ';' | ':' | '!' | '?')
                || unbalanced('(', ')')
                || unbalanced('[', ']')
            {
                end -= 1;
            } else {
                break;
            }
        }
        if end > start + scheme.len() {
            out.push(Link {
                chars: start..end,
                url: chars[start..end].iter().collect(),
            });
        }
        i = end.max(i + 1);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn links(s: &str) -> Vec<String> {
        let chars: Vec<char> = s.chars().collect();
        find(&chars).into_iter().map(|l| l.url).collect()
    }

    #[test]
    fn links_end_before_punctuation_and_unbalanced_brackets() {
        assert_eq!(
            links("see (https://example.org/a_(b)) now."),
            vec!["https://example.org/a_(b)"]
        );
        assert_eq!(
            links("docs: https://docs.rs/vt100. And file:///tmp/x"),
            vec!["https://docs.rs/vt100", "file:///tmp/x"]
        );
        assert!(links("http:// alone").is_empty());
        let chars: Vec<char> = "é https://a.b".chars().collect();
        assert_eq!(find(&chars)[0].chars, 2..13);
    }
}
