//! Highlighting through syntect (MIT): Sublime Text grammars, with the
//! syntaxes syntect bundles (loaded once per process, on first use).
//!
//! syntect parses a file from the top, each line from the state the line
//! before left. The highlighter keeps that state every [`STRIDE`] lines
//! and the frontier it has reached; a jump to the end of a long file
//! parses its way there within the editor's budget per pass, so the
//! interface never waits for it.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::{Arc, OnceLock};

use syntect::easy::ScopeRangeIterator;
use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxReference, SyntaxSet};

use super::{Highlighter, Kind, Run, Runs};
use crate::buffer::Buffer;

/// Parse states are kept every this many lines.
pub const STRIDE: usize = 64;

/// At most this many lines' runs are cached.
const CACHE: usize = 4096;

/// syntect's bundled syntaxes, loaded once.
pub fn syntaxes() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

#[derive(Clone)]
struct State {
    parse: ParseState,
    stack: ScopeStack,
}

/// A syntect highlighter for one language.
pub struct SyntectHighlighter {
    syntax: &'static SyntaxReference,
    /// The state at the start of line `k * STRIDE`.
    checkpoints: Vec<State>,
    /// The first line whose start state is not known past `front`.
    frontier: usize,
    front: State,
    cache: HashMap<usize, Runs>,
    kinds: HashMap<Scope, Option<Kind>>,
}

impl std::fmt::Debug for SyntectHighlighter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyntectHighlighter")
            .field("syntax", &self.syntax.name)
            .field("frontier", &self.frontier)
            .finish()
    }
}

impl SyntectHighlighter {
    /// For a language name (`Rust`), an extension (`rs`) or a file name
    /// (`main.rs`); `None` when syntect knows none of them.
    pub fn for_language(name: &str) -> Option<Self> {
        let set = syntaxes();
        let ext = name.rsplit_once('.').map(|(_, e)| e);
        let syntax = set
            .find_syntax_by_token(name)
            .or_else(|| ext.and_then(|e| set.find_syntax_by_extension(e)))
            .or_else(|| {
                set.syntaxes()
                    .iter()
                    .find(|s| s.name.eq_ignore_ascii_case(name))
            })?;
        Some(Self::new(syntax))
    }

    fn new(syntax: &'static SyntaxReference) -> Self {
        let start = State {
            parse: ParseState::new(syntax),
            stack: ScopeStack::new(),
        };
        Self {
            syntax,
            checkpoints: vec![start.clone()],
            frontier: 0,
            front: start,
            cache: HashMap::new(),
            kinds: HashMap::new(),
        }
    }

    /// The language's name.
    pub fn language(&self) -> &str {
        &self.syntax.name
    }

    /// How far the parse has reached (for the debug bridge).
    pub fn frontier(&self) -> usize {
        self.frontier
    }

    /// Parses one line from `state`, leaving the state after it.
    fn parse(&mut self, state: &mut State, line: &str) -> Runs {
        let mut text = String::with_capacity(line.len() + 1);
        text.push_str(line);
        text.push('\n');
        let Ok(ops) = state.parse.parse_line(&text, syntaxes()) else {
            return plain(line.len());
        };
        let mut runs: Vec<Run> = Vec::new();
        for (range, op) in ScopeRangeIterator::new(&ops, &text) {
            if state.stack.apply(op).is_err() {
                return plain(line.len());
            }
            let range = range.start.min(line.len())..range.end.min(line.len());
            if range.is_empty() {
                continue;
            }
            let kind = self.kind_of(&state.stack);
            match runs.last_mut() {
                Some(last) if last.kind == kind && last.bytes.end == range.start => {
                    last.bytes.end = range.end;
                }
                _ => runs.push(Run { bytes: range, kind }),
            }
        }
        Arc::from(runs)
    }

    /// The innermost scope that says something about colour.
    fn kind_of(&mut self, stack: &ScopeStack) -> Kind {
        for scope in stack.as_slice().iter().rev() {
            let k = *self
                .kinds
                .entry(*scope)
                .or_insert_with(|| Kind::of_scope(&scope.build_string()));
            if let Some(k) = k {
                return k;
            }
        }
        Kind::Plain
    }

    /// Parses the frontier's line and moves past it.
    fn advance(&mut self, buffer: &Buffer, keep: bool) {
        let mut state = self.front.clone();
        let runs = self.parse(&mut state, buffer.line(self.frontier));
        if keep {
            self.cache.insert(self.frontier, runs);
        }
        self.frontier += 1;
        self.front = state;
        if self.frontier.is_multiple_of(STRIDE) && self.checkpoints.len() == self.frontier / STRIDE
        {
            self.checkpoints.push(self.front.clone());
        }
    }

