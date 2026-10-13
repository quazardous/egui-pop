//! Text and code editors for egui-pop.
//!
//! - [`TextSurface`]: a multi-line text editor;
//! - [`CodeEditor`]: the same surface for code, with a gutter (line
//!   numbers, markers the application pushes), highlighting behind the
//!   [`Highlighter`](highlight::Highlighter) trait (syntect by default,
//!   tree-sitter behind a feature), folding, matching brackets and
//!   indentation guides;
//! - [`EditorState`]: the text, selection, history, marks and search,
//!   a handle shared with the application and the commands agents run;
//! - [`steps`]: `edit.change`, `edit.save` and `code.diagnostics`, the
//!   editors' commands and collectors, and [`EditorsPlugin`].
//!
//! Both are virtualised (only the lines in view are laid out: 100 000
//! lines cost a screenful), take the theme's tokens (`editor.*`,
//! `syntax.*`), behave as text inputs for egui-pop's focus rules
//! (Escape leaves them; plain keys stay with them) and bind their
//! shortcuts in the keymap on the `editor` surface.
//!
//! Layer: a component on the application API (L4, ADR 0001).

pub mod assist;
mod assist_ui;
pub mod buffer;
pub mod code;
mod find_bar;
pub mod fold;
pub mod highlight;
pub mod history;
pub mod markers;
pub mod model;
pub mod search;
pub mod selection;
pub mod steps;
pub mod strings;
pub mod text;
pub mod tokens;
mod view;

pub use code::CodeEditor;
pub use model::EditorState;
pub use steps::EditorsPlugin;
pub use text::TextSurface;
pub use view::{EditorOutput, Signal};
