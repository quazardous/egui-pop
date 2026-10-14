//! One set of tones for every mark: icons, progress bars and rings,
//! status bar items, library badges and indicators. A tone names what a
//! colour says, never a hue; the theme gives the colour.
//!
//! ```
//! use egui_pop_core::theme::Tokens;
//! use egui_pop_core::tone::Tone;
//!
//! let ctx = egui::Context::default();
//! egui_pop_core::theme::install(&ctx, &egui_pop_core::theme::ThemeSpec::new("pop-dark"));
//! let t = Tokens::of(&ctx);
//! assert_eq!(Tone::Danger.color(&t), t.status.danger);
//! // Recording is not an error, but looks like one unless the theme says otherwise.
//! assert_eq!(Tone::Record.color(&t), t.status.danger);
//! assert_eq!(Tone::Record.name(), "record");
//! ```

use egui::Color32;
use serde::Serialize;

use crate::theme::Tokens;

/// What a mark's colour says.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Tone {
    /// Nothing particular: ordinary text (`text.default`). Components
    /// may draw it quieter (a status bar item at rest is muted).
    #[default]
    Neutral,
    /// Secondary, paused, skipped (`text.muted`).
    Muted,
    /// Emphasis (`text.strong`).
    Strong,
    /// Something going on, a primary state (`accent.default`).
    Accent,
    /// On an accent fill (`text.on-accent`).
    OnAccent,
    Info,
    Success,
    Warning,
    /// An error, a failure, a destructive action (`danger`).
    Danger,
    /// Recording (`indicator.record`, the danger colour unless the theme
    /// tells them apart).
    Record,
}

impl Tone {
    /// Every tone, in the order above.
    pub const ALL: [Tone; 10] = [
        Tone::Neutral,
        Tone::Muted,
        Tone::Strong,
        Tone::Accent,
        Tone::OnAccent,
        Tone::Info,
        Tone::Success,
        Tone::Warning,
        Tone::Danger,
        Tone::Record,
    ];

    /// Its name, as serialized (`on_accent`).
    pub fn name(self) -> &'static str {
        match self {
            Tone::Neutral => "neutral",
            Tone::Muted => "muted",
            Tone::Strong => "strong",
            Tone::Accent => "accent",
            Tone::OnAccent => "on_accent",
            Tone::Info => "info",
            Tone::Success => "success",
            Tone::Warning => "warning",
            Tone::Danger => "danger",
            Tone::Record => "record",
        }
    }

    /// The tone of a name ([`Tone::name`]).
    pub fn from_name(name: &str) -> Option<Tone> {
        Tone::ALL.into_iter().find(|t| t.name() == name)
    }

    /// Its colour in the theme, as a foreground (text, an icon, a dot).
    pub fn color(self, t: &Tokens) -> Color32 {
        match self {
            Tone::Neutral => t.text.default,
            Tone::Muted => t.text.muted,
            Tone::Strong => t.text.strong,
            Tone::Accent => t.accent.default,
            Tone::OnAccent => t.text.on_accent,
            Tone::Info => t.status.info,
            Tone::Success => t.status.success,
            Tone::Warning => t.status.warning,
            Tone::Danger => t.status.danger,
            Tone::Record => t.color("indicator.record").unwrap_or(t.status.danger),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_read_back() {
        for tone in Tone::ALL {
            assert_eq!(Tone::from_name(tone.name()), Some(tone));
            assert_eq!(
                serde_json::to_value(tone).unwrap(),
                serde_json::json!(tone.name())
            );
        }
        assert_eq!(Tone::from_name("quiet"), None);
    }
}
