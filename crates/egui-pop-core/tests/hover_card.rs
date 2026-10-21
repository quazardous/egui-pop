//! Hover cards, headless: opened by the pointer after the delay or by
//! the keyboard, their links clicked, closed by Escape; the card is the
//! widget's accessible description.

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_core::hover_card::{self, Card, HoverCard};
use egui_pop_core::theme::{self, ThemeSpec};
use egui_pop_model::ids::CommandId;
use serde_json::json;

const OPEN: CommandId = CommandId::from_static("vault.open");

#[derive(Default)]
struct State {
    images: u32,
    ran: Vec<CommandId>,
}

fn card_id() -> egui::Id {
    HoverCard::new("vault").id()
}

fn harness() -> Harness<'static, State> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(500.0, 300.0))
        .build_ui_state(
            |ui, s: &mut State| {
                // No delay: the pointer opens it at once.
                ui.style_mut().interaction.tooltip_delay = 0.0;
                let r = ui.button("Vault");
                let card = Card::new()
                    .text("**Vault** is open")
                    .row("Images", s.images.to_string())
                    .link("Open the vault", OPEN, json!({ "at": "start" }));
                if let Some(link) = HoverCard::new("vault").show(ui, &r, &card) {
                    s.ran.push(link.command);
                }
                ui.add_space(120.0);
                let _ = ui.button("Elsewhere");
            },
            State {
                images: 3,
                ..State::default()
            },
        );
    theme::install(&h.ctx, &ThemeSpec::new("pop-dark"));
    h.run();
    h
}

#[test]
fn the_card_is_the_widgets_description_and_closed_until_hovered() {
    let h = harness();
    let vault = h.get_by_label("Vault");
    assert_eq!(
        vault.accesskit_node().description().as_deref(),
        Some("Vault is open\nImages: 3")
    );
    assert!(h.query_by_label("Open the vault").is_none());
    assert!(!hover_card::is_open(&h.ctx, card_id()));
}

#[test]
fn hovering_opens_it_with_live_values_and_a_link_runs_its_command() {
    let mut h = harness();
    h.get_by_label("Vault").hover();
    h.run();
    h.run();
    assert!(hover_card::is_open(&h.ctx, card_id()));
    assert!(h.query_by_label_contains("Images").is_some());
    // Live: the card is built each pass from the state.
    h.state_mut().images = 4;
    h.run();
    assert!(h.query_by_label("4").is_some());
    h.get_by_label("Open the vault").click();
    h.run();
    assert_eq!(h.state().ran, vec![OPEN]);
    assert!(!hover_card::is_open(&h.ctx, card_id()));
}

#[test]
fn the_keyboard_opens_it_focuses_its_first_link_and_escape_closes_it() {
    let mut h = harness();
    hover_card::open(&h.ctx, card_id());
    h.run();
    h.run();
    let link = h.get_by_label("Open the vault");
    assert!(link.is_focused());
    // Opened by key, it stays while the pointer is elsewhere.
    h.get_by_label("Elsewhere").hover();
    h.run();
    assert!(hover_card::is_open(&h.ctx, card_id()));
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(!hover_card::is_open(&h.ctx, card_id()));
    assert!(h.query_by_label("Open the vault").is_none());
}
