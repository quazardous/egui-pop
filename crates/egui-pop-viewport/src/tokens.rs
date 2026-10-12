//! The viewport's component tokens (`viewport.*` in the theme files),
//! each an alias of a system token.

use egui::{Color32, Context};
use egui_pop_core::theme::Tokens;

/// The viewport's colours, from the installed theme.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewportTokens {
    /// Around the image.
    pub background: Color32,
    /// Under the image where no tile has arrived yet.
    pub backdrop: Color32,
    /// The image's outline.
    pub frame: Color32,
    /// The pixel grid at high zoom.
    pub grid: Color32,
    /// Overlay shapes and guides.
    pub guide: Color32,
    /// A handle at rest.
    pub handle: Color32,
    /// A handle under the pointer or dragged.
    pub handle_hover: Color32,
    /// The ring around a handle, so it reads on any image.
    pub handle_outline: Color32,
}

impl ViewportTokens {
    /// The tokens of the theme installed in `ctx`.
    pub fn of(ctx: &Context) -> ViewportTokens {
        let t = Tokens::of(ctx);
        let c = |path: &str, fallback: Color32| t.color(path).unwrap_or(fallback);
        ViewportTokens {
            background: c("viewport.background", t.surface.canvas),
            backdrop: c("viewport.backdrop", t.surface.sunken),
            frame: c("viewport.frame", t.border.default),
            grid: c("viewport.grid", t.border.default),
            guide: c("viewport.guide", t.text.strong),
            handle: c("viewport.handle", t.accent.default),
            handle_hover: c("viewport.handle-hover", t.accent.hover),
            handle_outline: c("viewport.handle-outline", t.surface.canvas),
        }
    }
}
