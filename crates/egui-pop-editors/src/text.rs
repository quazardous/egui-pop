//! A multi-line text editor in the theme's body font.
//!
//! ```
//! use egui_pop_editors::{EditorState, TextSurface};
//!
//! let notes = EditorState::new("notes", "Shopping\n- bread");
//! fn ui(ui: &mut egui::Ui, notes: &EditorState) {
//!     let out = TextSurface::new().label("Notes").show(ui, notes);
//!     let _ = out.signals;
//! }
//! ```

use egui::Ui;
use egui_pop_api::AppHandle;

use crate::model::EditorState;
use crate::view::{self, EditorOutput, Options};

/// A text editor: typing, selection, the clipboard, undo, search and
/// replace, Escape leaves it. Virtualised like the code editor; no soft
/// wrap (long lines scroll).
#[derive(Debug, Default)]
pub struct TextSurface<'a> {
    label: Option<String>,
    app: Option<&'a AppHandle>,
    height: Option<f32>,
    status: bool,
}

impl<'a> TextSurface<'a> {
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

    /// Its height (default: what is available).
    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    /// Shows the cursor's line and column under the text.
    pub fn status(mut self, status: bool) -> Self {
        self.status = status;
        self
    }

    pub fn show(self, ui: &mut Ui, state: &EditorState) -> EditorOutput {
        view::show(
            ui,
            state,
            &Options {
                code: false,
                line_numbers: false,
                gutter_markers: false,
                label: self.label,
                app: self.app,
                height: self.height,
                budget: 0,
                status: self.status,
            },
        )
    }
}
