//! Focus keeps its place: real key presses over plain widgets, with the focus
//! plugin and nothing else from egui-pop in the page.

use std::sync::{Arc, Mutex};

use egui::{Id, Key, Modifiers, PointerButton, Pos2, ScrollArea, TextEdit, Ui, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_core::focus;
use egui_pop_core::overlay::{self, Anchor, Kind, OverlaySpec};
use egui_pop_core::placement::Side;
use egui_pop_core::theme::ThemeSpec;
use egui_pop_test::themed;
use egui_pop_test::walk::{self, ESCAPE, End, Press, SHIFT_TAB, Stop, TAB};

const ENTER: Press = Press::key(Key::Enter);

fn page<'a>(ui: impl FnMut(&mut Ui) + 'a) -> Harness<'a> {
    let mut h = themed(&ThemeSpec::default(), vec2(300.0, 300.0), ui);
    focus::install(&h.ctx);
    let _ = h.run_ok();
    h
}

type Shared<T> = Arc<Mutex<T>>;

/// A list of buttons that can change, and the page drawing it.
fn rows<'a>(names: &[&str]) -> (Harness<'a>, Shared<Vec<String>>) {
    let rows: Shared<Vec<String>> =
        Arc::new(Mutex::new(names.iter().map(|n| n.to_string()).collect()));
    let drawn = rows.clone();
    let h = page(move |ui| {
        for name in drawn.lock().unwrap().iter() {
            ui.push_id(name, |ui| {
                let _ = ui.button(name);
            });
        }
    });
    (h, rows)
}

fn name(stop: Option<Stop>) -> String {
    stop.map_or_else(|| "nothing".to_string(), |s| s.name)
}

#[track_caller]
fn reach<S>(h: &mut Harness<'_, S>, stop: &str) {
    walk::reach(h, TAB, stop, 20).unwrap_or_else(|| panic!("Tab reaches {stop}"));
}

fn remove(rows: &Shared<Vec<String>>, row: &str) {
    rows.lock().unwrap().retain(|r| r != row);
}

#[test]
fn tab_after_escape_goes_on_from_the_place() {
    let (mut h, _) = rows(&["One", "Two", "Three"]);
    reach(&mut h, "Two");
    assert_eq!(walk::press(&mut h, ESCAPE), None);
    assert_eq!(name(walk::press(&mut h, TAB)), "Three");
}

#[test]
fn shift_tab_after_escape_goes_back_from_the_place() {
    let (mut h, _) = rows(&["One", "Two", "Three"]);
    reach(&mut h, "Two");
    assert_eq!(walk::press(&mut h, ESCAPE), None);
    assert_eq!(name(walk::press(&mut h, SHIFT_TAB)), "One");
}

#[test]
fn tab_wraps_as_usual_from_the_last_place() {
    let (mut h, _) = rows(&["One", "Two"]);
    reach(&mut h, "Two");
    assert_eq!(walk::press(&mut h, ESCAPE), None);
    assert_eq!(name(walk::press(&mut h, TAB)), "One");
}

#[test]
fn escape_alone_leaves_nothing_focused() {
    let (mut h, _) = rows(&["One", "Two"]);
    reach(&mut h, "One");
    assert_eq!(walk::press(&mut h, ESCAPE), None);
    let _ = h.run_ok();
    assert_eq!(walk::focused(&h), None);
}

#[test]
fn a_removed_row_hands_the_focus_to_the_one_taking_its_place() {
    let (mut h, rows) = rows(&["One", "Two", "Three"]);
    reach(&mut h, "Two");
    remove(&rows, "Two");
    let _ = h.run_ok();
    assert_eq!(name(walk::focused(&h)), "Three");
}

#[test]
fn the_last_row_removed_hands_the_focus_back_a_row() {
    let (mut h, rows) = rows(&["One", "Two", "Three"]);
    reach(&mut h, "Three");
    remove(&rows, "Three");
    let _ = h.run_ok();
    assert_eq!(name(walk::focused(&h)), "Two");
}

