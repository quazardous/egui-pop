//! The tally's component tokens.
//!
//! An extension cannot add tokens to the built-in themes, so each of its
//! component tokens is read by path and falls back on the system token
//! it aliases. A theme file that wants another look defines the path
//! (`tally.background`), as built-in components' tokens are defined;
//! every other theme gets the fallback, whose contrast the built-in
//! themes already guarantee (`contrast::PAIRS`). A test checks the
//! pairs below in every built-in theme.

use egui_pop::egui::{Color32, Context};
use egui_pop::theme::Tokens;

/// What a tally is drawn with.
#[derive(Clone, Debug, PartialEq)]
pub struct TallyTokens {
    /// Behind the count (`tally.background`, else `surface.raised`).
    pub background: Color32,
    /// The count's text (`tally.text`, else `text.default`).
    pub text: Color32,
    /// The outline (`tally.border`, else `border.strong`).
    pub border: Color32,
    /// The bar under a count above zero (`tally.accent`, else
    /// `accent.default`).
    pub accent: Color32,
    /// Corner radius in points (`radius.widget`).
    pub radius: f32,
    /// Inner padding in points (`space.s`).
    pub padding: f32,
    /// The control's height in points (`size.control.height`).
    pub height: f32,
}

impl TallyTokens {
    /// The tokens of the theme installed in `ctx`.
    pub fn of(ctx: &Context) -> TallyTokens {
        let t = Tokens::of(ctx);
        let c = |path: &str, fallback: Color32| t.color(path).unwrap_or(fallback);
        TallyTokens {
            background: c("tally.background", t.surface.raised),
            text: c("tally.text", t.text.default),
            border: c("tally.border", t.border.strong),
            accent: c("tally.accent", t.accent.default),
            radius: t.radius.widget,
            padding: t.space.s,
            height: t.size.control_height,
        }
    }
}
