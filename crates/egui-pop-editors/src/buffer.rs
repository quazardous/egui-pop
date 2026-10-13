//! The text: lines, positions, changes and their inverses.
//!
//! A [`Buffer`] is a list of lines without their line breaks (a file of
//! 100 000 lines is a vector of 100 000 strings: an edit touches the
//! lines it changes, never the whole text). A [`Pos`] is a line and a
//! column counted in characters (Unicode scalar values), so positions
//! survive any encoding the application uses. A [`Change`] replaces the
//! text between two positions; applying one gives back its inverse,
//! which is what the history keeps.

use serde::{Deserialize, Serialize};

/// A place in the text: a line (from 0) and a column in characters
/// (from 0).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct Pos {
    pub line: usize,
    pub col: usize,
}

impl Pos {
    pub const fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

/// The text between two positions, `start` before or at `end`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Span {
    pub start: Pos,
    pub end: Pos,
}

impl Span {
    /// The span between two positions, in either order.
    pub fn new(a: Pos, b: Pos) -> Self {
        if a <= b {
            Self { start: a, end: b }
        } else {
            Self { start: b, end: a }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    pub fn contains(&self, p: Pos) -> bool {
        self.start <= p && p < self.end
    }
}

/// Replaces the text from `from` to `to` with `text` (`\n` separates
/// lines).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Change {
    pub from: Pos,
    pub to: Pos,
    pub text: String,
}

impl Change {
    pub fn new(from: Pos, to: Pos, text: impl Into<String>) -> Self {
        let (from, to) = if from <= to { (from, to) } else { (to, from) };
        Self {
            from,
            to,
            text: text.into(),
        }
    }

    /// Inserts `text` at `at`.
    pub fn insert(at: Pos, text: impl Into<String>) -> Self {
        Self::new(at, at, text)
    }

    /// Deletes the span.
    pub fn delete(span: Span) -> Self {
        Self::new(span.start, span.end, "")
    }

    /// Where the inserted text ends once applied.
    pub fn end_after(&self) -> Pos {
        end_of(self.from, &self.text)
    }
}

/// Where `text` ends when inserted at `at`.
pub fn end_of(at: Pos, text: &str) -> Pos {
    match text.rfind('\n') {
        Some(i) => Pos::new(
            at.line + text.matches('\n').count(),
            text[i + 1..].chars().count(),
        ),
        None => Pos::new(at.line, at.col + text.chars().count()),
    }
}

/// How an applied change moved the lines: lines after `line` shift by
/// `added - removed`; lines `line + 1 ..= line + removed` were joined into
/// `line` (and are gone).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineShift {
    pub line: usize,
    pub removed: usize,
    pub added: usize,
}

impl LineShift {
    /// Where line `l` went, or `None` when it was removed.
    pub fn map(&self, l: usize) -> Option<usize> {
        if l <= self.line {
            Some(l)
        } else if l <= self.line + self.removed {
            None
        } else {
            Some(l + self.added - self.removed)
        }
    }

    /// Whether the line count changed.
    pub fn moves_lines(&self) -> bool {
        self.added != self.removed
    }
}

/// The text, as lines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Buffer {
    lines: Vec<String>,
}

impl Default for Buffer {
    fn default() -> Self {
        Self {
            lines: vec![String::new()],
        }
    }
}

impl Buffer {
    /// The text split at `\n` (a `\r` before it is dropped).
    pub fn from_text(text: &str) -> Self {
        let lines: Vec<String> = text
            .split('\n')
            .map(|l| l.strip_suffix('\r').unwrap_or(l).to_string())
            .collect();
        Self { lines }
    }

