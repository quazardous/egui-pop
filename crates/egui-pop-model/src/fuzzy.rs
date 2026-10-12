//! Fuzzy matching for search fields (the command palette, the settings
//! filter): the query's characters in order in the text, not
//! necessarily next to each other, scored so that matches at word
//! starts, in runs and early in the text come first.
//!
//! Owned rather than adopted: `nucleo-matcher` is MPL-2.0 (refused by the
//! licence policy), `fuzzy-matcher` and `sublime_fuzzy` are unmaintained
//! and give no say over the word starts this needs (`Category: Title`).
//!
//! ```
//! use egui_pop_model::fuzzy;
//!
//! let m = fuzzy::score("opf", "File: Open file").unwrap();
//! assert_eq!(m.positions, [6, 7, 11]); // `Op`en `f`ile
//! assert!(fuzzy::score("zz", "File: Open file").is_none());
//! ```

/// A match: its score (higher is better) and the matched characters'
/// indices (in `char`s) in the text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match {
    pub score: i32,
    pub positions: Vec<usize>,
}

const MATCH: i32 = 16;
const WORD_START: i32 = 24;
const TEXT_START: i32 = 8;
const RUN: i32 = 20;
const SAME_CASE: i32 = 1;
const GAP: i32 = 1;
const GAP_MAX: i32 = 12;

fn word_start(text: &[char], j: usize) -> bool {
    if j == 0 {
        return true;
    }
    let (prev, c) = (text[j - 1], text[j]);
    !prev.is_alphanumeric() || (prev.is_lowercase() && c.is_uppercase())
}

fn same(a: char, b: char) -> bool {
    a == b || a.to_lowercase().eq(b.to_lowercase())
}

/// Matches `query` (spaces ignored, case-insensitive) in `text`; none
/// when its characters are not all there in order. An empty query
/// matches everything with a score of 0.
pub fn score(query: &str, text: &str) -> Option<Match> {
    let q: Vec<char> = query.chars().filter(|c| !c.is_whitespace()).collect();
    let t: Vec<char> = text.chars().collect();
    if q.is_empty() {
        return Some(Match {
            score: 0,
            positions: Vec::new(),
        });
    }
    let (n, m) = (q.len(), t.len());
    if n > m {
        return None;
    }
    // best[i][j]: the best score with q[i] matched at t[j], and from
    // where q[i - 1] was matched.
    const NONE: i32 = i32::MIN / 2;
    let mut best = vec![vec![NONE; m]; n];
    let mut from = vec![vec![usize::MAX; m]; n];
    for i in 0..n {
        for j in i..m {
            if !same(q[i], t[j]) {
                continue;
            }
            let mut here = MATCH;
            if word_start(&t, j) {
                here += WORD_START;
            }
            if j == 0 {
                here += TEXT_START;
            }
            if q[i] == t[j] {
                here += SAME_CASE;
            }
            if i == 0 {
                // Earlier is better, a little.
                best[i][j] = here - (j as i32).min(GAP_MAX);
                continue;
            }
            for k in (i - 1)..j {
                let before = best[i - 1][k];
                if before == NONE {
                    continue;
                }
                let s = if k + 1 == j {
                    before + here + RUN
                } else {
                    before + here - ((j - k - 1) as i32 * GAP).min(GAP_MAX)
                };
                if s > best[i][j] {
                    best[i][j] = s;
                    from[i][j] = k;
                }
            }
        }
    }
    let (end, &top) = best[n - 1]
        .iter()
        .enumerate()
        .max_by_key(|(j, s)| (**s, std::cmp::Reverse(*j)))?;
    if top == NONE {
        return None;
    }
    let mut positions = vec![end; n];
    let mut j = end;
    for i in (1..n).rev() {
        j = from[i][j];
        positions[i - 1] = j;
    }
    Some(Match {
        score: top,
        positions,
    })
}

/// The items matching `query`, best first (ties keep their order), each
/// with its match.
pub fn rank<T>(items: &[T], query: &str, text: impl Fn(&T) -> String) -> Vec<(usize, Match)> {
    let mut hits: Vec<(usize, Match)> = items
        .iter()
        .enumerate()
        .filter_map(|(i, it)| score(query, &text(it)).map(|m| (i, m)))
        .collect();
    hits.sort_by_key(|(i, m)| (std::cmp::Reverse(m.score), *i));
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order(query: &str, items: &[&str]) -> Vec<String> {
        rank(items, query, |s| s.to_string())
            .into_iter()
            .map(|(i, _)| items[i].to_string())
            .collect()
    }

    #[test]
    fn word_starts_and_runs_win() {
        assert_eq!(
            order(
                "of",
                &["Edit: Copy format", "File: Open file", "View: Zoom off"]
            ),
            ["File: Open file", "View: Zoom off", "Edit: Copy format"]
        );
        assert_eq!(
            order("undo", &["Edit: Redo", "Edit: Undo", "View: Unfold doc"]),
            ["Edit: Undo", "View: Unfold doc"]
        );
        // Camel case counts as a word start.
        let m = score("ts", "toggleSidebar").unwrap();
        assert_eq!(m.positions, [0, 6]);
    }

    #[test]
    fn every_character_must_be_there_in_order() {
        assert!(score("abc", "cab").is_none());
        assert!(score("abcd", "abc").is_none());
        assert_eq!(score("", "x").unwrap().score, 0);
        let m = score("o p", "File: Open").unwrap();
        assert_eq!(m.positions, [6, 7]);
        // Non-ASCII text: positions are in characters.
        assert_eq!(score("é", "Café").unwrap().positions, [3]);
    }

    #[test]
    fn positions_are_increasing_and_point_at_matching_characters() {
        let texts = [
            "Command palette",
            "View: Toggle side bar",
            "settings/appearance.focus",
            "aaaa bbbb aaaa",
        ];
        for t in texts {
            let chars: Vec<char> = t.chars().collect();
            for q in ["a", "ab", "cp", "vtsb", "sf", "aba", "e"] {
                if let Some(m) = score(q, t) {
                    assert!(m.positions.windows(2).all(|w| w[0] < w[1]), "{q} in {t}");
                    let qc: Vec<char> = q.chars().collect();
                    for (i, &p) in m.positions.iter().enumerate() {
                        assert!(same(qc[i], chars[p]), "{q} in {t}");
                    }
                }
            }
        }
    }
}
