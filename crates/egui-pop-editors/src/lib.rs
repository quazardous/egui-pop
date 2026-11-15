//! Text and code editors for egui-pop.
//!
//! <div class="warning">Experimental: may change in any release
//! (docs/stability.md).</div>
//!
//! - [`TextSurface`]: a multi-line text editor;
//! - [`CodeEditor`]: the same surface for code, with a gutter (line
//!   numbers, markers the application pushes), highlighting behind the
//!   [`Highlighter`](highlight::Highlighter) trait (syntect by default,
//!   tree-sitter behind a feature), folding, matching brackets and
//!   indentation guides;
//! - [`EditorState`]: the text, selection, history, marks and search,
//!   a handle shared with the application and the commands agents run;
//! - [`Terminal`]: a terminal over a
//!   [`TerminalState`] fed by a source (a
//!   program in a pseudo-terminal behind the `pty` feature, or a
//!   read-only [`Console`](terminal::Console) for logs), its steps,
//!   commands and [`TerminalPlugin`];
//! - [`mounts`]: what they need (the mount contract, ADR 0013);
//! - [`steps`]: `edit.change`, `edit.save` and `code.diagnostics`, the
//!   editors' commands and collectors, and [`EditorsPlugin`].
//!
//! Both are virtualised (only the lines in view are laid out: 100 000
//! lines cost a screenful), take the theme's tokens (`editor.*`,
//! `syntax.*`), behave as text inputs for egui-pop's focus rules
//! (Escape leaves them; plain keys stay with them) and bind their
//! shortcuts in the keymap on the `editor` surface.
//!
//! ## Without the application
//!
//! The `api` feature (on by default) brings the application layer: each
//! editor's `.app(&AppHandle)` and the plugins. Turned off
//! (`default-features = false`), an editor or a terminal is a plain egui
//! widget over egui-pop's core and model; the methods taking an
//! `Option<&AppHandle>` are given `None`.
//!
//! ```
//! use egui_pop_editors::{CodeEditor, EditorState, Signal};
//!
//! let code = EditorState::new("main.rs", "fn main() {}\n");
//! let ctx = egui::Context::default();
//! let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
//!     let out = CodeEditor::new().label("main.rs").height(200.0).show(ui, &code);
//!     for s in out.signals {
//!         if let Signal::Saved { .. } = s { /* write `code.text()` */ }
//!     }
//! });
//! out.textures_delta.clear();
//! code.undo(None, "me").unwrap();
//! ```
//!
//! Layer: a component on the application API (L4, ADR 0001); without
//! `api`, a component on core and model only.

mod app_link;
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
pub mod mounts;
pub mod search;
pub mod selection;
pub mod steps;
pub mod strings;
pub mod terminal;
pub mod text;
pub mod tokens;
mod view;

pub use code::CodeEditor;
pub use model::EditorState;
#[cfg(feature = "api")]
pub use steps::EditorsPlugin;
#[cfg(feature = "api")]
pub use terminal::TerminalPlugin;
#[cfg(feature = "api")]
pub use terminal::jobs::JobConsolesPlugin;
pub use terminal::{Terminal, TerminalState};

/// Without the `api` feature: the application handle the editors' methods
/// take as `Option<&AppHandle>`, a type with no value (pass `None`).
#[cfg(not(feature = "api"))]
pub use app_link::AppHandle;
pub use text::TextSurface;
pub use view::{EditorOutput, Signal};