    /// The whole text, lines joined by `\n`.
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    /// How many lines (at least one).
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Line `i`, or `""` past the end.
    pub fn line(&self, i: usize) -> &str {
        self.lines.get(i).map_or("", String::as_str)
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// Line `i`'s length in characters.
    pub fn line_len(&self, i: usize) -> usize {
        self.line(i).chars().count()
    }

    /// The text's length in bytes, line breaks counted.
    pub fn byte_len(&self) -> usize {
        self.lines.iter().map(String::len).sum::<usize>() + self.lines.len() - 1
    }

    /// The last position.
    pub fn end(&self) -> Pos {
        let last = self.lines.len() - 1;
        Pos::new(last, self.line_len(last))
    }

    /// The nearest position that exists.
    pub fn clamp(&self, p: Pos) -> Pos {
        if p.line >= self.lines.len() {
            return self.end();
        }
        Pos::new(p.line, p.col.min(self.line_len(p.line)))
    }

    /// The text of a span.
    pub fn slice(&self, span: Span) -> String {
        let (a, b) = (self.clamp(span.start), self.clamp(span.end));
        if a.line == b.line {
            let l = self.line(a.line);
            return l[byte_at(l, a.col)..byte_at(l, b.col)].to_string();
        }
        let mut out = String::new();
        let first = self.line(a.line);
        out.push_str(&first[byte_at(first, a.col)..]);
        for l in &self.lines[a.line + 1..b.line] {
            out.push('\n');
            out.push_str(l);
        }
        out.push('\n');
        let last = self.line(b.line);
        out.push_str(&last[..byte_at(last, b.col)]);
        out
    }

    /// Applies a change (its positions clamped first); returns its
    /// inverse and how the lines moved.
    pub fn apply(&mut self, change: &Change) -> (Change, LineShift) {
        let from = self.clamp(change.from);
        let to = self.clamp(change.to).max(from);
        let removed = self.slice(Span::new(from, to));
        let first = &self.lines[from.line];
        let head = first[..byte_at(first, from.col)].to_string();
        let last = &self.lines[to.line];
        let tail = last[byte_at(last, to.col)..].to_string();
        let mut new: Vec<String> = change.text.split('\n').map(str::to_string).collect();
        let n = new.len();
        new[0].insert_str(0, &head);
        new[n - 1].push_str(&tail);
        self.lines.splice(from.line..=to.line, new);
        let inverse = Change {
            from,
            to: end_of(from, &change.text),
            text: removed,
        };
        let shift = LineShift {
            line: from.line,
            removed: to.line - from.line,
            added: n - 1,
        };
        (inverse, shift)
    }

    /// The word around a position (letters, digits, `_`), or the run of
    /// other characters there.
    pub fn word_at(&self, p: Pos) -> Span {
        let chars: Vec<char> = self.line(p.line).chars().collect();
        let col = p.col.min(chars.len());
        let class = |c: char| char_class(c);
        let at = if col < chars.len() {
            class(chars[col])
        } else if col > 0 {
            class(chars[col - 1])
        } else {
            return Span::new(p, p);
        };
        let mut a = col;
        while a > 0 && class(chars[a - 1]) == at {
            a -= 1;
        }
        let mut b = col;
        while b < chars.len() && class(chars[b]) == at {
            b += 1;
        }
        Span::new(Pos::new(p.line, a), Pos::new(p.line, b))
    }

    /// The start of the previous word (Ctrl+Left), across lines.
    pub fn word_left(&self, p: Pos) -> Pos {
        if p.col == 0 {
            return if p.line == 0 {
                p
            } else {
                Pos::new(p.line - 1, self.line_len(p.line - 1))
            };
        }
        let chars: Vec<char> = self.line(p.line).chars().collect();
        let mut c = p.col.min(chars.len());
        while c > 0 && chars[c - 1].is_whitespace() {
            c -= 1;
        }
        if c > 0 {
            let k = char_class(chars[c - 1]);
            while c > 0 && char_class(chars[c - 1]) == k {
                c -= 1;
            }
        }
        Pos::new(p.line, c)
    }

    /// The end of the next word (Ctrl+Right), across lines.
    pub fn word_right(&self, p: Pos) -> Pos {
        let chars: Vec<char> = self.line(p.line).chars().collect();
        if p.col >= chars.len() {
            return if p.line + 1 < self.lines.len() {
                Pos::new(p.line + 1, 0)
            } else {
                p
            };
        }
        let mut c = p.col;
        while c < chars.len() && chars[c].is_whitespace() {
            c += 1;
        }
        if c < chars.len() {
            let k = char_class(chars[c]);
            while c < chars.len() && char_class(chars[c]) == k {
                c += 1;
            }
        }
        Pos::new(p.line, c)
    }

    /// The position one character before (across a line break).
    pub fn prev(&self, p: Pos) -> Pos {
        if p.col > 0 {
            Pos::new(p.line, p.col - 1)
        } else if p.line > 0 {
            Pos::new(p.line - 1, self.line_len(p.line - 1))
        } else {
            p
        }
    }

    /// The position one character after (across a line break).
    pub fn next(&self, p: Pos) -> Pos {
        if p.col < self.line_len(p.line) {
            Pos::new(p.line, p.col + 1)
        } else if p.line + 1 < self.lines.len() {
            Pos::new(p.line + 1, 0)
        } else {
            p
        }
    }

    /// The character at a position (`None` at the end of a line).
    pub fn char_at(&self, p: Pos) -> Option<char> {
        self.line(p.line).chars().nth(p.col)
    }

    /// Line `i`'s leading whitespace.
    pub fn indent_of(&self, i: usize) -> &str {
        let l = self.line(i);
        &l[..l.len() - l.trim_start().len()]
    }
}

/// Words, spaces and the rest, for word moves.
fn char_class(c: char) -> u8 {
    if c.is_alphanumeric() || c == '_' {
        0
    } else if c.is_whitespace() {
        1
    } else {
        2
    }
}

/// The byte offset of character `col` in `s` (its length past the end).
pub fn byte_at(s: &str, col: usize) -> usize {
    s.char_indices().nth(col).map_or(s.len(), |(i, _)| i)
}

/// The character column of byte offset `byte` in `s`.
pub fn col_at(s: &str, byte: usize) -> usize {
    s[..byte.min(s.len())].chars().count()
}

/// The width of an indentation in columns, a tab counting `tab` columns.
pub fn indent_width(indent: &str, tab: usize) -> usize {
    indent
        .chars()
        .map(|c| if c == '\t' { tab } else { 1 })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_gives_an_inverse_and_a_shift() {
        let mut b = Buffer::from_text("alpha\nbeta\ngamma");
        let (inv, shift) = b.apply(&Change::new(Pos::new(0, 2), Pos::new(1, 2), "X\nY"));
        assert_eq!(b.text(), "alX\nYta\ngamma");
        assert_eq!(
            shift,
            LineShift {
                line: 0,
                removed: 1,
                added: 1
            }
        );
        let (_, _) = b.apply(&inv);
        assert_eq!(b.text(), "alpha\nbeta\ngamma");
        let (inv, shift) = b.apply(&Change::insert(Pos::new(2, 5), "\nd\ne"));
        assert_eq!(b.line_count(), 5);
        assert_eq!(shift.map(1), Some(1));
        b.apply(&inv);
        assert_eq!(b.text(), "alpha\nbeta\ngamma");
        let (_, shift) = b.apply(&Change::new(Pos::new(0, 5), Pos::new(2, 0), ""));
        assert_eq!(b.text(), "alphagamma");
        assert_eq!(shift.map(1), None);
        assert_eq!(shift.map(3), Some(1));
    }

    #[test]
    fn columns_are_characters() {
        let mut b = Buffer::from_text("été\r\nok");
        assert_eq!(b.line(0), "été");
        assert_eq!(b.line_len(0), 3);
        b.apply(&Change::insert(Pos::new(0, 1), "x"));
        assert_eq!(b.line(0), "éxté");
        assert_eq!(b.slice(Span::new(Pos::new(0, 1), Pos::new(1, 1))), "xté\no");
        assert_eq!(end_of(Pos::new(3, 4), "ab\ncdé"), Pos::new(4, 3));
    }

    #[test]
    fn words() {
        let b = Buffer::from_text("let foo_bar = 1;");
        assert_eq!(
            b.word_at(Pos::new(0, 5)),
            Span::new(Pos::new(0, 4), Pos::new(0, 11))
        );
        assert_eq!(b.word_right(Pos::new(0, 0)), Pos::new(0, 3));
        assert_eq!(b.word_left(Pos::new(0, 11)), Pos::new(0, 4));
    }
}
