//! Contrast checks: the token pairs that must stay legible, and the WCAG
//! 2.x ratios they must reach.

use std::fmt;

pub use crate::color::contrast_ratio;
use crate::contract::CATEGORY;
use crate::{Color, ResolvedTokens};

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
    // Text by role, on panels and on the raised surface cards use.
    text("text-role.title.color", "surface.panel"),
    text("text-role.title.color", "surface.raised"),
    text("text-role.section.color", "surface.panel"),
    text("text-role.section.color", "surface.raised"),
    text("text-role.body.color", "surface.panel"),
    text("text-role.body.color", "surface.raised"),
    text("text-role.muted.color", "surface.panel"),
    text("text-role.muted.color", "surface.raised"),
    text("text-role.meta.color", "surface.panel"),
    text("text-role.meta.color", "surface.raised"),
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
    // Log levels colour a log view's text, on the canvas or a panel.
    text("palette.log.trace", "surface.canvas"),
    text("palette.log.trace", "surface.panel"),
    text("palette.log.debug", "surface.canvas"),
    text("palette.log.debug", "surface.panel"),
    text("palette.log.info", "surface.canvas"),
    text("palette.log.info", "surface.panel"),
    text("palette.log.warn", "surface.canvas"),
    text("palette.log.warn", "surface.panel"),
    text("palette.log.error", "surface.canvas"),
    text("palette.log.error", "surface.panel"),
    text("palette.log.critical", "surface.canvas"),
    text("palette.log.critical", "surface.panel"),
    // Components.
    text("button.rest.text", "button.rest.fill"),
    // Buttons by tone: the label on the tone's fill.
    text("button.tone.accent.text", "button.tone.accent.fill"),
    text(
        "button.tone.accent.hover.text",
        "button.tone.accent.hover.fill",
    ),
    text("button.tone.info.text", "button.tone.info.fill"),
    text("button.tone.success.text", "button.tone.success.fill"),
    text("button.tone.warning.text", "button.tone.warning.fill"),
    text("button.tone.danger.text", "button.tone.danger.fill"),
    text("button.tone.record.text", "button.tone.record.fill"),
    text("menu-item.rest.text", "menu-item.hover.fill"),
    text("menu-item.tone.muted.text", "surface.overlay"),
    text("toast.rest.text", "toast.rest.fill"),
    text("field.rest.text", "field.rest.fill"),
    text("field.rest.text", "field.tone.muted.fill"),
    text("field.tone.muted.text", "field.rest.fill"),
    text("field.tone.danger.border-color", "surface.panel"),
    text("dialog-header.rest.text", "header-band.rest.fill"),
    text("header-band.rest.text", "header-band.rest.fill"),
    text("band-icon-button.rest.text", "header-band.rest.fill"),
    text(
        "command-palette-row.rest.text",
        "command-palette-row.selected.fill",
    ),
    text("pill.rest.text", "pill.rest.fill"),
    text("readout.rest.text", "readout.rest.fill"),
    // Editors: code over its background, line numbers over the gutter.
    text("editor.line-number", "editor.gutter"),
    text("editor.line-number-active", "editor.gutter"),
    text("syntax.plain", "editor-frame.rest.fill"),
    text("syntax.plain", "editor.current-line"),
    text("syntax.plain", "editor.selection"),
    text("syntax.keyword", "editor-frame.rest.fill"),
    text("syntax.string", "editor-frame.rest.fill"),
    text("syntax.number", "editor-frame.rest.fill"),
    text("syntax.comment", "editor-frame.rest.fill"),
    text("syntax.function", "editor-frame.rest.fill"),
    text("syntax.type", "editor-frame.rest.fill"),
    text("syntax.invalid", "editor-frame.rest.fill"),
    ui("editor.cursor", "editor-frame.rest.fill"),
    ui("editor.error", "editor.gutter"),
    ui("editor.warning", "editor.gutter"),
    ui("editor.breakpoint", "editor.gutter"),
    // The terminal: its text over its background and the selection.
    text("terminal.foreground", "terminal-frame.rest.fill"),
    text("terminal.foreground", "terminal.selection"),
    ui("terminal.cursor", "terminal-frame.rest.fill"),
    // A cursor's strokes against the ring that carries them over images.
    ui("cursor.fill", "cursor.outline"),
    // Things that must be seen.
    ui("border.strong", "surface.panel"),
    ui("border.strong", "surface.raised"),
    ui("border.strong", "surface.overlay"),
    ui("focus.ring", "surface.panel"),
    ui("focus.ring", "surface.raised"),
    ui("focus.ring", "surface.overlay"),
    ui("focus.ring", "surface.sunken"),
    ui("accent.default", "surface.raised"),
    ui("slider-fill.rest.fill", "slider.rest.fill"),
    ui("progress-fill.rest.fill", "progress-bar.rest.fill"),
    ui("slider-thumb.rest.fill", "slider.rest.fill"),
    // Input fields: the state edges and markers over the field.
    ui("field.tone.accent.border-color", "field.rest.fill"),
    ui("field.tone.warning.border-color", "field.rest.fill"),
    ui("field.tone.danger.border-color", "field.rest.fill"),
    ui("field.tone.info.border-color", "field.rest.fill"),
    // A card's edge in a tone, on the panel it sits on.
    ui("card.tone.accent.border-color", "surface.panel"),
    ui("card.tone.info.border-color", "surface.panel"),
    ui("card.tone.success.border-color", "surface.panel"),
    ui("card.tone.warning.border-color", "surface.panel"),
    ui("card.tone.danger.border-color", "surface.panel"),
    ui("card.tone.record.border-color", "surface.panel"),
    ui("toast.rest.border-color", "surface.panel"),
    // Micro-indicators: badge text on every tone's fill, the marks that
    // carry a state on the surfaces they sit on, the outline against
    // what it separates.
    text("indicator-badge.rest.text", "indicator-badge.rest.fill"),
    text(
        "indicator-badge.rest.text",
        "indicator-badge.tone.accent.fill",
    ),
    text(
        "indicator-badge.rest.text",
        "indicator-badge.tone.info.fill",
    ),
    text(
        "indicator-badge.rest.text",
        "indicator-badge.tone.success.fill",
    ),
    text(
        "indicator-badge.rest.text",
        "indicator-badge.tone.warning.fill",
    ),
    text(
        "indicator-badge.rest.text",
        "indicator-badge.tone.danger.fill",
    ),
    text(
        "indicator-badge.rest.text",
        "indicator-badge.tone.muted.fill",
    ),
    text(
        "indicator-badge.rest.text",
        "indicator-badge.tone.record.fill",
    ),
    ui("indicator-dot.tone.record.fill", "surface.panel"),
    ui("indicator-dot.tone.record.fill", "surface.overlay"),
    ui("indicator-dot.rest.border-color", "surface.panel"),
    ui("indicator-dot.rest.border-color", "surface.overlay"),
    ui("indicator-swatch.rest.border-color", "surface.panel"),
    ui("indicator-swatch.rest.border-color", "surface.overlay"),
    // A section's band: its title on the panel and under the pointer,
    // its count on its pill, its disclosure mark on the panel.
    text("section-band.rest.text", "surface.panel"),
    text("section-band.rest.text", "section-band.hover.fill"),
    text("section-count.rest.text", "section-count.rest.fill"),
    ui("section-disclosure.rest.text", "surface.panel"),
    ui(
        "indicator-dot.tone.accent.fill",
        "indicator-outline.rest.ring-color",
    ),
    ui(
        "indicator-dot.tone.danger.fill",
        "indicator-outline.rest.ring-color",
    ),
    // A curve and its points over the curve editor's background.
    ui("curve.line", "curve.background"),
    ui("curve.point", "curve.background"),
    // A selected node's outline over the node.
    ui("graph.node-selected", "graph.node"),
    // A hovered and a selected wire over the canvas.
    ui("graph.wire-hover.color", "graph.background"),
    ui("graph.wire-selected.color", "graph.background"),
    // A plot: its axis text and its time cursor, at rest and under the
    // pointer, over its background. Its grid (`plot.grid-color`) is a
    // decoration, as `border.default` is: not checked.
    text("plot.rest.text", "plot.rest.fill"),
    ui("plot-cursor.rest.fill", "plot.rest.fill"),
    ui("plot-cursor.hover.fill", "plot.rest.fill"),
    // The categorical palette's series, on the surfaces charts are
    // drawn on (WCAG 1.4.11, a graphical object).
    ui("palette.category.0", "surface.canvas"),
    ui("palette.category.0", "surface.panel"),
    ui("palette.category.0", "surface.raised"),
    ui("palette.category.1", "surface.canvas"),
    ui("palette.category.1", "surface.panel"),
    ui("palette.category.1", "surface.raised"),
    ui("palette.category.2", "surface.canvas"),
    ui("palette.category.2", "surface.panel"),
    ui("palette.category.2", "surface.raised"),
    ui("palette.category.3", "surface.canvas"),
    ui("palette.category.3", "surface.panel"),
    ui("palette.category.3", "surface.raised"),
    ui("palette.category.4", "surface.canvas"),
    ui("palette.category.4", "surface.panel"),
    ui("palette.category.4", "surface.raised"),
    ui("palette.category.5", "surface.canvas"),
    ui("palette.category.5", "surface.panel"),
    ui("palette.category.5", "surface.raised"),
    ui("palette.category.6", "surface.canvas"),
    ui("palette.category.6", "surface.panel"),
    ui("palette.category.6", "surface.raised"),
    ui("palette.category.7", "surface.canvas"),
    ui("palette.category.7", "surface.panel"),
    ui("palette.category.7", "surface.raised"),
    ui("palette.category.8", "surface.canvas"),
    ui("palette.category.8", "surface.panel"),
    ui("palette.category.8", "surface.raised"),
    ui("palette.category.9", "surface.canvas"),
    ui("palette.category.9", "surface.panel"),
    ui("palette.category.9", "surface.raised"),
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