    /// The runs of a line before the frontier no longer cached: parsed
    /// again from the checkpoint before it.
    fn again(&mut self, buffer: &Buffer, line: usize) -> Runs {
        let k = (line / STRIDE).min(self.checkpoints.len() - 1);
        let mut state = self.checkpoints[k].clone();
        let mut runs = plain(0);
        for l in k * STRIDE..=line {
            runs = self.parse(&mut state, buffer.line(l));
            if l >= line.saturating_sub(STRIDE) {
                self.cache.insert(l, runs.clone());
            }
        }
        runs
    }
}

fn plain(len: usize) -> Runs {
    Arc::from(vec![Run {
        bytes: 0..len,
        kind: Kind::Plain,
    }])
}

impl Highlighter for SyntectHighlighter {
    fn name(&self) -> String {
        format!("syntect: {}", self.syntax.name)
    }

    fn edited(&mut self, line: usize) {
        if line >= self.frontier {
            self.cache.retain(|&l, _| l < line);
            return;
        }
        let k = (line / STRIDE).min(self.checkpoints.len() - 1);
        self.checkpoints.truncate(k + 1);
        self.front = self.checkpoints[k].clone();
        self.frontier = k * STRIDE;
        let from = self.frontier;
        self.cache.retain(|&l, _| l < from);
    }

    fn highlight(
        &mut self,
        buffer: &Buffer,
        lines: Range<usize>,
        budget: usize,
    ) -> Vec<Option<Runs>> {
        let count = buffer.line_count();
        let lines = lines.start.min(count)..lines.end.min(count);
        let mut left = budget;
        while self.frontier < lines.start && left > 0 {
            self.advance(buffer, false);
            left -= 1;
        }
        if self.cache.len() > CACHE {
            self.cache.retain(|l, _| lines.contains(l));
        }
        let mut out = Vec::with_capacity(lines.len());
        for i in lines.clone() {
            if let Some(r) = self.cache.get(&i) {
                out.push(Some(r.clone()));
            } else if i < self.frontier {
                out.push(Some(self.again(buffer, i)));
            } else if i == self.frontier && left > 0 {
                self.advance(buffer, true);
                left -= 1;
                out.push(self.cache.get(&i).cloned());
            } else {
                out.push(None);
            }
        }
        out
    }

    fn ready(&self, line: usize) -> bool {
        self.frontier >= line
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_is_highlighted_and_follows_edits() {
        let mut h = SyntectHighlighter::for_language("rs").expect("Rust");
        assert_eq!(h.language(), "Rust");
        let mut b = Buffer::from_text("fn main() {\n    let s = \"hi\"; // greet\n}");
        let lines = h.highlight(&b, 0..3, 100);
        let second = lines[1].as_ref().unwrap();
        let kind_at = |runs: &Runs, byte: usize| {
            runs.iter()
                .find(|r| r.bytes.contains(&byte))
                .map(|r| r.kind)
        };
        assert_eq!(kind_at(second, 4), Some(Kind::Keyword), "{second:?}");
        assert_eq!(kind_at(second, 13), Some(Kind::String), "{second:?}");
        assert_eq!(kind_at(second, 22), Some(Kind::Comment), "{second:?}");
        // Opening a block comment on line 0 turns the next lines into a
        // comment once the editor says where it changed.
        b.apply(&crate::buffer::Change::insert(
            crate::buffer::Pos::new(0, 0),
            "/* ",
        ));
        h.edited(0);
        let lines = h.highlight(&b, 0..3, 100);
        assert_eq!(kind_at(lines[1].as_ref().unwrap(), 4), Some(Kind::Comment));
    }

    #[test]
    fn a_far_line_waits_for_the_budget() {
        let mut h = SyntectHighlighter::for_language("Rust").unwrap();
        let text = "let x = 1;\n".repeat(1000);
        let b = Buffer::from_text(&text);
        let out = h.highlight(&b, 900..910, 100);
        assert!(out.iter().all(Option::is_none));
        assert_eq!(h.frontier(), 100);
        for _ in 0..9 {
            h.highlight(&b, 900..910, 100);
        }
        let out = h.highlight(&b, 900..910, 100);
        assert!(out.iter().all(Option::is_some));
        // A line far behind the frontier, evicted, is parsed again.
        h.cache.clear();
        assert!(h.highlight(&b, 10..11, 0)[0].is_some());
    }
}
