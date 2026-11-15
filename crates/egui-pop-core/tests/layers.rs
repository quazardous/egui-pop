//! Layer ranks (docs/overlays.md, "Layer ranks"; ADR 0026): the order
//! between kinds of foreground layers holds whatever was raised last,
//! a rank is remembered for one pass, a layer opened from above its rank
//! stays above its opener, and what cannot be ranked is left alone.

use egui::{Area, Id, LayerId, Order, Pos2, RawInput, pos2, vec2};
use egui_kittest::Harness;
use egui_pop_core::layers::{self, Rank};

/// Three overlapping squares: one at `Over`, one at `Overlay`, one
/// unranked (a menu, a dialog).
#[derive(Default)]
struct Stack {
    unranked: bool,
    raise: Option<LayerId>,
}

const SIDE: f32 = 160.0;
/// Inside `Over` and `Overlay`, outside the unranked square.
const OVER_AND_OVERLAY: Pos2 = pos2(80.0, 80.0);
/// Inside all three.
const ALL_THREE: Pos2 = pos2(150.0, 150.0);

fn layer(name: &str) -> LayerId {
    LayerId::new(Order::Foreground, Id::unique(name))
}

fn square(name: &str, at: Pos2) -> Area {
    Area::new(Id::unique(name)).fixed_pos(at)
}

fn fill(ui: &mut egui::Ui) {
    ui.set_min_size(vec2(SIDE, SIDE));
}

fn harness() -> Harness<'static, Stack> {
    Harness::builder()
        .with_size(vec2(400.0, 400.0))
        .with_context_setup(|ctx| layers::install(ctx))
        .build_ui_state(
            |ui, s: &mut Stack| {
                let ctx = ui.ctx().clone();
                layers::area(&ctx, square("over", pos2(20.0, 20.0)), Rank::Over, fill);
                layers::area(
                    &ctx,
                    square("overlay", pos2(60.0, 60.0)),
                    Rank::Overlay,
                    fill,
                );
                if s.unranked {
                    square("unranked", pos2(100.0, 100.0))
                        .order(Order::Foreground)
                        .show(&ctx, fill);
                }
                if let Some(l) = s.raise.take() {
                    // What a click on the layer does in egui.
                    ctx.move_to_top(l);
                }
            },
            Stack::default(),
        )
}

fn top_at(h: &Harness<'static, Stack>, at: Pos2) -> Option<LayerId> {
    h.ctx.layer_id_at(at)
}

#[test]
fn an_overlay_stays_above_a_layer_lying_over_whatever_was_raised_last() {
    let mut h = harness();
    h.run();
    assert_eq!(top_at(&h, OVER_AND_OVERLAY), Some(layer("overlay")));
    h.state_mut().raise = Some(layer("over"));
    h.run();
    assert_eq!(
        top_at(&h, OVER_AND_OVERLAY),
        Some(layer("overlay")),
        "raising the lying-over layer does not put it over the overlay"
    );
    assert_eq!(layers::rank_of(&h.ctx, layer("over")), Some(Rank::Over));
    assert_eq!(
        layers::rank_of(&h.ctx, layer("overlay")),
        Some(Rank::Overlay)
    );
}

#[test]
fn an_unranked_foreground_layer_stays_above_every_ranked_one() {
    let mut h = harness();
    h.state_mut().unranked = true;
    h.run();
    assert_eq!(top_at(&h, ALL_THREE), Some(layer("unranked")));
    assert_eq!(layers::rank_of(&h.ctx, layer("unranked")), None);
    for raised in ["overlay", "over"] {
        h.state_mut().raise = Some(layer(raised));
        h.run();
        assert_eq!(
            top_at(&h, ALL_THREE),
            Some(layer("unranked")),
            "{raised} raised stays under the unranked layer"
        );
    }
}