/// How a colour is seen, for telling the categorical palette's slots
/// apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Vision {
    /// Full colour vision.
    Normal,
    /// No long-wavelength (red) cones.
    Protanopia,
    /// No medium-wavelength (green) cones.
    Deuteranopia,
}

impl Vision {
    pub const ALL: [Vision; 3] = [Vision::Normal, Vision::Protanopia, Vision::Deuteranopia];

    pub fn name(self) -> &'static str {
        match self {
            Vision::Normal => "normal vision",
            Vision::Protanopia => "protanopia",
            Vision::Deuteranopia => "deuteranopia",
        }
    }

    /// The colour as seen with this vision: Machado, Oliveira and
    /// Fernandes' model (2009) at severity 1, in linear light.
    pub fn simulate(self, c: Color) -> Color {
        const PROTAN: [[f64; 3]; 3] = [
            [0.152_286, 1.052_583, -0.204_868],
            [0.114_503, 0.786_281, 0.099_216],
            [-0.003_882, -0.048_116, 1.051_998],
        ];
        const DEUTAN: [[f64; 3]; 3] = [
            [0.367_322, 0.860_646, -0.227_968],
            [0.280_085, 0.672_501, 0.047_413],
            [-0.011_820, 0.042_940, 0.968_881],
        ];
        let m = match self {
            Vision::Normal => return c,
            Vision::Protanopia => &PROTAN,
            Vision::Deuteranopia => &DEUTAN,
        };
        let rgb = c.to_linear();
        let row = |i: usize| {
            (m[i][0] * rgb[0] + m[i][1] * rgb[1] + m[i][2] * rgb[2]).clamp(0.0, 1.0) as f32
        };
        Color::from_linear(row(0), row(1), row(2), c.a)
    }
}

