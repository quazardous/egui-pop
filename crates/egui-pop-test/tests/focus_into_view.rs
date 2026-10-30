//! Focus scrolls into view: real key presses over plain egui scroll areas,
//! with the focus plugin and nothing else from egui-pop in the page.

use std::sync::{Arc, Mutex};

use egui::{Id, Modifiers, PointerButton, Pos2, Rect, ScrollArea, Sense, Ui, vec2};
use egui_kittest::Harness;
use egui_pop_core::focus;
use egui_pop_core::theme::ThemeSpec;
use egui_pop_test::themed;
use egui_pop_test::walk::{self, Press, SHIFT_TAB, Stop, TAB};

const SIZE: egui::Vec2 = vec2(300.0, 200.0);

fn page<'a>(ui: impl FnMut(&mut Ui) + 'a) -> Harness<'a> {
    let mut h = themed(&ThemeSpec::default(), SIZE, ui);
    focus::install(&h.ctx);
    let _ = h.run_ok();
    h
}

fn buttons(ui: &mut Ui, name: &str, count: usize) {
    for i in 0..count {
        let _ = ui.button(format!("{name} {i}"));
    }
}

/// All of the widget shows, in every area around it.
fn whole<S>(h: &Harness<'_, S>, stop: &Stop) -> bool {
    let r = h.ctx.read_response(stop.id).expect("it is drawn");
    r.interact_rect.expand(0.5).contains_rect(r.rect)
}

/// Goes round with a key, each stop having to show whole as it is reached:
/// the stops, in order.
#[track_caller]
fn round<S>(h: &mut Harness<'_, S>, key: Press) -> Vec<String> {
    let mut seen: Vec<Stop> = Vec::new();
    for _ in 0..300 {
        let stop = walk::press(h, key).expect("the focus stays");
        if seen.iter().any(|s| s.id == stop.id) {
            return seen.into_iter().map(|s| s.name).collect();
        }
        assert!(whole(h, &stop), "{stop} after {} stops", seen.len());
        seen.push(stop);
    }
    panic!("the walk does not end");
}

#[test]
fn every_stop_of_a_long_page_shows_going_forward_and_back() {
    let mut h = page(|ui| {
        ScrollArea::vertical().show(ui, |ui| buttons(ui, "Row", 40));
    });
    let forth = round(&mut h, TAB);
    assert_eq!(forth.len(), 40);
    assert_eq!(forth[39], "Row 39");
    let back = round(&mut h, SHIFT_TAB);
    assert_eq!(back.len(), 40);
    assert_eq!(back[0], "Row 39");
}

#[test]
fn a_wide_page_scrolls_sideways_too() {
    let mut h = page(|ui| {
        ScrollArea::horizontal().show(ui, |ui| {
            ui.horizontal(|ui| buttons(ui, "Column", 30));
        });
    });
    assert_eq!(round(&mut h, TAB).len(), 30);
    assert_eq!(round(&mut h, SHIFT_TAB).len(), 30);
}

#[test]
fn a_page_scrolling_both_ways_keeps_the_stop_clear_of_its_bars() {
    let mut h = page(|ui| {
        ScrollArea::both().show(ui, |ui| {
            for row in 0..20 {
                ui.horizontal(|ui| buttons(ui, &format!("Cell {row}"), 8));
            }
        });
    });
    assert_eq!(round(&mut h, TAB).len(), 160);
    assert_eq!(round(&mut h, SHIFT_TAB).len(), 160);
}

#[test]
fn nested_areas_all_follow_the_focus() {
    let mut h = page(|ui| {
        ScrollArea::vertical().id_salt("outer").show(ui, |ui| {
            buttons(ui, "Before", 15);
            ScrollArea::vertical()
                .id_salt("middle")
                .max_height(120.0)
                .show(ui, |ui| {
                    buttons(ui, "Middle", 10);
                    ScrollArea::vertical()
                        .id_salt("inner")
                        .max_height(60.0)
                        .show(ui, |ui| buttons(ui, "Inner", 10));
                    buttons(ui, "Last of the middle", 10);
                });
            buttons(ui, "After", 15);
        });
    });
    assert_eq!(round(&mut h, TAB).len(), 60);
    assert_eq!(round(&mut h, SHIFT_TAB).len(), 60);
}

