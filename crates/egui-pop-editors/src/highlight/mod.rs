//! Syntax highlighting behind a trait, so the grammar engine is the
//! application's choice.
//!
//! A [`Highlighter`] gives each line its [`Run`]s (byte ranges and their
//! [`Kind`]); the editor colours each kind from the theme's `syntax.*`
//! tokens, so code follows the theme like everything else. Highlighting
//! is incremental: the editor says from which line the text changed
//! ([`Highlighter::edited`]) and asks only for the lines in view; a
//! highlighter that must read from the top of the file to know a line
//! (syntect) does so within a budget per pass, and a line it has not
//! reached yet is drawn plain until it has.
//!
//! - [`Plain`]: no highlighting;
//! - [`SyntectHighlighter`] (feature `syntect`, on by default): Sublime
//!   Text grammars through syntect, with its bundled syntaxes;
//! - `TreeSitterHighlighter` (feature `tree-sitter`): tree-sitter's
//!   highlight queries over a grammar the application brings.

use std::ops::Range;
use std::sync::Arc;

use serde::Serialize;

use crate::buffer::Buffer;

#[cfg(feature = "syntect")]
mod syntect;
#[cfg(feature = "syntect")]
pub use self::syntect::SyntectHighlighter;

#[cfg(feature = "tree-sitter")]
mod tree_sitter;
#[cfg(feature = "tree-sitter")]
pub use self::tree_sitter::TreeSitterHighlighter;

/// What a piece of code is; each kind has a `syntax.*` token.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Kind {
    Plain,
    Keyword,
    String,
    Number,
    Constant,
    Escape,
    Comment,
    Function,
    Type,
    Macro,
    Tag,
    Operator,
    Punctuation,
    Invalid,
}

impl Kind {
    pub const ALL: [Kind; 14] = [
        Kind::Plain,
        Kind::Keyword,
        Kind::String,
        Kind::Number,
        Kind::Constant,
        Kind::Escape,
        Kind::Comment,
        Kind::Function,
        Kind::Type,
        Kind::Macro,
        Kind::Tag,
        Kind::Operator,
        Kind::Punctuation,
        Kind::Invalid,
    ];

    /// Its token's name under `syntax.`.
    pub fn token(self) -> &'static str {
        match self {
            Kind::Plain => "plain",
            Kind::Keyword => "keyword",
            Kind::String => "string",
            Kind::Number => "number",
            Kind::Constant => "constant",
            Kind::Escape => "escape",
            Kind::Comment => "comment",
            Kind::Function => "function",
            Kind::Type => "type",
            Kind::Macro => "macro",
            Kind::Tag => "tag",
            Kind::Operator => "operator",
            Kind::Punctuation => "punctuation",
            Kind::Invalid => "invalid",
        }
    }

    /// The kind of a TextMate scope or a tree-sitter capture name
    /// (`keyword.control.rust`, `string.quoted.double`, `function.call`),
    /// by its first matching prefix; `None` when it says nothing about
    /// colour.
    pub fn of_scope(scope: &str) -> Option<Kind> {
        const TABLE: &[(&str, Kind)] = &[
            ("comment", Kind::Comment),
            ("constant.character.escape", Kind::Escape),
            ("string.escape", Kind::Escape),
            ("escape", Kind::Escape),
            ("string", Kind::String),
            ("constant.numeric", Kind::Number),
            ("number", Kind::Number),
            ("constant", Kind::Constant),
            ("boolean", Kind::Constant),
            ("keyword.operator", Kind::Operator),
            ("operator", Kind::Operator),
            ("keyword", Kind::Keyword),
            ("storage.type.function", Kind::Keyword),
            ("storage", Kind::Keyword),
            ("entity.name.function", Kind::Function),
            ("support.function", Kind::Function),
            ("variable.function", Kind::Function),
            ("function", Kind::Function),
            ("entity.name.type", Kind::Type),
            ("entity.name.struct", Kind::Type),
            ("entity.name.enum", Kind::Type),
            ("entity.name.trait", Kind::Type),
            ("entity.name.class", Kind::Type),
            ("support.type", Kind::Type),
            ("support.class", Kind::Type),
            ("type", Kind::Type),
            ("constructor", Kind::Type),
            ("support.macro", Kind::Macro),
            ("entity.name.macro", Kind::Macro),
            ("meta.attribute", Kind::Macro),
            ("attribute", Kind::Macro),
            ("macro", Kind::Macro),
            ("entity.name.tag", Kind::Tag),
            ("tag", Kind::Tag),
            ("entity.other.attribute-name", Kind::Macro),
            ("punctuation", Kind::Punctuation),
            ("invalid", Kind::Invalid),
        ];
        TABLE
            .iter()
            .find(|(p, _)| {
                scope == *p
                    || (scope.starts_with(p) && scope.as_bytes().get(p.len()) == Some(&b'.'))
            })
            .map(|(_, k)| *k)
    }
}

/// A piece of a line: a byte range and its kind.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Run {
    pub bytes: Range<usize>,
    pub kind: Kind,
}

/// A line's runs, shared with the editor's cache.
pub type Runs = Arc<[Run]>;

/// Gives lines their runs. Implemented for syntect and tree-sitter here;
/// an application may bring its own (a language server's semantic
/// tokens).
pub trait Highlighter: Send {
    /// A name for the debug bridge (`syntect: Rust`).
    fn name(&self) -> String;

    /// The text changed from line `line` on; what was known about lines
    /// before it still holds.
    fn edited(&mut self, line: usize);

    /// The runs of the lines in `lines`, in order: `None` for a line not
    /// reached yet (drawn plain, asked again at the next pass). `budget`
    /// is how many lines it may parse to get there this pass.
    fn highlight(
        &mut self,
        buffer: &Buffer,
        lines: Range<usize>,
        budget: usize,
    ) -> Vec<Option<Runs>>;

    /// Whether lines before `line` are all known (no need to ask again).
    fn ready(&self, line: usize) -> bool {
        let _ = line;
        true
    }
}

/// No highlighting: every line plain.
#[derive(Clone, Copy, Debug, Default)]
pub struct Plain;

impl Highlighter for Plain {
    fn name(&self) -> String {
        "plain".into()
    }

    fn edited(&mut self, _: usize) {}

    fn highlight(&mut self, buffer: &Buffer, lines: Range<usize>, _: usize) -> Vec<Option<Runs>> {
        lines
            .map(|i| {
                let len = buffer.line(i).len();
                Some(Arc::from(vec![Run {
                    bytes: 0..len,
                    kind: Kind::Plain,
                }]))
            })
            .collect()
    }
}

/// The highlighter for a file name or a language name: syntect's when
/// the feature is on and it knows the language, else [`Plain`].
pub fn for_language(name: &str) -> Box<dyn Highlighter> {
    #[cfg(feature = "syntect")]
    if let Some(h) = SyntectHighlighter::for_language(name) {
        return Box::new(h);
    }
    let _ = name;
    Box::new(Plain)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scopes_map_by_prefix() {
        assert_eq!(Kind::of_scope("keyword.control.rust"), Some(Kind::Keyword));
        assert_eq!(
            Kind::of_scope("keyword.operator.arithmetic"),
            Some(Kind::Operator)
        );
        assert_eq!(
            Kind::of_scope("constant.numeric.integer"),
            Some(Kind::Number)
        );
        assert_eq!(Kind::of_scope("string.quoted.double"), Some(Kind::String));
        assert_eq!(Kind::of_scope("function.call"), Some(Kind::Function));
        assert_eq!(Kind::of_scope("functional"), None);
        assert_eq!(Kind::of_scope("source.rust"), None);
    }
}