/// The perceptual distance between two colours, as seen with `vision`:
/// the Euclidean distance in OKLab, times 100 (ΔE OK). About 2 is the
/// smallest difference seen side by side; categorical palettes aim for
/// 15 and more between neighbours.
pub fn delta_e(a: Color, b: Color, vision: Vision) -> f32 {
    let [l1, a1, b1] = vision.simulate(a).to_oklab();
    let [l2, a2, b2] = vision.simulate(b).to_oklab();
    (((l1 - l2).powi(2) + (a1 - a2).powi(2) + (b1 - b2).powi(2)).sqrt() * 100.0) as f32
}

/// Between two slots next to each other in the categorical palette
/// (series 1 and 2 of a chart), with full colour vision.
pub const CATEGORY_ADJACENT: f32 = 15.0;
/// Between any two slots of the categorical palette, with full colour
/// vision: a scatter or a legend puts any two side by side.
pub const CATEGORY_ANY: f32 = 10.0;
/// Between two slots next to each other, with protanopia or
/// deuteranopia.
pub const CATEGORY_CVD_ADJACENT: f32 = 8.0;

/// Two slots of the categorical palette told apart too little.
#[derive(Clone, Debug, PartialEq)]
pub struct Separation {
    /// The two slots, the lower first.
    pub slots: (usize, usize),
    pub vision: Vision,
    /// The distance reached ([`delta_e`]); `None` when a slot is not a
    /// colour.
    pub delta_e: Option<f32>,
    pub required: f32,
}

impl fmt::Display for Separation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (a, b) = self.slots;
        match self.delta_e {
            Some(d) => write!(
                f,
                "palette.category.{a} and .{b} ({}): ΔE {d:.1}, needs {}",
                self.vision.name(),
                self.required
            ),
            None => write!(f, "palette.category.{a} and .{b}: not two colours"),
        }
    }
}

/// Checks that the categorical palette's slots are told apart: every two
/// at [`CATEGORY_ANY`] and neighbours at [`CATEGORY_ADJACENT`] with full
/// colour vision, neighbours at [`CATEGORY_CVD_ADJACENT`] with
/// protanopia and deuteranopia. Empty when all pass. Their contrast with
/// the surfaces is in [`PAIRS`].
pub fn check_category(tokens: &ResolvedTokens) -> Vec<Separation> {
    let colors: Vec<Option<Color>> = CATEGORY.iter().map(|p| tokens.color(p)).collect();
    let mut out = Vec::new();
    for i in 0..colors.len() {
        for j in i + 1..colors.len() {
            let adjacent = j == i + 1;
            for vision in Vision::ALL {
                let required = match (vision, adjacent) {
                    (Vision::Normal, true) => CATEGORY_ADJACENT,
                    (Vision::Normal, false) => CATEGORY_ANY,
                    (_, true) => CATEGORY_CVD_ADJACENT,
                    (_, false) => continue,
                };
                let d = colors[i].zip(colors[j]).map(|(a, b)| delta_e(a, b, vision));
                if d.is_none_or(|d| d < required) {
                    out.push(Separation {
                        slots: (i, j),
                        vision,
                        delta_e: d,
                        required,
                    });
                }
            }
        }
    }
    out
}