#[test]
fn removing_every_row_leaves_nothing_focused() {
    let (mut h, rows) = rows(&["One"]);
    reach(&mut h, "One");
    remove(&rows, "One");
    let _ = h.run_ok();
    assert_eq!(walk::focused(&h), None);
}

#[test]
fn a_removed_widget_stays_in_its_scope_before_a_nearer_stop() {
    let gone: Shared<bool> = Arc::default();
    let drawn = gone.clone();
    let mut h = page(move |ui| {
        focus::scope(ui, "left", &[], |ui| {
            let _ = ui.button("Far");
            ui.add_space(100.0);
            if !*drawn.lock().unwrap() {
                let _ = ui.button("Row");
            }
        });
        focus::scope(ui, "right", &[], |ui| {
            let _ = ui.button("Near");
        });
    });
    reach(&mut h, "Row");
    *gone.lock().unwrap() = true;
    let _ = h.run_ok();
    assert_eq!(name(walk::focused(&h)), "Far");
}

#[test]
fn tab_after_the_place_went_away_goes_to_what_took_it() {
    let (mut h, rows) = rows(&["One", "Two", "Three"]);
    reach(&mut h, "Two");
    assert_eq!(walk::press(&mut h, ESCAPE), None);
    remove(&rows, "Two");
    let _ = h.run_ok();
    assert_eq!(
        walk::focused(&h),
        None,
        "Escape asked for nothing to hold it"
    );
    assert_eq!(name(walk::press(&mut h, TAB)), "Three");
}

#[test]
fn a_place_going_away_in_the_pass_of_tab_hands_on_to_a_stop_drawn() {
    // Given back at the start of the pass, then not drawn in it: AccessKit
    // would panic on the focus, which is what running this checks.
    let (mut h, rows) = rows(&["One", "Two", "Three"]);
    reach(&mut h, "Two");
    assert_eq!(walk::press(&mut h, ESCAPE), None);
    remove(&rows, "Two");
    assert_eq!(name(walk::press(&mut h, TAB)), "Three");
}

#[test]
fn a_widget_disabled_while_focused_lets_tab_go_on() {
    let off: Shared<bool> = Arc::default();
    let drawn = off.clone();
    let mut h = page(move |ui| {
        let _ = ui.button("One");
        ui.add_enabled_ui(!*drawn.lock().unwrap(), |ui| {
            let _ = ui.button("Two");
        });
        let _ = ui.button("Three");
    });
    reach(&mut h, "Two");
    *off.lock().unwrap() = true;
    let _ = h.run_ok();
    assert_eq!(walk::focused(&h), None);
    assert_eq!(name(walk::press(&mut h, TAB)), "Three");
}

#[test]
fn a_press_on_nothing_forgets_the_place() {
    let (mut h, _) = rows(&["One", "Two", "Three"]);
    reach(&mut h, "Two");
    let blank = Pos2::new(250.0, 280.0);
    h.event(egui::Event::PointerMoved(blank));
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: blank,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        let _ = h.run_ok();
    }
    assert_eq!(walk::focused(&h), None);
    assert_eq!(name(walk::press(&mut h, TAB)), "One");
}

#[test]
fn a_binding_on_tab_leaves_nothing_focused_after_escape() {
    let on: Shared<bool> = Arc::default();
    let taken: Shared<u32> = Arc::default();
    let (bound, count) = (on.clone(), taken.clone());
    let mut h = page(move |ui| {
        let nothing = ui.memory(|m| m.focused()).is_none();
        if *bound.lock().unwrap()
            && nothing
            && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Tab))
        {
            *count.lock().unwrap() += 1;
        }
        let _ = ui.button("One");
        let _ = ui.button("Two");
    });
    reach(&mut h, "One");
    *on.lock().unwrap() = true;
    assert_eq!(walk::press(&mut h, ESCAPE), None);
    assert_eq!(walk::press(&mut h, TAB), None, "the binding took it");
    assert_eq!(*taken.lock().unwrap(), 1);
}

