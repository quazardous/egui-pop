//! What draws into a window's own surfaces besides egui, per window, as
//! the part drawing it says it: the image viewport's image surface (its
//! format, colour space, route, or why it has none). The window registry
//! (`windows.list`) shows it with each window; the part sets it each
//! frame it draws and clears it when the window closes.
//!
//! <div class="warning">Experimental: may change in any release
//! (docs/stability.md).</div>

use egui::{Context, ViewportId};
use serde_json::Value as Json;

use crate::state::Slot;

const SURFACE: Slot<Json> = Slot::new("core.window_surface");

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
}
