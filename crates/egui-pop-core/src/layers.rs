//! Layer ranks: a fixed order between the foreground layers of a window
//! (ADR 0026).
//!
//! egui has no z-index: inside an [`Order`], the layer shown or clicked
//! last goes on top. A panel lying over the centre and an application's
//! card both live in [`Order::Foreground`], so a click on the panel would
//! put it over the card. Ranks fix the order between kinds of layers,
//! bottom to top, in each window:
//!
//! 1. the docked panels and the central panel (egui's background);
//! 2. [`Rank::Over`]: the zones lying over their neighbour and flyouts;
//! 3. [`Rank::Overlay`]: egui-pop's and the application's overlays
//!    (cards, popovers, toasts, the zen way out);
//! 4. every foreground layer nobody ranked: menus, combo lists, context
//!    menus, dialogs;
//! 5. tooltips ([`Order::Tooltip`]).
//!
//! Inside a rank, egui's rule still holds: the layer clicked last is on
//! top.
//!
//! Each pinned rank has a base: an empty foreground layer egui-pop puts
//! at the bottom of the foreground on a window's first pass and never
//! raises. A ranked layer is made a sublayer of its rank's base each
//! pass ([`egui::Context::set_sublayer`]), so egui moves it right above
//! that base at the end of the pass, whatever was clicked.
//!
//! An application overlay calls [`area`] where it would show an
//! [`egui::Area`] (instead of calling `ctx.move_to_top` on it):
//!
//! ```no_run
//! # fn card(ctx: &egui::Context) {
//! use egui_pop_core::layers::{self, Rank};
//!
//! layers::area(
//!     ctx,
//!     egui::Area::new(egui::Id::unique("my_app.card")).anchor(egui::Align2::LEFT_BOTTOM, [8.0, -8.0]),
//!     Rank::Overlay,
//!     |ui| ui.label("Did you know?"),
//! );
//! # }
//! ```
//!
//! A layer drawn some other way (a [`egui::Context::layer_painter`])
//! calls [`rank`] each pass it is shown. A layer left unranked stays
//! above every ranked one; a layer to keep under the lying-over zones is
//! made a sublayer of [`base`]`(ctx, Rank::Over)` by its owner.
//!
//! [`begin_pass`] and [`end_pass`] run at the start and the end of every
//! pass of every window once [`install`] was called (the application
//! does).

use std::collections::HashMap;

use egui::{Context, Id, InnerResponse, LayerId, Order, Ui};

use crate::state::{lock, shared_in_window};

/// Where a foreground layer sits among the others of its window, bottom
/// to top (see [the module](self)). The ranks not listed here are
/// egui's: the background below, the unranked foreground layers and
/// tooltips above.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Rank {
    /// The zones lying over their neighbour and flyouts.
    Over,
    /// Overlays: cards, popovers, toasts, the zen way out, an
    /// application's own.
    Overlay,
}

impl Rank {
    /// Every rank, bottom to top.
    pub const ALL: [Rank; 2] = [Rank::Over, Rank::Overlay];

    fn name(self) -> &'static str {
        match self {
            Rank::Over => "over",
            Rank::Overlay => "overlay",
        }
    }
}

/// The base layer of `rank`: an empty foreground layer kept at the
/// bottom of the foreground, the ranked layers right above it. A host
/// drawing its own lying-over surface makes it a sublayer of
/// `base(ctx, Rank::Over)` (egui_stack's `LayoutArea::sublayer_of`).
///
/// The same in every window: each window has its own egui order.
pub fn base(_ctx: &Context, rank: Rank) -> LayerId {
    LayerId::new(
        Order::Foreground,
        Id::unique(("egui_pop.layers.base", rank.name())),
    )
}

/// An empty layer each base holds as its sublayer, so that the base is
/// in egui's order from the first pass on.
fn anchor(rank: Rank) -> LayerId {
    LayerId::new(
        Order::Foreground,
        Id::unique(("egui_pop.layers.anchor", rank.name())),
    )
}

/// The layers ranked in a window: this pass's and the previous one's.
#[derive(Debug, Default)]
struct Ranked {
    pass: u64,
    now: HashMap<LayerId, Rank>,
    last: HashMap<LayerId, Rank>,
}

impl Ranked {
    /// Moves on to `pass`: what was ranked in the pass before stays
    /// readable for this one.
    fn at(&mut self, pass: u64) -> &mut Self {
        if self.pass != pass {
            self.last = if self.pass + 1 == pass {
                std::mem::take(&mut self.now)
            } else {
                HashMap::new()
            };
            self.now.clear();
            self.pass = pass;
        }
        self
    }
}

