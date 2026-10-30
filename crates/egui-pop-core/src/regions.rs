//! F6 through a window's regions: the parts of the window (an area, a palette,
//! the status bar), each a [`focus::region`].
//!
//! [`next`] says where the keyboard goes; whoever binds F6 moves it there (an
//! application: the commands `workbench.next_region` and
//! `workbench.previous_region`). It changes no Tab order: a region is a focus
//! scope, not a [`tab::scope`].
//!
//! - **The order** is the regions' ranks, and it wraps. From nothing it starts
//!   at the first, or the last going back; after Escape or a press, nothing
//!   holds the focus and the focus chain still says where the user is, so it
//!   goes on from that region.
//! - **Where it lands**: the widget focused there last, if it can still take the
//!   focus; else the one the region names with [`focus::prefer`]; else the first
//!   focusable widget of a scope inside the region, the content before what
//!   stands around it (a dock's tabs, a palette's band); else nowhere, for a
//!   [`Region::holding`], whose content then holds the focus itself; else any
//!   focusable widget of the region in view. A region with nothing to land on
//!   is passed over.
//! - **Panel [`tab::scope`]s** of a region take that region's
//!   turn, each a stop of the round, at the stop focused there last or its
//!   first; what lies around them is reached by Tab. F6 goes on out of them.
//! - **Nothing moves behind a modal**, overlay or scope.
//! - **A focused widget taking every key** (a terminal) gets F6 before any
//!   binding does.
//! - **Each window has its own regions**, as it has its own focus.
//!
//! upstream: egui lists no focusable widgets, but its widget rectangles carry
//! each widget's sense in the order drawn, as a scope's widgets are recorded;
//! the last pass's are enough to choose from before anything is drawn.
//!
//! [`Region::holding`]: crate::focus::Region::holding
//!
//! ```
//! use egui_pop_core::{focus, regions};
//!
//! fn window(ui: &mut egui::Ui) {
//!     focus::region(ui, &focus::Region::new("list", 0), |ui| {
//!         let _ = ui.button("Row");
//!     });
//!     focus::region(ui, &focus::Region::new("status", 1), |ui| {
//!         let _ = ui.button("Item");
//!     });
//!     if ui.input(|i| i.key_pressed(egui::Key::F6))
//!         && let Some(regions::Hop { target: Some(id), .. }) = regions::next(ui.ctx(), false)
//!     {
//!         ui.memory_mut(|m| m.request_focus(id));
//!     }
//! }
//! ```

use egui::{Context, Id, WidgetRect};

use crate::focus::{self, DrawnRegion};
use crate::{overlay, place, tab};

/// Where F6 goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hop {
    /// The region's name.
    pub region: String,
    /// The widget to focus. `None`: nothing in the region's scopes can take
    /// the focus, and the region said its content can hold it
    /// ([`Region::holding`](crate::focus::Region::holding)).
    pub target: Option<Id>,
}

/// The next region's landing, or the previous one's with `back`. `None` behind
/// a modal, or with nowhere to go.
pub fn next(ctx: &Context, back: bool) -> Option<Hop> {
    if overlay::modal_open(ctx) || tab::modal_drawn(ctx) {
        return None;
    }
    let (regions, current) = focus::drawn_regions(ctx);
    let hops = hops(ctx, &regions);
    if hops.is_empty() {
        return None;
    }
    let n = hops.len();
    let at = match (hops.iter().position(|h| h.here), current) {
        (Some(p), _) if back => (p + n - 1) % n,
        (Some(p), _) => (p + 1) % n,
        (None, None) if back => n - 1,
        (None, None) => 0,
        (None, Some(c)) if back => hops.iter().rposition(|h| h.region < c).unwrap_or(n - 1),
        // Its panel scopes are its turn, and the focus is in none of them.
        (None, Some(c)) => {
            let first = |h: &Step| h.region > c || (h.region == c && h.sub);
            hops.iter().position(first).unwrap_or(0)
        }
    };
    let step = &hops[at];
    Some(Hop {
        region: regions[step.region].name.clone(),
        target: step.target,
    })
}

/// One stop of F6's round.
struct Step {
    /// Its region, as an index.
    region: usize,
    target: Option<Id>,
    /// A panel scope in the region.
    sub: bool,
    /// The focus is in that scope.
    here: bool,
}

/// F6's round through the regions, skipping those with nothing to land on.
fn hops(ctx: &Context, regions: &[DrawnRegion]) -> Vec<Step> {
    let panels = tab::panel_hops(ctx);
    let mut out = Vec::new();
    for (i, r) in regions.iter().enumerate() {
        let inside = panels.iter().filter(|p| {
            let at = ctx.viewport(|v| v.prev_pass.widgets.order(p.first));
            at.is_some_and(|(layer, order)| layer == r.layer && r.widgets.contains(&order))
        });
        let before = out.len();
        out.extend(inside.map(|p| Step {
            region: i,
            target: Some(p.target),
            sub: true,
            here: p.here,
        }));
        if out.len() == before
            && let Some(target) = landing(ctx, r)
        {
            out.push(Step {
                region: i,
                target,
                sub: false,
                here: false,
            });
        }
    }
    out
}

/// Where a region takes the focus, in [the order](self): `None` for nowhere,
/// `Some(None)` for its content holding it.
fn landing(ctx: &Context, r: &DrawnRegion) -> Option<Option<Id>> {
    let screen = ctx.viewport_rect();
    ctx.viewport(|v| {
        let drawn: Vec<&WidgetRect> = v.prev_pass.widgets.get_layer(r.layer).collect();
        let own = drawn.get(r.widgets.clone()).unwrap_or_default();
        let usable = |id: &Id| own.iter().any(|w| w.id == *id && place::can_focus(w));
        let named = r
            .remembered
            .iter()
            .chain(&r.preferred)
            .find(|id| usable(id));
        if let Some(id) = named {
            return Some(Some(*id));
        }
        let in_view = |w: &&&WidgetRect| {
            place::can_focus(w) && w.interact_rect.intersect(screen).is_positive()
        };
        let in_scope = |offset: usize| {
            let order = r.widgets.start + offset;
            r.inner.iter().any(|span| span.contains(&order))
        };
        let inner = own
            .iter()
            .enumerate()
            .filter(|(i, _)| in_scope(*i))
            .map(|(_, w)| w)
            .find(in_view);
        if let Some(w) = inner {
            return Some(Some(w.id));
        }
        if r.holding {
            return Some(None);
        }
        let any = own.iter().find(in_view);
        any.map(|w| Some(w.id))
    })
}
