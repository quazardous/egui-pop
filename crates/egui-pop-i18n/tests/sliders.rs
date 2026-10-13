//! A slider's value in the interface's language: the number and the
//! space before its unit are the locale's.

use std::sync::Arc;

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_core::slider::{RangeSlider, Slider};
use egui_pop_i18n::{I18n, Provider};

struct Values {
    exposure: f64,
    opacity: f64,
    low: f64,
    high: f64,
}

fn shown(locale: &str) -> Vec<String> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(420.0, 120.0))
        .build_ui_state(
            |ui, v: &mut Values| {
                ui.add(
                    Slider::new(&mut v.exposure, "Exposure", -3.0..=4.0)
                        .step(0.01)
                        .unit("EV"),
                );
                ui.add(Slider::new(&mut v.opacity, "Opacity", 0.0..=100.0).unit("%"));
                ui.add(
                    RangeSlider::new(&mut v.low, &mut v.high, "ISO", 100.0..=6400.0).unit("ISO"),
                );
            },
            Values {
                exposure: 1.5,
                opacity: 50.0,
                low: 200.0,
                high: 1600.0,
            },
        );
    egui_pop_core::strings::install(&h.ctx, Arc::new(Provider::new(I18n::new(locale))));
    h.run();
    ["Exposure", "Opacity", "ISO, minimum", "ISO, maximum"]
        .iter()
        .map(|l| {
            h.get_by_label(l)
                .accesskit_node()
                .value()
                .unwrap_or_default()
        })
        .collect()
}

#[test]
fn french_puts_a_narrow_no_break_space_before_the_unit() {
    assert_eq!(
        shown("fr"),
        [
            "1,50\u{202f}EV",
            "50\u{202f}%",
            "200\u{202f}ISO",
            "1\u{202f}600\u{202f}ISO"
        ]
    );
}

#[test]
fn english_keeps_a_plain_space_before_the_unit() {
    assert_eq!(shown("en"), ["1.50 EV", "50 %", "200 ISO", "1,600 ISO"]);
}