fn note(ctx: &Context, layer: LayerId, rank: Rank) {
    let st = shared_in_window::<Ranked>(ctx, "core.layers.ranked");
    lock(&st)
        .at(ctx.cumulative_pass_nr())
        .now
        .insert(layer, rank);
}

/// The rank `layer` was given in this pass or the previous one, in the
/// window being drawn; `None` for a layer nobody ranked.
pub fn rank_of(ctx: &Context, layer: LayerId) -> Option<Rank> {
    if let Some(r) = Rank::ALL.into_iter().find(|r| base(ctx, *r) == layer) {
        return Some(r);
    }
    let st = shared_in_window::<Ranked>(ctx, "core.layers.ranked");
    let mut s = lock(&st);
    let s = s.at(ctx.cumulative_pass_nr());
    s.now.get(&layer).or_else(|| s.last.get(&layer)).copied()
}

/// Puts `layer` at `rank` for this pass: call it each pass the layer is
/// shown. Only a foreground layer can be ranked; another is left where
/// it is.
pub fn rank(ctx: &Context, layer: LayerId, rank: Rank) {
    if layer.order != Order::Foreground {
        log::debug!("layers: {layer:?} is not a foreground layer, left unranked");
        return;
    }
    ctx.set_sublayer(base(ctx, rank), layer);
    note(ctx, layer, rank);
}

/// Puts `layer` at `rank` when the layer it was opened from, `opener`,
/// lies at that rank or below it (a panel, a window, a lying-over zone,
/// another overlay); leaves it unranked, above its opener, when the
/// opener is above that rank (a dialog, a menu). Returns whether it was
/// ranked.
///
/// A hover card or a popover opened from a dialog thus stays above the
/// dialog.
pub fn rank_from(ctx: &Context, layer: LayerId, rank: Rank, opener: LayerId) -> bool {
    let below = match opener.order {
        Order::Background | Order::Middle => true,
        Order::Foreground => rank_of(ctx, opener).is_some_and(|r| r <= rank),
        Order::Tooltip | Order::Debug => false,
    };
    if below {
        self::rank(ctx, layer, rank);
    }
    below
}

/// Shows `area` in the foreground at `rank`: what an application calls
/// for its own overlay instead of `ctx.move_to_top`.
pub fn area<R>(
    ctx: &Context,
    area: egui::Area,
    rank: Rank,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> InnerResponse<R> {
    let r = area.order(Order::Foreground).show(ctx, add_contents);
    self::rank(ctx, r.response.layer_id, rank);
    r
}

/// Starts a pass of the window being drawn: puts the bases into egui's
/// order, `Over` under `Overlay`. On a window's first pass, before any
/// other foreground layer is shown, this puts them at the bottom of the
/// foreground, where they stay since nothing raises them. Calling it
/// again in a pass does nothing more.
pub fn begin_pass(ctx: &Context) {
    for r in Rank::ALL {
        ctx.set_sublayer(base(ctx, r), anchor(r));
    }
}

/// Ends a pass of the window being drawn: a ranked layer's own
/// sublayers (a scene, a canvas overlay inside a lying-over panel) are
/// made sublayers of its base too, since egui nests one level only; and
/// the layers someone else made sublayers of a base (egui_stack's
/// lying-over zones) are noted at its rank.
pub fn end_pass(ctx: &Context) {
    for r in Rank::ALL {
        let b = base(ctx, r);
        let children: Vec<LayerId> = ctx.memory(|m| m.areas().child_layers(b).collect());
        for child in children {
            if child == anchor(r) {
                continue;
            }
            note(ctx, child, r);
            let nested: Vec<LayerId> = ctx.memory(|m| m.areas().child_layers(child).collect());
            for n in nested {
                if n.order == Order::Foreground {
                    ctx.set_sublayer(b, n);
                    note(ctx, n, r);
                }
            }
        }
    }
}

/// Runs [`begin_pass`] and [`end_pass`] in every pass of every window.
#[derive(Debug)]
struct LayersPlugin;

impl egui::plugin::Plugin for LayersPlugin {
    fn debug_name(&self) -> &'static str {
        "egui_pop_core::layers"
    }

    fn on_begin_pass(&mut self, ui: &mut Ui) {
        begin_pass(ui.ctx());
    }

    fn on_end_pass(&mut self, ui: &mut Ui) {
        end_pass(ui.ctx());
    }
}

#[derive(Debug, Default)]
struct Installed(bool);

/// Installs the ranks on a context (once; later calls do nothing), and
/// starts the current pass: call it before the pass shows any
/// foreground layer.
pub fn install(ctx: &Context) {
    let st = crate::state::shared::<Installed>(ctx, "core.layers.installed");
    let mut s = lock(&st);
    if !s.0 {
        s.0 = true;
        drop(s);
        ctx.add_plugin(LayersPlugin);
    }
    begin_pass(ctx);
}