#[test]
fn an_area_between_that_cannot_scroll_does_not_stop_it() {
    let mut h = page(|ui| {
        ScrollArea::vertical().id_salt("page").show(ui, |ui| {
            buttons(ui, "Before", 15);
            ScrollArea::vertical()
                .id_salt("fits")
                .show(ui, |ui| buttons(ui, "Fits", 3));
            ScrollArea::horizontal().id_salt("wide").show(ui, |ui| {
                ui.horizontal(|ui| buttons(ui, "Wide", 12));
            });
            buttons(ui, "After", 15);
        });
    });
    assert_eq!(round(&mut h, TAB).len(), 45);
    assert_eq!(round(&mut h, SHIFT_TAB).len(), 45);
}

#[test]
fn areas_side_by_side_leave_each_other_alone() {
    let offsets = Arc::new(Mutex::new([0.0_f32; 2]));
    let seen = offsets.clone();
    let mut h = page(move |ui| {
        let mut out = seen.lock().unwrap();
        ui.columns(2, |columns| {
            let left = ScrollArea::vertical()
                .id_salt("left")
                .show(&mut columns[0], |ui| buttons(ui, "Left", 30));
            let right = ScrollArea::vertical()
                .id_salt("right")
                .show(&mut columns[1], |ui| buttons(ui, "Right", 30));
            *out = [left.state.offset.y, right.state.offset.y];
        });
    });
    walk::reach(&mut h, TAB, "Left 29", 40).expect("Tab gets there");
    let at_the_left = *offsets.lock().unwrap();
    assert!(at_the_left[0] > 0.0, "{at_the_left:?}");
    assert_eq!(at_the_left[1], 0.0);
    // Back on the first of the right column, the left one stays where it was.
    let stop = walk::press(&mut h, TAB).expect("the focus stays");
    assert_eq!(stop.name, "Right 0");
    assert_eq!(offsets.lock().unwrap()[0], at_the_left[0]);
}

#[test]
fn a_stop_in_view_scrolls_nothing() {
    let offset = Arc::new(Mutex::new(0.0_f32));
    let seen = offset.clone();
    let mut h = page(move |ui| {
        let out = ScrollArea::vertical().show(ui, |ui| buttons(ui, "Row", 40));
        *seen.lock().unwrap() = out.state.offset.y;
    });
    walk::reach(&mut h, TAB, "Row 2", 5).expect("Tab gets there");
    assert_eq!(*offset.lock().unwrap(), 0.0);
}

#[test]
fn it_jumps_without_an_animation() {
    let mut h = page(|ui| {
        ScrollArea::vertical().show(ui, |ui| buttons(ui, "Row", 40));
    });
    walk::reach(&mut h, TAB, "Row 8", 20);
    for _ in 0..4 {
        // A pass reads the key and moves the focus, the next draws it in view.
        h.key_press(egui::Key::Tab);
        h.step();
        h.step();
        let stop = walk::focused(&h).expect("the focus stays");
        assert!(whole(&h, &stop), "{stop}");
    }
}

#[test]
fn the_wheel_can_take_a_focused_widget_away_again() {
    let mut h = page(|ui| {
        ScrollArea::vertical().show(ui, |ui| buttons(ui, "Row", 40));
    });
    let stop = walk::reach(&mut h, TAB, "Row 20", 30).expect("Tab gets there");
    h.event(egui::Event::PointerMoved(stop.rect.center()));
    let _ = h.run_ok();
    for _ in 0..5 {
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: vec2(0.0, 20.0),
            phase: egui::TouchPhase::Move,
            modifiers: Modifiers::NONE,
        });
        let _ = h.run_ok();
    }
    assert_eq!(walk::focused(&h).map(|s| s.id), Some(stop.id));
    assert!(!whole(&h, &stop), "it stays where the wheel left it");
}

