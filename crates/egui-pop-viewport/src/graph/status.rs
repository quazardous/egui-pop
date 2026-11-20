//! A node's status in its title bar ([`GraphEditor::status`](super::GraphEditor::status)):
//! a short word in a colour, its detail in a tooltip, read aloud as part
//! of the node's description.

use egui::Color32;
use egui_pop_core::kit::Tone;
use egui_pop_core::theme::Tokens;

/// The colour of a node's status.
#[derive(Clone, Debug, PartialEq)]
pub enum StatusColor {
    /// A kit tone, as text in that tone is drawn
    /// (`egui_pop_core::ui::tone_color`).
    Tone(Tone),
    /// A colour the application gives.
    Color(Color32),
    /// A theme token's colour by its path (`success`, `state.running`); the muted
    /// text's colour when the theme has no such token.
    Token(String),
}

impl Default for StatusColor {
    fn default() -> Self {
        StatusColor::Tone(Tone::Muted)
    }
}

impl StatusColor {
    /// The colour it draws in, with the theme's tokens.
    pub fn resolve(&self, tokens: &Tokens) -> Color32 {
        match self {
            StatusColor::Tone(tone) => egui_pop_core::ui::tone_color(tokens, *tone),
            StatusColor::Color(c) => *c,
            StatusColor::Token(path) => tokens.color(path).unwrap_or(tokens.text_role.muted.color),
        }
    }
}

/// A node's status: a short text drawn small at its title bar's right
/// end (left of the [`header`](super::GraphEditor::header) slot), in its
/// colour, its detail shown as a tooltip; both are part of the node's
/// accessible description.
///
/// ```
/// use egui_pop_core::kit::Tone;
/// use egui_pop_viewport::graph::{NodeStatus, StatusColor};
///
/// let s = NodeStatus::new("error")
///     .tone(Tone::Danger)
///     .detail("The input file is missing");
/// assert_eq!(s.color, StatusColor::Tone(Tone::Danger));
/// assert_eq!(s.description(), "error, The input file is missing");
/// ```
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NodeStatus {
    /// The short text ("running", "stale").
    pub text: String,
    /// Its colour (muted text by default).
    pub color: StatusColor,
    /// What it means in full, shown as a tooltip.
    pub detail: Option<String>,
}

impl NodeStatus {
    /// A status saying `text`, in muted text.
    pub fn new(text: impl Into<String>) -> Self {
        NodeStatus {
            text: text.into(),
            ..NodeStatus::default()
        }
    }

    /// In a kit tone.
    pub fn tone(mut self, tone: Tone) -> Self {
        self.color = StatusColor::Tone(tone);
        self
    }

    /// In a colour of the application's.
    pub fn color(mut self, color: Color32) -> Self {
        self.color = StatusColor::Color(color);
        self
    }

    /// In a theme token's colour, by its path (`success`, `state.running`).
    pub fn token(mut self, path: impl Into<String>) -> Self {
        self.color = StatusColor::Token(path.into());
        self
    }

    /// Its detail, shown as a tooltip.
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// What it adds to the node's accessible description: its text, then
    /// its detail.
    pub fn description(&self) -> String {
        match &self.detail {
            Some(d) if !d.is_empty() => format!("{}, {d}", self.text),
            _ => self.text.clone(),
        }
    }
}