#[test]
fn a_rank_is_remembered_for_the_pass_after_and_no_longer() {
    let ctx = egui::Context::default();
    let card = layer("card");
    let pass = |f: &mut dyn FnMut(&egui::Context)| {
        let _ = ctx.run_ui(RawInput::default(), |ui| f(ui.ctx()));
    };
    layers::install(&ctx);
    pass(&mut |ctx| {
        layers::rank(ctx, card, Rank::Overlay);
        assert_eq!(layers::rank_of(ctx, card), Some(Rank::Overlay));
    });
    let mut seen = Vec::new();
    for _ in 0..2 {
        pass(&mut |ctx| seen.push(layers::rank_of(ctx, card)));
    }
    assert_eq!(seen, [Some(Rank::Overlay), None]);
    // A base is at its rank, always.
    for r in Rank::ALL {
        assert_eq!(layers::rank_of(&ctx, layers::base(&ctx, r)), Some(r));
    }
}

#[test]
fn a_layer_made_a_sublayer_of_a_base_by_its_owner_is_noted_at_that_rank() {
    let ctx = egui::Context::default();
    layers::install(&ctx);
    let zone = layer("lying-over zone");
    let inner = layer("zone's own popup");
    let _ = ctx.run_ui(RawInput::default(), |ui| {
        let ctx = ui.ctx();
        // egui_stack's lying-over zones do this, not `rank`.
        ctx.set_sublayer(layers::base(ctx, Rank::Over), zone);
        ctx.set_sublayer(zone, inner);
    });
    let _ = ctx.run_ui(RawInput::default(), |ui| {
        assert_eq!(layers::rank_of(ui.ctx(), zone), Some(Rank::Over));
        assert_eq!(
            layers::rank_of(ui.ctx(), inner),
            Some(Rank::Over),
            "a ranked layer's own sublayer follows it to its base"
        );
    });
}

#[test]
fn a_layer_opened_from_above_its_rank_stays_unranked_above_its_opener() {
    let ctx = egui::Context::default();
    layers::install(&ctx);
    let popover = layer("popover");
    let over = layer("lying-over zone");
    let overlay = layer("a card");
    let dialog = layer("a dialog");
    let _ = ctx.run_ui(RawInput::default(), |ui| {
        let ctx = ui.ctx();
        layers::rank(ctx, over, Rank::Over);
        layers::rank(ctx, overlay, Rank::Overlay);
        let from = |opener: LayerId, rank: Rank| layers::rank_from(ctx, popover, rank, opener);
        assert!(from(LayerId::background(), Rank::Overlay), "from a panel");
        assert!(
            from(
                LayerId::new(Order::Middle, Id::unique("window")),
                Rank::Overlay
            ),
            "from a window"
        );
        assert!(from(over, Rank::Overlay), "from a zone lying over");
        assert!(from(overlay, Rank::Overlay), "from another overlay");
        assert!(!from(overlay, Rank::Over), "from above that rank");
        assert!(!from(dialog, Rank::Overlay), "from an unranked dialog");
        assert!(
            !from(
                LayerId::new(Order::Tooltip, Id::unique("tip")),
                Rank::Overlay
            ),
            "from a tooltip"
        );
    });
}

#[test]
fn a_layer_outside_the_foreground_is_left_where_it_is() {
    let ctx = egui::Context::default();
    layers::install(&ctx);
    let window = LayerId::new(Order::Middle, Id::unique("a window"));
    let _ = ctx.run_ui(RawInput::default(), |ui| {
        // No panic (egui asserts sublayers share an order), no rank.
        layers::rank(ui.ctx(), window, Rank::Overlay);
        assert_eq!(layers::rank_of(ui.ctx(), window), None);
        assert_eq!(layers::rank_of(ui.ctx(), LayerId::background()), None);
    });
}

#[test]
fn installing_twice_changes_nothing() {
    let mut h = harness();
    layers::install(&h.ctx);
    layers::install(&h.ctx);
    h.state_mut().raise = Some(layer("over"));
    h.run();
    assert_eq!(top_at(&h, OVER_AND_OVERLAY), Some(layer("overlay")));
}
