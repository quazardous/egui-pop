//! Inputs keep to the width they are given: a slider's value text and a
//! number input's field stay inside a narrow panel, the slider's bar
//! shrinking to leave its value room (snapshots at two widths).

use std::sync::{Arc, Mutex};

use egui::{Rect, Stroke, UiBuilder, vec2};
use egui_kittest::Harness;
use egui_pop::core::number::NumberInput;
use egui_pop::core::slider::Slider;
use egui_pop::theme::{self, ThemeSpec};
use egui_pop_test::SharedGpu as _;

/// Where each input landed in the last frame.
#[derive(Clone, Copy, Debug, Default)]
struct Drawn {
    panel: Option<Rect>,
    /// A slider in a row as wide as the panel.
    slider: Option<Rect>,
    /// A slider in a row whose room runs past the panel's edge.
    cut: Option<Rect>,
    /// A number input's field, with no range.
    field: Option<Rect>,
}

const ROW: f32 = 28.0;

fn panel(width: f32) -> (Harness<'static>, Arc<Mutex<Drawn>>) {
    egui_pop::api::files::refuse_system_chooser();
    let drawn = Arc::new(Mutex::new(Drawn::default()));
    let out = drawn.clone();
    let mut threshold = 0.8;
    let mut also = 0.8;
    let mut value = 1_250.0;
    let mut h = Harness::builder()
        .with_size(vec2(width + 24.0, 4.0 * ROW + 24.0))
        .shared_wgpu()
        .build_ui(move |ui| {
            let panel = Rect::from_min_size(ui.max_rect().min, vec2(width, 3.0 * ROW + 8.0));
            ui.painter().rect_stroke(
                panel,
                0.0,
                Stroke::new(1.0, ui.visuals().weak_text_color()),
                egui::StrokeKind::Outside,
            );
            let mut d = Drawn {
                panel: Some(panel),
                ..Drawn::default()
            };
            let row = |i: f32, room: f32| {
                Rect::from_min_size(panel.min + vec2(0.0, 4.0 + i * ROW), vec2(room, ROW))
            };
            // A button and a slider sharing a row, as a panel's threshold.
            ui.scope_builder(UiBuilder::new().max_rect(row(0.0, width)), |ui| {
                ui.set_clip_rect(panel);
                ui.horizontal(|ui| {
                    let _ = ui.button("Accept above");
                    let r = Slider::new(&mut threshold, "Threshold", 0.0..=1.0)
                        .step(0.05)
                        .display_scale(100.0)
                        .precision(0)
                        .unit("%")
                        .show(ui)
                        .response;
                    d.slider = Some(r.rect);
                });
            });
            // The same row given more room than the panel shows: the bar
            // ends at the panel's edge, with its value inside.
            ui.scope_builder(UiBuilder::new().max_rect(row(1.0, width + 60.0)), |ui| {
                ui.set_clip_rect(panel);
                ui.horizontal(|ui| {
                    let _ = ui.button("Accept above");
                    let r = Slider::new(&mut also, "Threshold, cut", 0.0..=1.0)
                        .step(0.05)
                        .display_scale(100.0)
                        .precision(0)
                        .unit("%")
                        .show(ui)
                        .response;
                    d.cut = Some(r.rect);
                });
            });
            // A number with no range: its field is sized to the value.
            ui.scope_builder(UiBuilder::new().max_rect(row(2.0, width)), |ui| {
                ui.set_clip_rect(panel);
                ui.horizontal(|ui| {
                    ui.label("Value");
                    let r = NumberInput::new(&mut value, "Value", f64::MIN..=f64::MAX)
                        .unit("EV")
                        .show(ui);
                    d.field = Some(r.rect);
                });
            });
            *out.lock().unwrap() = d;
        });
    theme::install(&h.ctx, &ThemeSpec::default());
    egui_pop::core::motion::freeze(&h.ctx, 0.0);
    h.run();
    (h, drawn)
}

fn check(width: f32, name: &str) {
    let (mut h, drawn) = panel(width);
    let d = *drawn.lock().unwrap();
    let panel = d.panel.expect("drawn");
    for (what, r) in [
        ("slider", d.slider),
        ("cut slider", d.cut),
        ("field", d.field),
    ] {
        let r = r.expect("drawn");
        assert!(
            r.right() <= panel.right() + 0.5,
            "{name}: the {what} ends at {}, past the panel's edge at {}",
            r.right(),
            panel.right()
        );
    }
    // A number with no range takes the value's width, not the widest field.
    let field = d.field.expect("drawn");
    assert!(field.width() < 100.0, "{name}: field {field:?}");
    h.snapshot(name);
}

#[test]
fn a_slider_and_a_number_keep_to_a_narrow_panel() {
    check(200.0, "input-widths-narrow-pop-dark");
}

#[test]
fn a_slider_and_a_number_keep_to_a_wide_panel() {
    check(360.0, "input-widths-wide-pop-dark");
}
