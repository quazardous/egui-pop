//! The keyboard walk on itself: real key presses, what holds the focus.

use egui_kittest::Harness;
use egui_pop_core::theme::ThemeSpec;
use egui_pop_test::themed;
use egui_pop_test::walk::{self, ESCAPE, End, Press, SHIFT_TAB, TAB};

fn three<'a>() -> Harness<'a> {
    let mut h = themed(&ThemeSpec::default(), egui::vec2(200.0, 150.0), |ui| {
        let _ = ui.button("One");
        let _ = ui.button("Two");
        let _ = ui.button("Three");
    });
    h.run();
    h
}

#[test]
fn a_walk_goes_round_and_stops_where_it_started() {
    let mut h = three();
    let forth = walk::walk(&mut h, TAB, 10);
    assert_eq!(forth.from, None);
    assert_eq!(forth.names(), ["One", "Two", "Three"]);
    assert_eq!(forth.end, End::Looped { at: 0 });
    assert!(forth.stops.iter().all(|s| s.visible), "{forth}");
    // It ended on "One" again, so the way back starts with "Three".
    let back = walk::walk(&mut h, SHIFT_TAB, 10);
    assert_eq!(back.from.as_ref().map(|s| s.name.as_str()), Some("One"));
    assert_eq!(back.names(), ["Three", "Two", "One"]);
}

#[test]
fn a_walk_says_when_the_focus_is_lost_or_the_limit_comes_first() {
    let mut h = three();
    let walked = walk::walk(&mut h, TAB, 2);
    assert_eq!(walked.names(), ["One", "Two"]);
    assert_eq!(walked.end, End::Limit);
    let walked = walk::walk(&mut h, ESCAPE, 5);
    assert_eq!(walked.names(), Vec::<&str>::new());
    assert_eq!(walked.end, End::Lost);
    assert_eq!(walk::focused(&h), None);
}

#[test]
fn a_key_that_does_not_move_the_focus_ends_the_walk_at_once() {
    let mut h = three();
    let stop = walk::press(&mut h, TAB).unwrap();
    assert_eq!(stop.name, "One");
    let walked = walk::walk(&mut h, Press::key(egui::Key::F6), 5);
    assert_eq!(walked.names(), ["One"]);
    assert_eq!(walked.end, End::Looped { at: 0 });
}

#[test]
fn a_stop_scrolled_out_of_its_area_is_not_visible() {
    let mut h = themed(&ThemeSpec::default(), egui::vec2(200.0, 100.0), |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            for i in 0..20 {
                let _ = ui.button(format!("Row {i}"));
            }
        });
    });
    h.run();
    let walked = walk::walk(&mut h, TAB, 30);
    assert_eq!(walked.stops.len(), 20);
    assert!(walked.stops[0].visible);
    assert!(!walked.stops[19].visible, "{walked}");
}

#[test]
fn a_stop_labelled_by_a_text_takes_its_name_and_reach_finds_it() {
    let mut on = false;
    let mut h = themed(&ThemeSpec::default(), egui::vec2(200.0, 100.0), |ui| {
        let _ = ui.button("Before");
        let label = ui.label("Grid");
        let _ = ui.checkbox(&mut on, "").labelled_by(label.id);
    });
    h.run();
    let stop = walk::reach(&mut h, TAB, "Grid", 5).expect("Tab gets there");
    assert_eq!(stop.role, egui::accesskit::Role::CheckBox);
    assert_eq!(walk::reach(&mut h, SHIFT_TAB, "Nothing like it", 5), None);
}
