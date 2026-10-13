//! What an application pins to lines: diagnostics (errors, warnings,
//! notes, with a message and maybe a range), breakpoints and diff marks.
//! They follow the text: a line inserted above moves them down, a line
//! deleted under them drops them.

use serde::{Deserialize, Serialize};

use crate::buffer::LineShift;

/// How bad a diagnostic is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Hint,
    Info,
    Warning,
    Error,
}

impl Severity {
    pub fn parse(text: &str) -> Option<Severity> {
        match text {
            "error" => Some(Severity::Error),
            "warning" => Some(Severity::Warning),
            "info" | "information" => Some(Severity::Info),
            "hint" => Some(Severity::Hint),
            _ => None,
        }
    }
}

/// A message about a place in the code (a compiler's, a linter's).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    /// The line, from 0.
    pub line: usize,
    /// The columns underlined on that line (characters, end excluded);
    /// none: the line is marked in the gutter only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub columns: Option<(usize, usize)>,
    pub severity: Severity,
    pub message: String,
    /// Who said it (`rustc`, `clippy`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
}

impl Diagnostic {
    pub fn new(line: usize, severity: Severity, message: impl Into<String>) -> Self {
        Self {
            line,
            columns: None,
            severity,
            message: message.into(),
            source: String::new(),
        }
    }

    /// Underlines columns `from..to` of its line.
    pub fn columns(mut self, from: usize, to: usize) -> Self {
        self.columns = Some((from, to.max(from + 1)));
        self
    }

    pub fn source(mut self, source: impl Into<String>) -> Self {
        self.source = source.into();
        self
    }
}

/// How a line differs from the version it is compared with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Diff {
    Added,
    Modified,
    /// Lines were removed after this one.
    Removed,
}

/// Everything pinned to lines, kept in line order.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Marks {
    pub diagnostics: Vec<Diagnostic>,
    pub breakpoints: Vec<usize>,
    pub diff: Vec<(usize, Diff)>,
}

impl Marks {
    /// Moves the marks as the lines moved; marks on removed lines go.
    pub fn shift(&mut self, s: &LineShift) {
        if !s.moves_lines() && s.removed == 0 {
            return;
        }
        self.diagnostics.retain_mut(|d| match s.map(d.line) {
            Some(l) => {
                d.line = l;
                true
            }
            None => false,
        });
        self.breakpoints = self.breakpoints.iter().filter_map(|&l| s.map(l)).collect();
        self.diff = self
            .diff
            .iter()
            .filter_map(|&(l, d)| s.map(l).map(|l| (l, d)))
            .collect();
    }

    /// The worst diagnostic on a line.
    pub fn worst(&self, line: usize) -> Option<&Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.line == line)
            .max_by_key(|d| d.severity)
    }

    pub fn has_breakpoint(&self, line: usize) -> bool {
        self.breakpoints.binary_search(&line).is_ok()
    }

    /// Sets or clears a breakpoint; whether it is set now.
    pub fn toggle_breakpoint(&mut self, line: usize) -> bool {
        match self.breakpoints.binary_search(&line) {
            Ok(i) => {
                self.breakpoints.remove(i);
                false
            }
            Err(i) => {
                self.breakpoints.insert(i, line);
                true
            }
        }
    }

    pub fn diff_of(&self, line: usize) -> Option<Diff> {
        self.diff.iter().find(|(l, _)| *l == line).map(|(_, d)| *d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_follow_the_lines() {
        let mut m = Marks {
            diagnostics: vec![
                Diagnostic::new(1, Severity::Error, "a"),
                Diagnostic::new(5, Severity::Warning, "b"),
            ],
            breakpoints: vec![2, 6],
            diff: vec![(5, Diff::Added)],
        };
        // Line 1 joined into line 0, two lines inserted.
        m.shift(&LineShift {
            line: 0,
            removed: 1,
            added: 3,
        });
        assert_eq!(m.diagnostics.len(), 1);
        assert_eq!(m.diagnostics[0].line, 7);
        assert_eq!(m.breakpoints, vec![4, 8]);
        assert_eq!(m.diff_of(7), Some(Diff::Added));
        assert!(m.toggle_breakpoint(5));
        assert_eq!(m.breakpoints, vec![4, 5, 8]);
    }
}