/// A click at a point of a widget, by the pointer's own events.
fn click_at<S>(h: &mut Harness<'_, S>, at: Pos2) {
    h.event(egui::Event::PointerMoved(at));
    let _ = h.run_ok();
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        h.step();
    }
    let _ = h.run_ok();
}

#[test]
fn a_click_on_a_half_visible_widget_does_not_scroll() {
    let field = Arc::new(Mutex::new((Id::NULL, Rect::NOTHING)));
    let seen = field.clone();
    let mut text = String::new();
    let mut h = page(move |ui| {
        ScrollArea::vertical().show(ui, |ui| {
            ui.allocate_space(vec2(10.0, SIZE.y - 24.0));
            let r = ui.text_edit_singleline(&mut text);
            *seen.lock().unwrap() = (r.id, r.rect);
            ui.allocate_space(vec2(10.0, 400.0));
        });
    });
    let (id, before) = *field.lock().unwrap();
    assert!(before.max.y > SIZE.y, "half out of view: {before:?}");
    click_at(&mut h, before.left_top() + vec2(10.0, 4.0));
    assert_eq!(walk::focused(&h).map(|s| s.id), Some(id), "the click took");
    assert_eq!(field.lock().unwrap().1, before);
    // The keyboard's way to the same widget does bring it in.
    let _ = walk::press(&mut h, walk::ESCAPE);
    let stop = walk::press(&mut h, TAB).expect("Tab goes back to it");
    assert_eq!(stop.id, id);
    assert!(whole(&h, &stop), "{stop}");
}

#[test]
fn a_widget_longer_than_the_view_stays_where_it_is() {
    let rect = Arc::new(Mutex::new(Rect::NOTHING));
    let seen = rect.clone();
    let mut h = page(move |ui| {
        ScrollArea::vertical().show(ui, |ui| {
            let (r, _) = ui.allocate_exact_size(vec2(200.0, 1000.0), Sense::FOCUSABLE);
            *seen.lock().unwrap() = r;
        });
    });
    let before = *rect.lock().unwrap();
    assert!(walk::press(&mut h, TAB).is_some());
    assert_eq!(*rect.lock().unwrap(), before);
}

#[test]
fn windowed_rows_go_on_as_the_scrolling_draws_them() {
    let mut h = page(|ui| {
        let height = ui.spacing().interact_size.y;
        ScrollArea::vertical().show_rows(ui, height, 500, |ui, rows| {
            for i in rows {
                let _ = ui.button(format!("Row {i}"));
            }
        });
    });
    let mut stop = None;
    for _ in 0..60 {
        let now = walk::press(&mut h, TAB).expect("the focus stays");
        assert!(whole(&h, &now), "{now}");
        stop = Some(now);
    }
    assert_eq!(stop.take().map(|s| s.name).as_deref(), Some("Row 59"));
    for _ in 0..59 {
        let now = walk::press(&mut h, SHIFT_TAB).expect("the focus stays");
        assert!(whole(&h, &now), "{now}");
        stop = Some(now);
    }
    assert_eq!(stop.map(|s| s.name).as_deref(), Some("Row 0"));
}

/// Why the focus plugin does not simply call `scroll_to_me`: what egui itself
/// does for a target inside nested areas (docs/upstream.md). When this fails,
/// egui has fixed it and the plugin's search can go.
#[test]
fn egui_scroll_target_stops_at_the_innermost_area() {
    let mut h = Harness::builder().with_size(SIZE).build_ui(|ui| {
        ScrollArea::vertical().id_salt("outer").show(ui, |ui| {
            ui.allocate_space(vec2(10.0, 400.0));
            ScrollArea::vertical()
                .id_salt("inner")
                .max_height(60.0)
                .show(ui, |ui| {
                    let r = ui.button("Inside");
                    if r.has_focus() {
                        r.scroll_to_me(None);
                    }
                    ui.allocate_space(vec2(10.0, 200.0));
                });
        });
    });
    let _ = h.run_ok();
    let stop = walk::press(&mut h, TAB).expect("Tab finds it");
    assert_eq!(stop.name, "Inside");
    assert!(!stop.visible, "the outer area did not move");
}
