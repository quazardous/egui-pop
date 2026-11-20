//! A frameless text button sized for a bar (`widgets::bar_text_button`):
//! a button for assistive technology named by its text, in the small text
//! style, no box at rest, the hover look under the pointer, the focus
//! ring with the keyboard, and a click reported.

use egui::accesskit::Role;
use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_core::theme::{self, ThemeSpec};
use egui_pop_core::widgets;

#[derive(Default)]
struct State {
    clicks: u32,
    height: f32,
    body: f32,
}

fn harness() -> Harness<'static, State> {
    let mut h = Harness::builder()
        .with_size(vec2(320.0, 80.0))
        .build_ui_state(
            |ui, s: &mut State| {
                ui.horizontal(|ui| {
                    let r = widgets::bar_text_button(ui, "Ana's profile");
                    if r.clicked() {
                        s.clicks += 1;
                    }
                    s.height = r.rect.height();
                    s.body = ui.add(egui::Button::new("Body button")).rect.height();
                });
            },
            State::default(),
        );
    theme::install(&h.ctx, &ThemeSpec::new("pop-dark"));
    h.run();
    h
}

#[test]
fn a_bar_text_button_is_a_small_named_button_that_clicks() {
    let mut h = harness();
    assert_eq!(
        h.get_by_label("Ana's profile").accesskit_node().role(),
        Role::Button
    );
    h.get_by_label("Ana's profile").focus();
    h.run();
    assert!(
        h.get_by_label("Ana's profile").is_focused(),
        "the keyboard reaches it"
    );
    h.get_by_label("Ana's profile").click();
    h.run();
    assert_eq!(h.state().clicks, 1);
    assert!(
        h.state().height <= h.state().body,
        "no taller than a button of body text: {} > {}",
        h.state().height,
        h.state().body
    );
}

#[test]
fn a_bar_text_button_draws_no_box_at_rest() {
    let mut h = harness();
    let rect = h.get_by_label("Ana's profile").rect();
    h.run();
    fn rects(shape: &egui::Shape, into: &mut Vec<(egui::Rect, egui::Color32)>) {
        match shape {
            egui::Shape::Rect(r) => into.push((r.rect, r.fill)),
            egui::Shape::Vec(v) => v.iter().for_each(|s| rects(s, into)),
            _ => {}
        }
    }
    let mut all = Vec::new();
    h.output()
        .shapes
        .iter()
        .for_each(|c| rects(&c.shape, &mut all));
    let boxed = all.iter().any(|(r, f)| {
        f.a() > 0 && r.contains_rect(rect.shrink(1.0)) && rect.expand(4.0).contains_rect(*r)
    });
    assert!(!boxed, "a box under the button at rest");
}
