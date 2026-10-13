//! Highlighting through tree-sitter (MIT), feature `tree-sitter`: the
//! application brings the grammar (a `tree-sitter-<language>` crate and
//! its highlight query) as a `HighlightConfiguration`; capture names
//! (`keyword`, `string.special`, `function.builtin`) map to kinds by
//! prefix, as TextMate scopes do.
//!
//! This first version highlights the whole text again after a change
//! (tree-sitter is fast, but it is not incremental here yet: the edits
//! are not handed to the parser).

use std::ops::Range;
use std::sync::Arc;

use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent};

use super::{Highlighter, Kind, Run, Runs};
use crate::buffer::Buffer;

/// The capture names the configuration is given; each maps to a kind.
pub const CAPTURES: &[&str] = &[
    "attribute",
    "boolean",
    "comment",
    "constant",
    "constant.builtin",
    "constructor",
    "escape",
    "function",
    "function.builtin",
    "function.macro",
    "function.method",
    "keyword",
    "number",
    "operator",
    "punctuation",
    "punctuation.bracket",
    "punctuation.delimiter",
    "string",
    "string.escape",
    "string.special",
    "tag",
    "type",
    "type.builtin",
];

/// A tree-sitter highlighter over a grammar the application brings.
pub struct TreeSitterHighlighter {
    name: String,
    config: HighlightConfiguration,
    lines: Option<Vec<Runs>>,
}

impl std::fmt::Debug for TreeSitterHighlighter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TreeSitterHighlighter")
            .field("name", &self.name)
            .finish()
    }
}

impl TreeSitterHighlighter {
    /// A highlighter named `name` (for the debug bridge) over `config`,
    /// which it configures with [`CAPTURES`].
    pub fn new(name: impl Into<String>, mut config: HighlightConfiguration) -> Self {
        config.configure(CAPTURES);
        Self {
            name: name.into(),
            config,
            lines: None,
        }
    }

    fn run_all(&self, buffer: &Buffer) -> Vec<Runs> {
        let text = buffer.text();
        let mut starts = Vec::with_capacity(buffer.line_count());
        let mut at = 0;
        for l in buffer.lines() {
            starts.push(at);
            at += l.len() + 1;
        }
        let mut lines: Vec<Vec<Run>> = vec![Vec::new(); buffer.line_count()];
        let mut h = tree_sitter_highlight::Highlighter::new();
        let Ok(events) = h.highlight(&self.config, text.as_bytes(), None, None, |_| None) else {
            return lines.into_iter().map(Arc::from).collect();
        };
        let mut stack: Vec<Kind> = Vec::new();
        for event in events {
            match event {
                Ok(HighlightEvent::HighlightStart(hl)) => {
                    let kind = CAPTURES
                        .get(hl.0)
                        .and_then(|n| Kind::of_scope(n))
                        .unwrap_or(Kind::Plain);
                    stack.push(kind);
                }
                Ok(HighlightEvent::HighlightEnd) => {
                    stack.pop();
                }
                Ok(HighlightEvent::Source { start, end }) => {
                    let kind = stack.last().copied().unwrap_or(Kind::Plain);
                    split(&starts, &mut lines, start..end, kind);
                }
                Err(_) => break,
            }
        }
        lines.into_iter().map(Arc::from).collect()
    }
}

/// Adds a byte range of the whole text to the lines it covers.
fn split(starts: &[usize], lines: &mut [Vec<Run>], range: Range<usize>, kind: Kind) {
    let mut line = starts
        .partition_point(|&s| s <= range.start)
        .saturating_sub(1);
    let mut from = range.start;
    while from < range.end && line < lines.len() {
        let start = starts[line];
        let end = starts.get(line + 1).map_or(usize::MAX, |n| n - 1);
        let to = range.end.min(end);
        if to > from {
            lines[line].push(Run {
                bytes: from - start..to - start,
                kind,
            });
        }
        line += 1;
        from = starts.get(line).copied().unwrap_or(range.end).max(from);
    }
}

impl Highlighter for TreeSitterHighlighter {
    fn name(&self) -> String {
        format!("tree-sitter: {}", self.name)
    }

    fn edited(&mut self, _: usize) {
        self.lines = None;
    }

    fn highlight(&mut self, buffer: &Buffer, lines: Range<usize>, _: usize) -> Vec<Option<Runs>> {
        if self.lines.as_ref().map(Vec::len) != Some(buffer.line_count()) {
            self.lines = Some(self.run_all(buffer));
        }
        let all = self.lines.as_ref().expect("just made");
        lines.map(|i| all.get(i).cloned()).collect()
    }
}
