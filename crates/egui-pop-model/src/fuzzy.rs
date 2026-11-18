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

/// Matches `word` (one word of a query, case-insensitive) at the start
/// of a word of `text`: the whole word or its beginning, never letters
/// scattered over several words. For long text, such as a description,
/// where [`score`] would find almost any short word somewhere.
///
/// ```
/// use egui_pop_model::fuzzy;
///
/// let m = fuzzy::word_prefix("par", "Reads a paragraph").unwrap();
/// assert_eq!(m.positions, [8, 9, 10]);
/// assert!(fuzzy::word_prefix("agr", "Reads a paragraph").is_none());
/// ```
pub fn word_prefix(word: &str, text: &str) -> Option<Match> {
    let q: Vec<char> = word.chars().filter(|c| !c.is_whitespace()).collect();
    let t: Vec<char> = text.chars().collect();
    if q.is_empty() {
        return Some(Match {
            score: 0,
            positions: Vec::new(),
        });
    }
    let n = q.len();
    let mut best: Option<Match> = None;
    for j in 0..t.len().saturating_sub(n - 1) {
        if !word_start(&t, j) || !q.iter().zip(&t[j..j + n]).all(|(a, b)| same(*a, *b)) {
            continue;
        }
        let mut s = MATCH * n as i32 + WORD_START + RUN * (n as i32 - 1);
        // The whole word, not only its start.
        if t.get(j + n).is_none_or(|c| !c.is_alphanumeric()) {
            s += WORD_START;
        }
        if j == 0 {
            s += TEXT_START;
        }
        s += q.iter().zip(&t[j..j + n]).filter(|(a, b)| a == b).count() as i32 * SAME_CASE;
        s -= (j as i32).min(GAP_MAX);
        if best.as_ref().is_none_or(|b| s > b.score) {
            best = Some(Match {
                score: s,
                positions: (j..j + n).collect(),
            });
        }
    }
    best
}

/// Which kind of field a search word was found in, best first: a label
/// before a keyword, a keyword before a description.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tier {
    /// A name the user reads first (a title, a label): matched fuzzily.
    Label,
    /// A word given to be found by (a keyword, an id, a section's
    /// title): matched fuzzily.
    Keyword,
    /// Running text (a description): matched by whole words or word
    /// prefixes only ([`word_prefix`]).
    Description,
}

/// What a search found in [`Fields`]: the worst tier any of the query's
/// words needed, and the words' summed score. Sorting ascending puts
/// the best first (tier, then the higher score).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Found {
    pub tier: Tier,
    pub score: i32,
}

impl Ord for Found {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.tier
            .cmp(&other.tier)
            .then(other.score.cmp(&self.score))
    }
}

impl PartialOrd for Found {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// The fields of one thing a search field looks through (a setting, a
/// page), each of a [`Tier`]: labels and keywords are matched fuzzily
/// ([`score`]), descriptions by words and word prefixes
/// ([`word_prefix`]), so a long description does not match a word
/// whose letters it merely holds in order. Each field is matched on its
/// own: a word never spans two fields.
///
/// ```
/// use egui_pop_model::fuzzy::{Fields, Tier};
///
/// let wrap = Fields::new()
///     .label("Soft wrap")
///     .description("Lines break at the pane's edge, so a paragraph reads without scrolling; the file itself remains unchanged.");
/// assert_eq!(wrap.find("swrp").unwrap().tier, Tier::Label);
/// assert_eq!(wrap.find("parag").unwrap().tier, Tier::Description);
/// assert!(wrap.find("specimen").is_none());
/// ```
#[derive(Clone, Debug, Default)]
pub struct Fields {
    fields: Vec<(Tier, String)>,
}

impl Fields {
    pub fn new() -> Self {
        Self::default()
    }

    /// A field of `tier`; empty text is left out.
    pub fn field(mut self, tier: Tier, text: impl Into<String>) -> Self {
        let text = text.into();
        if !text.trim().is_empty() {
            self.fields.push((tier, text));
        }
        self
    }

    pub fn label(self, text: impl Into<String>) -> Self {
        self.field(Tier::Label, text)
    }

    pub fn keyword(self, text: impl Into<String>) -> Self {
        self.field(Tier::Keyword, text)
    }

    pub fn keywords<S: Into<String>>(self, words: impl IntoIterator<Item = S>) -> Self {
        words.into_iter().fold(self, |f, w| f.keyword(w))
    }

    pub fn description(self, text: impl Into<String>) -> Self {
        self.field(Tier::Description, text)
    }

    /// Adds another's fields, keeping their tiers.
    pub fn extend(mut self, other: Fields) -> Self {
        self.fields.extend(other.fields);
        self
    }

    /// One word's best field: the best tier, then the best score there.
    fn word(&self, word: &str) -> Option<Found> {
        self.fields
            .iter()
            .filter_map(|(tier, text)| {
                let m = match tier {
                    Tier::Label | Tier::Keyword => score(word, text),
                    Tier::Description => word_prefix(word, text),
                };
                m.map(|m| Found {
                    tier: *tier,
                    score: m.score,
                })
            })
            .min()
    }

    /// Whether every word of `query` (split on whitespace) is found, and
    /// how well; an empty query matches with a score of 0.
    pub fn find(&self, query: &str) -> Option<Found> {
        let mut found = Found {
            tier: Tier::Label,
            score: 0,
        };
        for w in query.split_whitespace() {
            let f = self.word(w)?;
            found.tier = found.tier.max(f.tier);
            found.score += f.score;
        }
        Some(found)
    }
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

    const WRAP: &str = "Soft wrap: lines break at the pane's edge, so a paragraph reads without scrolling; the file itself remains unchanged on disk.";

    #[test]
    fn a_long_description_does_not_match_scattered_letters() {
        // The letters are all there, in order, over several words.
        assert!(score("specimen", WRAP).is_some());
        assert!(word_prefix("specimen", WRAP).is_none());
        assert!(Fields::new().description(WRAP).find("specimen").is_none());
        // Words and word prefixes still match, in any case.
        assert!(word_prefix("Paragraph", WRAP).is_some());
        assert!(word_prefix("scroll", WRAP).is_some());
        assert!(word_prefix("ragraph", WRAP).is_none());
        assert_eq!(
            word_prefix("pane", WRAP).unwrap().positions,
            [30, 31, 32, 33]
        );
    }

    #[test]
    fn fields_rank_label_then_keyword_then_description() {
        let f = Fields::new()
            .label("Text zoom")
            .keyword("magnify")
            .description("The size of the text, in percent.");
        assert_eq!(f.find("tz").unwrap().tier, Tier::Label);
        assert_eq!(f.find("mag").unwrap().tier, Tier::Keyword);
        assert_eq!(f.find("percent").unwrap().tier, Tier::Description);
        // A query's tier is its worst word's.
        assert_eq!(f.find("zoom percent").unwrap().tier, Tier::Description);
        assert!(f.find("zoom nothing").is_none());
        assert_eq!(f.find("  ").unwrap().score, 0);
        // Keywords match fuzzily, a description does not.
        assert!(f.find("mgnf").is_some());
        assert!(f.find("prcnt").is_none());
        // A word does not span two fields.
        assert!(Fields::new().label("ab").label("cd").find("abcd").is_none());
        // Best first.
        let label = Fields::new().label("Zoom").find("zoom").unwrap();
        let desc = Fields::new().description("zoom").find("zoom").unwrap();
        let mut v = [desc, label];
        v.sort();
        assert_eq!(v, [label, desc]);
    }
}