#[test]
fn a_place_under_a_modal_leaves_tab_in_the_modal() {
    let open: Shared<bool> = Arc::default();
    let drawn = open.clone();
    let mut h = page(move |ui| {
        let _ = ui.button("Behind");
        let _ = ui.button("Next");
        if *drawn.lock().unwrap() {
            egui::Modal::new(Id::new("modal")).show(ui.ctx(), |ui| {
                let _ = ui.button("Inside");
            });
        }
    });
    reach(&mut h, "Behind");
    assert_eq!(walk::press(&mut h, ESCAPE), None);
    *open.lock().unwrap() = true;
    let _ = h.run_ok();
    assert_eq!(name(walk::press(&mut h, TAB)), "Inside");
}

#[test]
fn a_place_in_a_layer_gone_leaves_tab_to_egui() {
    let open: Shared<bool> = Arc::new(Mutex::new(true));
    let drawn = open.clone();
    let mut h = page(move |ui| {
        let _ = ui.button("One");
        let _ = ui.button("Two");
        if *drawn.lock().unwrap() {
            egui::Area::new(Id::new("popup"))
                .fixed_pos(Pos2::new(150.0, 150.0))
                .show(ui.ctx(), |ui| {
                    let _ = ui.button("Popped");
                });
        }
    });
    reach(&mut h, "Popped");
    *open.lock().unwrap() = false;
    let _ = h.run_ok();
    assert_eq!(walk::focused(&h), None, "nothing is left in that layer");
    assert_eq!(name(walk::press(&mut h, TAB)), "One");
}

#[test]
fn a_widget_given_the_focus_back_does_not_read_the_key_moving_on() {
    // A text editor takes Tab and Shift+Tab as its own while it has the focus:
    // it must not edit because Tab goes on from it.
    for key in [TAB, SHIFT_TAB] {
        let read: Shared<u32> = Arc::default();
        let seen = read.clone();
        let mut h = page(move |ui| {
            let _ = ui.button("One");
            let r = ui.button("Editor");
            if r.has_focus() && ui.input(|i| i.key_pressed(Key::Tab)) {
                *seen.lock().unwrap() += 1;
            }
            let _ = ui.button("Three");
        });
        reach(&mut h, "Editor");
        assert_eq!(walk::press(&mut h, ESCAPE), None);
        // egui's own Tab shows the key to the widget it lands on: not counted.
        *read.lock().unwrap() = 0;
        let _ = walk::press(&mut h, key);
        assert_eq!(*read.lock().unwrap(), 0, "{key:?}");
    }
}

/// Buttons in a windowed area, scrolled by the wheel.
fn windowed<'a>() -> Harness<'a> {
    page(|ui| {
        let _ = ui.button("Before");
        ScrollArea::vertical()
            .max_height(150.0)
            .show_rows(ui, 20.0, 500, |ui, range| {
                for i in range {
                    let _ = ui.button(format!("Row {}", i + 1));
                }
            });
    })
}

fn wheel<S>(h: &mut Harness<'_, S>, at: Pos2, times: usize) {
    h.event(egui::Event::PointerMoved(at));
    let _ = h.run_ok();
    for _ in 0..times {
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: vec2(0.0, -20.0),
            phase: egui::TouchPhase::Move,
            modifiers: Modifiers::NONE,
        });
        let _ = h.run_ok();
    }
}

#[test]
fn a_row_scrolled_away_hands_nothing_on_and_tab_goes_on_from_the_place() {
    let mut h = windowed();
    let row = walk::reach(&mut h, TAB, "Row 3", 10).expect("Tab reaches it");
    wheel(&mut h, row.rect.center(), 6);
    assert!(h.query_by_label("Row 3").is_none(), "scrolled away");
    assert_eq!(walk::focused(&h), None, "the wheel does not move the focus");
    let next = walk::press(&mut h, TAB).expect("Tab takes a row");
    assert!(next.name.starts_with("Row"), "{next}");
    assert!(next.visible, "{next}");
}

// A popup's frame is no stop (keyboard survey, item 8).

