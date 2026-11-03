//! Arrows stay in their region: real key presses over two regions side by
//! side, with the focus plugin.

use egui::{Key, Ui, vec2};
use egui_kittest::Harness;
use egui_pop_core::focus::{self, Region};
use egui_pop_core::theme::ThemeSpec;
use egui_pop_test::themed;
use egui_pop_test::walk::{self, Press, TAB};

fn page<'a>(regions: bool) -> Harness<'a> {
    let mut h = themed(
        &ThemeSpec::default(),
        vec2(400.0, 200.0),
        move |ui: &mut Ui| {
            ui.horizontal_top(|ui| {
                let column = |ui: &mut Ui, names: [&str; 2]| {
                    ui.vertical(|ui| {
                        for n in names {
                            let _ = ui.button(n);
                        }
                    });
                };
                if regions {
                    focus::region(ui, &Region::new("left", 0), |ui| column(ui, ["A1", "A2"]));
                    focus::region(ui, &Region::new("right", 1), |ui| column(ui, ["B1", "B2"]));
                } else {
                    column(ui, ["A1", "A2"]);
                    column(ui, ["B1", "B2"]);
                }
            });
        },
    );
    focus::install(&h.ctx);
    let _ = h.run_ok();
    h
}

fn name(stop: Option<walk::Stop>) -> String {
    stop.map(|s| s.name).unwrap_or_default()
}

#[test]
fn an_arrow_does_not_take_the_focus_out_of_its_region() {
    let mut h = page(true);
    assert_eq!(name(walk::press(&mut h, TAB)), "A1");
    // Inside the region, the arrows move as egui has it.
    assert_eq!(name(walk::press(&mut h, Press::key(Key::ArrowDown))), "A2");
    // Out of it, the focus stays.
    assert_eq!(name(walk::press(&mut h, Press::key(Key::ArrowRight))), "A2");
    // Tab crosses.
    assert_eq!(name(walk::press(&mut h, TAB)), "B1");
    assert_eq!(name(walk::press(&mut h, Press::key(Key::ArrowLeft))), "B1");
}

#[test]
fn without_regions_an_arrow_goes_where_egui_sends_it() {
    let mut h = page(false);
    assert_eq!(name(walk::press(&mut h, TAB)), "A1");
    assert_eq!(name(walk::press(&mut h, Press::key(Key::ArrowRight))), "B1");
}
