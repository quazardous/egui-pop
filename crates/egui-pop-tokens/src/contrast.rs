//! Contrast checks: the token pairs that must stay legible, and the WCAG
//! 2.x ratios they must reach.

use std::fmt;

use crate::ResolvedTokens;
pub use crate::color::contrast_ratio;

/// What a pair is: text to read, or a user-interface part to see.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairKind {
    /// Text over a background: 4.5:1 (WCAG 1.4.3), 7:1 in high contrast
    /// (1.4.6).
    Text,
    /// A control's boundary, a state or a focus indicator over what is
    /// around it: 3:1 (WCAG 1.4.11).
    Ui,
}

/// The level a theme is checked at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    /// WCAG AA.
    Normal,
    /// WCAG AAA for text, for high-contrast themes.
    High,
}

impl Level {
    /// The ratio a pair of this kind must reach at this level.
    pub fn required(self, kind: PairKind) -> f32 {
        match (kind, self) {
            (PairKind::Text, Level::Normal) => 4.5,
            (PairKind::Text, Level::High) => 7.0,
            (PairKind::Ui, _) => 3.0,
        }
    }
}

/// A foreground token over a background token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pair {
    pub foreground: &'static str,
    pub background: &'static str,
    pub kind: PairKind,
}

const fn text(foreground: &'static str, background: &'static str) -> Pair {
    Pair {
        foreground,
        background,
        kind: PairKind::Text,
    }
}

const fn ui(foreground: &'static str, background: &'static str) -> Pair {
    Pair {
        foreground,
        background,
        kind: PairKind::Ui,
    }
}

/// Every pair checked, listed by hand: where text is drawn and where a
/// control must be seen. Decorative borders (`border.default`) are not
/// in it.
pub const PAIRS: &[Pair] = &[
    // Ordinary text wherever it is drawn.
    text("text.default", "surface.canvas"),
    text("text.default", "surface.panel"),
    text("text.default", "surface.raised"),
    text("text.default", "surface.hover"),
    text("text.default", "surface.overlay"),
    text("text.default", "surface.sunken"),
    text("text.default", "selection"),
    // Text under the pointer or pressed, and titles.
    text("text.strong", "surface.panel"),
    text("text.strong", "surface.hover"),
    text("text.strong", "surface.active"),
    // Secondary text.
    text("text.muted", "surface.canvas"),
    text("text.muted", "surface.panel"),
    text("text.muted", "surface.raised"),
    text("text.muted", "surface.overlay"),
    text("text.muted", "surface.sunken"),
    // Labels on accent fills, and the accent as link text.
    text("text.on-accent", "accent.default"),
    text("text.on-accent", "accent.hover"),
    text("accent.default", "surface.panel"),
    // Status colours are used as text.
    text("danger", "surface.panel"),
    text("danger", "surface.overlay"),
    text("warning", "surface.panel"),
    text("warning", "surface.overlay"),
    text("success", "surface.panel"),
    text("success", "surface.overlay"),
    text("info", "surface.panel"),
    text("info", "surface.overlay"),
    // Components.
    text("button.text", "button.background"),
    text("menu.item.text", "menu.item.background-hover"),
    text("menu.item.text-muted", "surface.overlay"),
    text("toast.text", "toast.background"),
    text("dialog.header-text", "dialog.header"),
    // Editors: code over its background, line numbers over the gutter.
    text("editor.line-number", "editor.gutter"),
    text("editor.line-number-active", "editor.gutter"),
    text("syntax.plain", "editor.background"),
    text("syntax.plain", "editor.current-line"),
    text("syntax.plain", "editor.selection"),
    text("syntax.keyword", "editor.background"),
    text("syntax.string", "editor.background"),
    text("syntax.number", "editor.background"),
    text("syntax.comment", "editor.background"),
    text("syntax.function", "editor.background"),
    text("syntax.type", "editor.background"),
    text("syntax.invalid", "editor.background"),
    ui("editor.cursor", "editor.background"),
    ui("editor.error", "editor.gutter"),
    ui("editor.warning", "editor.gutter"),
    ui("editor.breakpoint", "editor.gutter"),
    // The terminal: its text over its background and the selection.
    text("terminal.foreground", "terminal.background"),
    text("terminal.foreground", "terminal.selection"),
    ui("terminal.cursor", "terminal.background"),
    // Things that must be seen.
    ui("border.strong", "surface.panel"),
    ui("border.strong", "surface.raised"),
    ui("border.strong", "surface.overlay"),
    ui("focus.ring", "surface.panel"),
    ui("focus.ring", "surface.raised"),
    ui("focus.ring", "surface.overlay"),
    ui("focus.ring", "surface.sunken"),
    ui("accent.default", "surface.raised"),
    ui("slider.fill", "slider.track"),
    ui("slider.thumb", "slider.track"),
    ui("toast.border", "surface.panel"),
];

/// A pair under its required ratio, or with a token missing.
#[derive(Clone, Debug, PartialEq)]
pub struct Failure {
    pub pair: Pair,
    /// The ratio reached; `None` when a token is not a colour.
    pub ratio: Option<f32>,
    pub required: f32,
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Pair {
            foreground,
            background,
            ..
        } = self.pair;
        match self.ratio {
            Some(r) => write!(
                f,
                "{foreground} on {background}: {r:.2}:1, needs {}:1",
                self.required
            ),
            None => write!(f, "{foreground} on {background}: not two colours"),
        }
    }
}

/// The ratio of one pair in a theme, if both tokens are colours.
pub fn pair_ratio(tokens: &ResolvedTokens, pair: &Pair) -> Option<f32> {
    Some(contrast_ratio(
        tokens.color(pair.foreground)?,
        tokens.color(pair.background)?,
    ))
}

/// Checks every pair of [`PAIRS`] at a level; empty when all pass.
pub fn check(tokens: &ResolvedTokens, level: Level) -> Vec<Failure> {
    PAIRS
        .iter()
        .filter_map(|pair| {
            let required = level.required(pair.kind);
            let ratio = pair_ratio(tokens, pair);
            ratio.is_none_or(|r| r < required).then_some(Failure {
                pair: *pair,
                ratio,
                required,
            })
        })
        .collect()
}