/// A button opening a menu, a button after it.
fn menu<'a>() -> Harness<'a> {
    let mut h = page(|ui| {
        let open = ui.button("Open");
        egui::Popup::menu(&open).show(|ui| {
            let _ = ui.button("One");
            let _ = ui.button("Two");
        });
        let _ = ui.button("After");
    });
    reach(&mut h, "Open");
    walk::press(&mut h, Press::key(Key::Enter));
    h
}

#[test]
fn tab_passes_over_a_popups_frame() {
    let mut h = menu();
    assert_eq!(name(walk::press(&mut h, TAB)), "One");
    assert_eq!(name(walk::press(&mut h, TAB)), "Two");
}

#[test]
fn shift_tab_passes_over_a_popups_frame() {
    let mut h = menu();
    assert_eq!(name(walk::press(&mut h, TAB)), "One");
    assert_eq!(name(walk::press(&mut h, SHIFT_TAB)), "Open");
    assert_eq!(name(walk::focused(&h)), "Open");
}

#[test]
fn tab_passes_over_a_combo_boxs_frame() {
    let mut choice = 0;
    let mut h = page(move |ui| {
        egui::ComboBox::from_label("Choice")
            .selected_text("Pick")
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut choice, 0, "First");
                ui.selectable_value(&mut choice, 1, "Second");
            });
    });
    walk::press(&mut h, TAB);
    walk::press(&mut h, Press::key(Key::Enter));
    assert!(h.query_by_label("Second").is_some(), "the list is open");
    assert_eq!(name(walk::press(&mut h, TAB)), "First");
}

#[test]
fn a_popup_with_no_stop_and_none_behind_it_lets_the_focus_go_and_settles() {
    let mut h = page(|ui| {
        let at = ui.label("Anchor");
        egui::Popup::from_response(&at).show(|ui| ui.label("Text alone"));
    });
    h.key_press(Key::Tab);
    assert!(h.try_run().is_ok(), "the focus does not go round for ever");
    assert_eq!(walk::focused(&h), None);
}

#[test]
fn focus_put_on_a_popups_frame_by_other_means_is_dropped() {
    let frame: Shared<Option<Id>> = Arc::default();
    let seen = frame.clone();
    let mut h = page(move |ui| {
        let open = ui.button("Open");
        let shown = egui::Popup::menu(&open).show(|ui| {
            let _ = ui.button("One");
        });
        *seen.lock().unwrap() = shown.map(|s| s.response.layer_id.id.with("move"));
    });
    reach(&mut h, "Open");
    walk::press(&mut h, Press::key(Key::Enter));
    // Long after the key that opened it.
    for _ in 0..4 {
        h.step();
    }
    let id = frame.lock().unwrap().expect("the menu is open");
    h.ctx.memory_mut(|m| m.request_focus(id));
    let _ = h.run_ok();
    assert_eq!(walk::focused(&h), None);
}

// A popover keeps Tab.

/// A page around a popover that the button "Item" opens: "Before" and "After"
/// stand behind it. `inside` is its content.
fn popover<'a>(
    start_open: bool,
    mut inside: impl FnMut(&mut Ui) + 'a,
) -> (Harness<'a>, Shared<bool>) {
    let open = Arc::new(Mutex::new(start_open));
    let flag = open.clone();
    let h = page(move |ui| {
        let _ = ui.button("Before");
        let item = ui.button("Item");
        let mut shown = *flag.lock().unwrap() || item.clicked();
        let spec = OverlaySpec::new("pop", "Options", Kind::Popover);
        let anchor = Anchor {
            rect: item.rect,
            side: Side::Bottom,
        };
        overlay::show(ui.ctx(), &spec, &mut shown, Some(anchor), &mut inside);
        *flag.lock().unwrap() = shown;
        let _ = ui.button("After");
    });
    (h, open)
}

fn two_stops(ui: &mut Ui) {
    let _ = ui.button("One");
    let _ = ui.button("Two");
}

