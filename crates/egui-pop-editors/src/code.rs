//! A code editor: the text surface in the monospace font, with line
//! numbers, a gutter of markers the application pushes (diagnostics,
//! breakpoints, diff), the cursor's line, matching brackets, indentation
//! guides, folding by indentation and syntax highlighting.
//!
//! ```
//! use egui_pop_editors::markers::{Diagnostic, Severity};
//! use egui_pop_editors::{CodeEditor, EditorState};
//!
//! let code = EditorState::new("main.rs", "fn main() {\n    let x = 1;\n}").language("rs");
//! code.set_diagnostics(None, vec![Diagnostic::new(1, Severity::Warning, "unused variable `x`").columns(8, 9)])
//!     .unwrap();
//! fn ui(ui: &mut egui::Ui, code: &EditorState) {
//!     CodeEditor::new().label("main.rs").show(ui, code);
//! }
//! ```

use egui::Ui;
use egui_pop_api::AppHandle;

use crate::model::EditorState;
use crate::view::{self, EditorOutput, Options};

/// How many lines a highlighter may parse per pass by default.
pub const HIGHLIGHT_BUDGET: usize = 500;

/// A code editor over an [`EditorState`].
#[derive(Debug)]
pub struct CodeEditor<'a> {
    label: Option<String>,
    app: Option<&'a AppHandle>,
    height: Option<f32>,
    line_numbers: bool,
    markers: bool,
    status: bool,
    budget: usize,
    assist: bool,
}

impl Default for CodeEditor<'_> {
    fn default() -> Self {
        Self {
            label: None,
            app: None,
            height: None,
            line_numbers: true,
            markers: true,
            status: true,
            budget: HIGHLIGHT_BUDGET,
            assist: true,
        }
    }
}

impl<'a> CodeEditor<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Its accessible name.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Runs its moments as steps of this application, registers it for
    /// commands, and lets the keymap carry its shortcuts.
    pub fn app(mut self, app: &'a AppHandle) -> Self {
        self.app = Some(app);
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    pub fn line_numbers(mut self, on: bool) -> Self {
        self.line_numbers = on;
        self
    }

    /// The breakpoint and diagnostic columns of the gutter.
    pub fn markers(mut self, on: bool) -> Self {
        self.markers = on;
        self
    }

    /// The cursor's line and column under the code.
    pub fn status(mut self, on: bool) -> Self {
        self.status = on;
        self
    }

    /// How many lines the highlighter may parse per pass to reach the
    /// lines in view.
    pub fn highlight_budget(mut self, lines: usize) -> Self {
        self.budget = lines;
        self
    }

    /// Code assist (on by default): the state's provider, or the text's
    /// words ([`WordCompletion`](crate::assist::WordCompletion)) when it
    /// has none.
    pub fn assist(mut self, on: bool) -> Self {
        self.assist = on;
        self
    }

    pub fn show(self, ui: &mut Ui, state: &EditorState) -> EditorOutput {
        if self.assist {
            let mut m = state.lock();
            if m.assist.provider.is_none() {
                m.assist.provider = Some(Box::new(crate::assist::WordCompletion::default()));
            }
        }
        view::show(
            ui,
            state,
            &Options {
                code: true,
                line_numbers: self.line_numbers,
                gutter_markers: self.markers,
                label: self.label,
                app: self.app,
                height: self.height,
                budget: self.budget,
                status: self.status,
            },
        )
    }
}
