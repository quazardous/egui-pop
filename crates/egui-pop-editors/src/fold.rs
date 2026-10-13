//! Folding by indentation: a line folds the lines under it that are
//! indented further (blank lines inside included, blank lines at the end
//! left out), as most editors do without a grammar.

use std::collections::BTreeSet;

use crate::buffer::{Buffer, LineShift, indent_width};

/// How far a fold looks for the next non-blank line.
const LOOK: usize = 1000;

fn blank(b: &Buffer, i: usize) -> bool {
    b.line(i).trim().is_empty()
}

fn width(b: &Buffer, i: usize, tab: usize) -> usize {
    indent_width(b.indent_of(i), tab)
}

/// Whether line `line` starts a fold: the next non-blank line is
/// indented further.
pub fn foldable(b: &Buffer, line: usize, tab: usize) -> bool {
    if blank(b, line) {
        return false;
    }
    let w = width(b, line, tab);
    let end = (line + LOOK).min(b.line_count());
    (line + 1..end)
        .find(|&i| !blank(b, i))
        .is_some_and(|i| width(b, i, tab) > w)
}

/// The last line folded under `line`, if it folds.
pub fn fold_end(b: &Buffer, line: usize, tab: usize) -> Option<usize> {
    if !foldable(b, line, tab) {
        return None;
    }
    let w = width(b, line, tab);
    let mut last = line;
    for i in line + 1..b.line_count() {
        if blank(b, i) {
            continue;
        }
        if width(b, i, tab) <= w {
            break;
        }
        last = i;
    }
    (last > line).then_some(last)
}

/// The folded lines' starts, and the rows they leave on screen.
#[derive(Clone, Debug, Default)]
pub struct Folds {
    starts: BTreeSet<usize>,
    /// Row to line, when something is folded; built for `built`.
    rows: Vec<usize>,
    built: Option<(u64, usize)>,
    changes: usize,
}

impl Folds {
    pub fn is_folded(&self, line: usize) -> bool {
        self.starts.contains(&line)
    }

    pub fn starts(&self) -> impl Iterator<Item = usize> + '_ {
        self.starts.iter().copied()
    }

    pub fn is_empty(&self) -> bool {
        self.starts.is_empty()
    }

    /// Folds `line` if it folds; whether it did.
    pub fn fold(&mut self, b: &Buffer, line: usize, tab: usize) -> bool {
        if fold_end(b, line, tab).is_some() && self.starts.insert(line) {
            self.changes += 1;
            return true;
        }
        false
    }

    pub fn unfold(&mut self, line: usize) -> bool {
        let done = self.starts.remove(&line);
        if done {
            self.changes += 1;
        }
        done
    }

    pub fn clear(&mut self) {
        if !self.starts.is_empty() {
            self.starts.clear();
            self.changes += 1;
        }
    }

    /// Moves the folds as the lines moved.
    pub fn shift(&mut self, s: &LineShift) {
        if self.starts.is_empty() || (!s.moves_lines() && s.removed == 0) {
            return;
        }
        self.starts = self.starts.iter().filter_map(|&l| s.map(l)).collect();
        self.changes += 1;
    }

    /// Unfolds what hides `line`; whether anything was.
    pub fn reveal(&mut self, b: &Buffer, line: usize, tab: usize) -> bool {
        let hiding: Vec<usize> = self
            .starts
            .range(..line)
            .copied()
            .filter(|&s| fold_end(b, s, tab).is_some_and(|e| line <= e))
            .collect();
        for s in &hiding {
            self.starts.remove(s);
        }
        if !hiding.is_empty() {
            self.changes += 1;
        }
        !hiding.is_empty()
    }

    /// Rebuilds the rows for this version of the text.
    pub fn update(&mut self, b: &Buffer, version: u64, tab: usize) {
        if self.built == Some((version, self.changes)) {
            return;
        }
        self.built = Some((version, self.changes));
        self.rows.clear();
        if self.starts.is_empty() {
            return;
        }
        // Folds that no longer fold go.
        self.starts.retain(|&s| fold_end(b, s, tab).is_some());
        let mut i = 0;
        let n = b.line_count();
        while i < n {
            self.rows.push(i);
            if self.starts.contains(&i)
                && let Some(e) = fold_end(b, i, tab)
            {
                i = e + 1;
            } else {
                i += 1;
            }
        }
    }

    /// How many rows are on screen.
    pub fn row_count(&self, b: &Buffer) -> usize {
        if self.starts.is_empty() {
            b.line_count()
        } else {
            self.rows.len()
        }
    }

    /// The line drawn on row `row`.
    pub fn line_of(&self, row: usize) -> usize {
        if self.starts.is_empty() {
            row
        } else {
            self.rows.get(row).copied().unwrap_or(usize::MAX)
        }
    }

    /// The row line `line` is drawn on (its fold's when hidden).
    pub fn row_of(&self, line: usize) -> usize {
        if self.starts.is_empty() {
            line
        } else {
            self.rows.partition_point(|&l| l <= line).saturating_sub(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CODE: &str = "fn a() {\n    one;\n\n    two;\n}\nfn b() {}\n";

    #[test]
    fn indentation_folds() {
        let b = Buffer::from_text(CODE);
        assert!(foldable(&b, 0, 4));
        assert_eq!(fold_end(&b, 0, 4), Some(3));
        assert!(!foldable(&b, 1, 4));
        assert!(!foldable(&b, 5, 4));
        let mut f = Folds::default();
        assert!(f.fold(&b, 0, 4));
        f.update(&b, 1, 4);
        assert_eq!(f.row_count(&b), 4);
        assert_eq!(f.line_of(1), 4);
        assert_eq!(f.row_of(2), 0);
        assert!(f.reveal(&b, 2, 4));
        f.update(&b, 1, 4);
        assert_eq!(f.row_count(&b), b.line_count());
    }
}