/// The popover open from its item, by the keyboard.
fn opened<'a>(inside: impl FnMut(&mut Ui) + 'a) -> (Harness<'a>, Shared<bool>) {
    let (mut h, open) = popover(false, inside);
    reach(&mut h, "Item");
    walk::press(&mut h, ENTER);
    let _ = h.run_ok();
    assert!(*open.lock().unwrap());
    (h, open)
}

#[test]
fn tab_cycles_in_a_popover_and_escape_goes_back_to_its_item() {
    let (mut h, open) = opened(two_stops);
    let walked = walk::walk(&mut h, TAB, 6);
    assert_eq!(walked.names(), ["One", "Two"], "{walked}");
    assert_eq!(walked.end, End::Looped { at: 0 });
    assert_eq!(name(walk::focused(&h)), "One");
    assert_eq!(name(walk::press(&mut h, SHIFT_TAB)), "Two");
    assert_eq!(name(walk::press(&mut h, SHIFT_TAB)), "One");
    assert!(*open.lock().unwrap());
    assert_eq!(name(walk::press(&mut h, ESCAPE)), "Item");
    assert!(!*open.lock().unwrap());
    egui_pop_test::assert_all_named(&h);
}

#[test]
fn tab_stays_on_a_popovers_only_stop() {
    let (mut h, open) = opened(|ui| {
        let _ = ui.button("Only");
    });
    assert_eq!(name(walk::press(&mut h, TAB)), "Only");
    assert_eq!(name(walk::press(&mut h, TAB)), "Only");
    assert_eq!(name(walk::press(&mut h, SHIFT_TAB)), "Only");
    assert!(*open.lock().unwrap());
}

#[test]
fn shift_tab_from_the_item_goes_to_the_popovers_last_stop() {
    let (mut h, open) = opened(two_stops);
    assert_eq!(name(walk::press(&mut h, SHIFT_TAB)), "Two");
    assert!(*open.lock().unwrap());
}

#[test]
fn tab_from_nothing_or_from_behind_goes_into_a_popover() {
    let (mut h, _) = popover(true, two_stops);
    assert_eq!(name(walk::press(&mut h, TAB)), "One");
    let (mut h, _) = popover(true, two_stops);
    assert_eq!(name(walk::press(&mut h, SHIFT_TAB)), "Two");
    // From a widget drawn before it, and one after it.
    let (mut h, open) = opened(two_stops);
    *open.lock().unwrap() = false;
    let _ = h.run_ok();
    reach(&mut h, "Before");
    *open.lock().unwrap() = true;
    let _ = h.run_ok();
    assert_eq!(name(walk::press(&mut h, TAB)), "One");
    *open.lock().unwrap() = false;
    let _ = h.run_ok();
    reach(&mut h, "After");
    *open.lock().unwrap() = true;
    let _ = h.run_ok();
    assert_eq!(name(walk::press(&mut h, SHIFT_TAB)), "Two");
}

#[test]
fn a_popover_with_no_stop_closes_as_tab_goes_on() {
    let (mut h, open) = opened(|ui| {
        ui.label("Only text");
    });
    assert_eq!(name(walk::press(&mut h, TAB)), "After");
    assert!(!*open.lock().unwrap());
    let (mut h, open) = opened(|ui| {
        ui.label("Only text");
    });
    assert_eq!(name(walk::press(&mut h, SHIFT_TAB)), "Before");
    assert!(!*open.lock().unwrap());
}

#[test]
fn a_widget_taking_tab_keeps_it_at_a_popovers_end() {
    let mut code = String::new();
    let (mut h, open) = opened(move |ui| {
        let _ = ui.button("One");
        ui.add(TextEdit::multiline(&mut code).lock_focus(true));
    });
    assert_eq!(name(walk::press(&mut h, TAB)), "One");
    let field = walk::press(&mut h, TAB).expect("Tab reaches the field");
    assert_ne!(field.name, "One");
    assert_eq!(walk::press(&mut h, TAB).map(|s| s.id), Some(field.id));
    assert!(*open.lock().unwrap());
}

