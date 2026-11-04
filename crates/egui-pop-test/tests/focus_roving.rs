//! A roving group is one Tab stop with arrows inside: real key presses over
//! a row of buttons between two others, with the focus plugin.

use egui::{Key, Ui, vec2};
use egui_kittest::Harness;
use egui_pop_core::focus;
use egui_pop_core::tab::{self, Axis};
use egui_pop_core::theme::ThemeSpec;
use egui_pop_test::themed;
use egui_pop_test::walk::{self, ESCAPE, Press, SHIFT_TAB, TAB};

fn page<'a>(axis: Axis, disabled: Option<&'static str>) -> Harness<'a> {
    let mut h = themed(
        &ThemeSpec::default(),
        vec2(400.0, 300.0),
        move |ui: &mut Ui| {
            let _ = ui.button("Before");
            tab::roving(ui, "tools", axis, |ui| {
                let tools = |ui: &mut Ui| {
                    for n in ["A", "B", "C"] {
                        let _ = ui.add_enabled(disabled != Some(n), egui::Button::new(n));
                    }
                };
                match axis {
                    Axis::Horizontal => ui.horizontal(tools),
                    Axis::Vertical => ui.vertical(tools),
                };
            });
            let _ = ui.button("After");
        },
    );
    focus::install(&h.ctx);
    let _ = h.run_ok();
    h
}

fn name(stop: Option<walk::Stop>) -> String {
    stop.map(|s| s.name).unwrap_or_default()
}

fn press(h: &mut Harness<'_>, p: Press) -> String {
    name(walk::press(h, p))
}

#[test]
fn a_roving_group_is_one_stop_and_tab_comes_back_to_the_last() {
    let mut h = page(Axis::Horizontal, None);
    assert_eq!(press(&mut h, TAB), "Before");
    assert_eq!(press(&mut h, TAB), "A");
    assert_eq!(press(&mut h, Press::key(Key::ArrowRight)), "B");
    // Out past its end, from the middle.
    assert_eq!(press(&mut h, TAB), "After");
    // Back in, on the last focused.
    assert_eq!(press(&mut h, SHIFT_TAB), "B");
    assert_eq!(press(&mut h, SHIFT_TAB), "Before");
    assert_eq!(press(&mut h, TAB), "B");
}

#[test]
fn the_arrows_go_round_and_home_and_end_go_to_the_ends() {
    let mut h = page(Axis::Horizontal, None);
    walk::reach(&mut h, TAB, "A", 4).expect("the group");
    assert_eq!(press(&mut h, Press::key(Key::ArrowLeft)), "C");
    assert_eq!(press(&mut h, Press::key(Key::ArrowRight)), "A");
    assert_eq!(press(&mut h, Press::key(Key::End)), "C");
    assert_eq!(press(&mut h, Press::key(Key::Home)), "A");
    // The arrows across a row are not the group's.
    assert_eq!(press(&mut h, Press::key(Key::ArrowDown)), "After");
}

#[test]
fn a_column_takes_up_and_down() {
    let mut h = page(Axis::Vertical, None);
    walk::reach(&mut h, TAB, "A", 4).expect("the group");
    assert_eq!(press(&mut h, Press::key(Key::ArrowDown)), "B");
    assert_eq!(press(&mut h, Press::key(Key::ArrowUp)), "A");
    assert_eq!(press(&mut h, Press::key(Key::ArrowUp)), "C");
}

#[test]
fn a_disabled_widget_is_passed_over() {
    let mut h = page(Axis::Horizontal, Some("B"));
    walk::reach(&mut h, TAB, "A", 4).expect("the group");
    assert_eq!(press(&mut h, Press::key(Key::ArrowRight)), "C");
}

#[test]
fn shift_tab_comes_in_on_the_first_the_first_time() {
    let mut h = page(Axis::Horizontal, None);
    walk::reach(&mut h, TAB, "After", 6).expect("past the group");
    assert_eq!(press(&mut h, SHIFT_TAB), "A");
}

#[test]
fn after_escape_tab_goes_on_past_the_group() {
    let mut h = page(Axis::Horizontal, None);
    walk::reach(&mut h, TAB, "A", 4).expect("the group");
    assert_eq!(press(&mut h, Press::key(Key::ArrowRight)), "B");
    assert_eq!(press(&mut h, ESCAPE), "");
    assert_eq!(press(&mut h, TAB), "After");
}

#[test]
fn the_walk_sees_one_stop() {
    let mut h = page(Axis::Horizontal, None);
    let walked = walk::walk(&mut h, TAB, 10);
    assert_eq!(walked.names(), ["Before", "A", "After"], "{walked}");
}
