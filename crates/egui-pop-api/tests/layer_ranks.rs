//! Layer ranks (ADR 0026): a panel lying over the centre stays under an
//! application's overlay, whatever was clicked last, and a popup opened
//! from that panel stays above it.

use egui::{Id, LayerId, Order, Pos2, Rect};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_core::layers::{self, Rank};
use egui_pop_model::ids::{PanelId, PluginId};
use egui_pop_model::panels::{Area, PanelSpec};

const TREE: PanelId = PanelId::from_static("demo.tree");

struct Demo;

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.panel(
            PanelSpec::new(TREE, "Tree", Area::StartSide).default_size(260.0),
            |ui, _| {
                let _ = ui.button("tree button");
                let mut pick = 0;
                egui::ComboBox::from_label("pick")
                    .selected_text("first")
                    .show_ui(ui, |ui| {
                        for (i, name) in ["first", "second", "third"].into_iter().enumerate() {
                            ui.selectable_value(&mut pick, i, name);
                        }
                    });
                ui.label("tree body");
            },
        )?;
        Ok(())
    }
}

/// How the application shows its own card.
#[derive(Clone, Copy)]
enum Card {
    /// A plain `egui::Area` in the foreground, as tvty's "Did you know?"
    /// card is.
    Plain,
    /// Through `layers::area` at `Rank::Overlay`.
    Ranked,
}

/// Where the card is: over the start side bar, which lies over the
/// centre.
const CARD_AT: Pos2 = Pos2::new(60.0, 300.0);

fn card_id() -> Id {
    Id::unique("demo.card")
}

fn card_layer() -> LayerId {
    LayerId::new(Order::Foreground, card_id())
}

fn harness(card: Card) -> Harness<'static, App> {
    let app = App::builder()
        .plugin(Demo)
        .over(&[Area::StartSide])
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 600.0))
        .build_ui_state(
            move |ui, app: &mut App| {
                app.show(ui);
                let area = egui::Area::new(card_id())
                    .order(Order::Foreground)
                    .fixed_pos(CARD_AT)
                    .interactable(true);
                let add = |ui: &mut egui::Ui| {
                    ui.set_min_size(egui::vec2(160.0, 60.0));
                    ui.label("Did you know?");
                };
                match card {
                    Card::Plain => {
                        area.show(ui.ctx(), add);
                    }
                    Card::Ranked => {
                        layers::area(ui.ctx(), area, Rank::Overlay, add);
                    }
                }
            },
            app,
        );
    h.run();
    h
}

fn card_rect(h: &Harness<'_, App>) -> Rect {
    h.ctx
        .memory(|m| m.area_rect(card_id()))
        .expect("the card is shown")
}

/// The card's layer is the top one at its centre, the panel lying
/// under it.
fn assert_card_on_top(h: &Harness<'_, App>, when: &str) {
    let card = card_rect(h);
    let side = h.state().handle().area_rect(Area::StartSide).unwrap();
    assert!(
        side.contains_rect(card),
        "the card {card:?} lies over the start side bar {side:?}"
    );
    assert_eq!(
        h.ctx.layer_id_at(card.center()),
        Some(card_layer()),
        "{when}: the card is above the panel lying over the centre"
    );
}

fn card_stays_on_top(card: Card) {
    let mut h = harness(card);
    assert_eq!(
        h.state().handle().layout().over,
        [Area::StartSide],
        "the start side bar lies over the centre"
    );
    assert_card_on_top(&h, "first pass");
    h.run();
    assert_card_on_top(&h, "second pass");
    // A click on the panel lying over: egui would raise its layer.
    h.get_by_label("tree button").click();
    h.run();
    h.run();
    assert_card_on_top(&h, "after a click on the panel");
    // Pressed in the panel, beside the card's rectangle, and dragged.
    let body = h.get_by_label("tree body").rect().center();
    h.drag_at(body);
    h.run();
    h.drop_at(body + egui::vec2(4.0, 4.0));
    h.run();
    assert_card_on_top(&h, "after a press in the panel");
}

#[test]
fn an_application_area_in_the_foreground_stays_above_a_panel_lying_over() {
    card_stays_on_top(Card::Plain);
}

#[test]
fn an_application_overlay_ranked_stays_above_a_panel_lying_over() {
    card_stays_on_top(Card::Ranked);
}

#[test]
fn a_popup_opened_from_a_panel_lying_over_stays_above_it() {
    let mut h = harness(Card::Ranked);
    let panel = h
        .ctx
        .layer_id_at(h.get_by_label("tree button").rect().center())
        .expect("the panel's layer");
    assert_eq!(panel.order, Order::Foreground, "the panel lies over");
    h.get_by_label("pick").click();
    h.run();
    h.run();
    let item = h.get_by_label("third").rect();
    let side = h.state().handle().area_rect(Area::StartSide).unwrap();
    assert!(side.contains(item.center()), "the list is over the panel");
    for pass in 0..3 {
        let top = h.ctx.layer_id_at(item.center()).expect("a layer");
        assert_ne!(top, panel, "pass {pass}: the list is above the panel");
        assert_ne!(top, card_layer(), "pass {pass}: the list is above the card");
        assert_eq!(layers::rank_of(&h.ctx, top), None, "a popup is unranked");
        h.run();
    }
    // The panel lying over is ranked, the card too.
    assert_eq!(layers::rank_of(&h.ctx, panel), Some(Rank::Over));
    assert_eq!(layers::rank_of(&h.ctx, card_layer()), Some(Rank::Overlay));
}