#[test]
fn an_arrow_does_not_rest_on_the_end_of_a_popovers_order() {
    let (mut h, _) = opened(two_stops);
    assert_eq!(name(walk::press(&mut h, SHIFT_TAB)), "Two");
    for key in [
        Key::ArrowDown,
        Key::ArrowLeft,
        Key::ArrowRight,
        Key::ArrowUp,
    ] {
        let at = walk::press(&mut h, Press::key(key)).expect("the focus stays");
        assert!(!at.name.is_empty(), "{key:?} landed on a stop with no name");
    }
}

#[test]
fn a_removed_last_stop_hands_the_focus_to_a_real_stop() {
    let rows: Shared<Vec<&str>> = Arc::new(Mutex::new(vec!["One", "Two"]));
    let drawn = rows.clone();
    let (mut h, _) = opened(move |ui| {
        for row in drawn.lock().unwrap().iter() {
            let _ = ui.button(*row);
        }
    });
    assert_eq!(name(walk::press(&mut h, SHIFT_TAB)), "Two");
    rows.lock().unwrap().retain(|r| *r != "Two");
    let _ = h.run_ok();
    assert_eq!(name(walk::focused(&h)), "One");
}

#[test]
fn a_flyout_held_by_its_owner_lets_tab_go_on() {
    let mut open = true;
    let mut h = page(move |ui| {
        let spec = OverlaySpec::new("flyout", "Flyout", Kind::Popover);
        let (ctx, rect) = (ui.ctx().clone(), ui.max_rect());
        overlay::hold(&ctx, &spec, &mut open, None, rect, || {
            let _ = ui.button("Inside");
        });
        let _ = ui.button("After");
    });
    assert_eq!(name(walk::press(&mut h, TAB)), "Inside");
    assert_eq!(name(walk::press(&mut h, TAB)), "After");
}

#[test]
fn a_menu_of_the_stack_is_left_to_its_own_keys() {
    let mut open = true;
    let mut h = page(move |ui| {
        let spec = OverlaySpec::new("menu", "Menu", Kind::Menu);
        overlay::show(ui.ctx(), &spec, &mut open, None, |ui| {
            let _ = ui.button("Entry");
        });
        let _ = ui.button("After");
    });
    assert_eq!(name(walk::press(&mut h, TAB)), "Entry");
    assert_eq!(name(walk::press(&mut h, TAB)), "After");
}

#[test]
fn an_egui_popup_keeps_tab_when_it_asks() {
    let id = Id::new("popup");
    let mut h = page(move |ui| {
        let _ = ui.button("Before");
        let item = ui.button("Item");
        egui::Popup::from_toggle_button_response(&item)
            .id(id)
            .kind(egui::PopupKind::Popup)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                two_stops(ui);
                focus::keep_tab(ui);
            });
        let _ = ui.button("After");
    });
    reach(&mut h, "Item");
    walk::press(&mut h, ENTER);
    let _ = h.run_ok();
    assert!(egui::Popup::is_id_open(&h.ctx, id));
    let walked = walk::walk(&mut h, TAB, 6);
    assert_eq!(walked.names(), ["One", "Two"], "{walked}");
    assert_eq!(walked.end, End::Looped { at: 0 });
    assert_eq!(name(walk::press(&mut h, SHIFT_TAB)), "Two");
    assert!(egui::Popup::is_id_open(&h.ctx, id));
}

#[test]
fn keep_tab_without_the_focus_plugin_leaves_tab_as_it_was() {
    let mut open = true;
    let mut h = themed(&ThemeSpec::default(), vec2(300.0, 300.0), move |ui| {
        let spec = OverlaySpec::new("pop", "Options", Kind::Popover);
        overlay::show(ui.ctx(), &spec, &mut open, None, |ui| {
            let _ = ui.button("Inside");
        });
        let _ = ui.button("After");
    });
    let _ = h.run_ok();
    assert_eq!(name(walk::press(&mut h, TAB)), "Inside");
    assert_eq!(name(walk::press(&mut h, TAB)), "After");
}
