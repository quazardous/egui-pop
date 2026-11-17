//! What draws into a window's own surfaces besides egui, per window, as
//! the part drawing it says it: the image viewport's image surface (its
//! format, colour space, route, or why it has none). The window registry
//! (`windows.list`) shows it with each window; the part sets it each
//! frame it draws and clears it when the window closes.
//!
//! Also how the window itself was built for such a part
//! ([`Transparency`]): a surface below egui shows through a transparent
//! window only, so whoever builds a window that may show one says
//! whether it is transparent, or why not ([`set_transparency`]); the part
//! reads it before making its surface.
//!
//! <div class="warning">Experimental: may change in any release
//! (docs/stability.md).</div>

use egui::{Context, ViewportId};
use serde_json::Value as Json;

use crate::state::Slot;

const SURFACE: Slot<Json> = Slot::new("core.window_surface");
const TRANSPARENCY: Slot<Transparency> = Slot::new("core.window_transparency");

/// How a window was built for a surface below egui (the image surface):
/// transparent, so it shows through, or opaque and why.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Transparency {
    /// Built transparent (`ViewportBuilder::with_transparent(true)`).
    Transparent,
    /// Built opaque, and why (the platform has no surface to show
    /// through it, or the application did not ask).
    Opaque(String),
}

impl Transparency {
    /// Whether the window is transparent.
    pub fn is_transparent(&self) -> bool {
        matches!(self, Transparency::Transparent)
    }

    /// Why the window is opaque; none when it is transparent.
    pub fn why_opaque(&self) -> Option<&str> {
        match self {
            Transparency::Transparent => None,
            Transparency::Opaque(why) => Some(why),
        }
    }
}

/// Says how `viewport`'s window was built ([`Transparency`]); `None`
/// forgets it.
pub fn set_transparency(ctx: &Context, viewport: ViewportId, transparency: Option<Transparency>) {
    let slot = TRANSPARENCY.keyed(viewport);
    match transparency {
        Some(t) => {
            if slot.get(ctx).as_ref() != Some(&t) {
                slot.set(ctx, t);
            }
        }
        None => {
            slot.remove(ctx);
        }
    }
}

/// How `viewport`'s window was built, as said ([`set_transparency`]).
pub fn transparency(ctx: &Context, viewport: ViewportId) -> Option<Transparency> {
    TRANSPARENCY.keyed(viewport).get(ctx)
}

/// Says what draws into `viewport`'s own surface now; `None` forgets it
/// (the window closed, or nothing draws there any more).
pub fn set(ctx: &Context, viewport: ViewportId, state: Option<Json>) {
    let slot = SURFACE.keyed(viewport);
    match state {
        Some(s) => slot.set(ctx, s),
        None => {
            slot.remove(ctx);
        }
    }
}

/// What was said for `viewport` ([`set`]).
pub fn get(ctx: &Context, viewport: ViewportId) -> Option<Json> {
    SURFACE.keyed(viewport).get(ctx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kept_per_window_and_forgotten() {
        let ctx = Context::default();
        let other = ViewportId::from_hash_of("other");
        set(&ctx, other, Some(serde_json::json!({ "route": "tagged" })));
        assert_eq!(get(&ctx, other).unwrap()["route"], "tagged");
        assert_eq!(get(&ctx, ViewportId::ROOT), None);
        set(&ctx, other, None);
        assert_eq!(get(&ctx, other), None);
    }

    #[test]
    fn transparency_kept_per_window_with_its_reason() {
        let ctx = Context::default();
        let other = ViewportId::from_hash_of("other");
        set_transparency(&ctx, other, Some(Transparency::Opaque("X11".into())));
        set_transparency(&ctx, ViewportId::ROOT, Some(Transparency::Transparent));
        assert_eq!(transparency(&ctx, other).unwrap().why_opaque(), Some("X11"));
        assert!(
            transparency(&ctx, ViewportId::ROOT)
                .unwrap()
                .is_transparent()
        );
        assert_eq!(
            serde_json::to_value(Transparency::Opaque("X11".into())).unwrap(),
            serde_json::json!({ "opaque": "X11" })
        );
        set_transparency(&ctx, other, None);
        assert_eq!(transparency(&ctx, other), None);
    }
}
