//! Keys on `UiPop::scroll_area`, by real key presses.

use std::cell::Cell;
use std::rc::Rc;

use egui::scroll_area::State;
use egui::{Context, Id, Key, Modifiers, ScrollArea, Slider, TextEdit, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_core::focus;
use egui_pop_core::keys::KeyDispatcher;
use egui_pop_core::theme::{ThemeSpec, Tokens};
use egui_pop_core::ui::UiPop as _;
use egui_pop_model::ids::OwnerId;
use egui_pop_model::keymap::{Action, Keymap, Layer};
use egui_pop_model::when::WhenContext;
use egui_pop_test::themed;
use egui_pop_test::walk::{self, End, Press, SHIFT_TAB, TAB};

const AREA: &str = "keys";
const SCROLLABLE: &str = "Scrollable content";

fn area() -> ScrollArea {
    ScrollArea::vertical()
        .id_salt(AREA)
        .max_height(200.0)
        .auto_shrink(false)
}

/// Labels too tall for the area, nothing to focus.
fn text(ui: &mut egui::Ui) {
    for i in 0..60 {
        ui.label(format!("Line {i}"));
    }
}

/// The offset of an area drawn at the root, by its salt.
fn offset_of(ctx: &Context, id: Id) -> egui::Vec2 {
    State::load(ctx, id).map_or(egui::Vec2::ZERO, |s| s.offset)
}

/// A harness drawing `content` in the area, and where to read its offset.
fn with<'a>(mut content: impl FnMut(&mut egui::Ui) + 'a) -> (Harness<'a>, Rc<Cell<Id>>) {
    let id = Rc::new(Cell::new(Id::NULL));
    let out = id.clone();
    let mut h = themed(&ThemeSpec::default(), vec2(300.0, 300.0), move |ui| {
        let drawn = ui.scroll_area(area(), &mut content);
        out.set(drawn.id);
    });
    h.run();
    (h, id)
}

fn y(h: &Harness<'_>, id: &Rc<Cell<Id>>) -> f32 {
    offset_of(&h.ctx, id.get()).y
}

/// Presses a key with the pointer where it is.
fn press(h: &mut Harness<'_>, key: Key) {
    let _ = walk::press(h, Press::key(key));
}

/// The pointer over the area.
fn hover(h: &mut Harness<'_>) {
    h.hover_at(pos2(100.0, 100.0));
    h.run();
}

/// What a page is here: the view's height less a row.
fn page(h: &Harness<'_>) -> f32 {
    let style = h.ctx.global_style();
    let line = h
        .ctx
        .fonts_mut(|f| f.row_height(&style.text_styles[&egui::TextStyle::Body]));
    200.0 - (line + style.spacing.item_spacing.y)
}

#[test]
fn page_keys_move_the_area_under_the_pointer_by_its_height_less_a_row() {
    let (mut h, id) = with(text);
    hover(&mut h);
    let page = page(&h);
    press(&mut h, Key::PageDown);
    assert!((y(&h, &id) - page).abs() < 1.0, "{} {page}", y(&h, &id));
    press(&mut h, Key::PageDown);
    assert!((y(&h, &id) - 2.0 * page).abs() < 1.0, "{}", y(&h, &id));
    press(&mut h, Key::PageUp);
    assert!((y(&h, &id) - page).abs() < 1.0, "{}", y(&h, &id));
}

#[test]
fn home_and_end_go_to_the_ends_and_the_arrows_by_a_line() {
    let (mut h, id) = with(text);
    hover(&mut h);
    press(&mut h, Key::End);
    let end = y(&h, &id);
    assert!(end > 400.0, "{end}");
    press(&mut h, Key::PageDown);
    assert_eq!(y(&h, &id), end, "stays at its end");
    press(&mut h, Key::Home);
    assert_eq!(y(&h, &id), 0.0);
    press(&mut h, Key::ArrowDown);
    let line = y(&h, &id);
    assert!(line > 5.0 && line < 40.0, "{line}");
    press(&mut h, Key::ArrowUp);
    assert_eq!(y(&h, &id), 0.0);
}

#[test]
fn with_the_pointer_elsewhere_and_no_focus_the_keys_go_nowhere() {
    let (mut h, id) = with(text);
    h.hover_at(pos2(100.0, 280.0));
    h.run();
    press(&mut h, Key::PageDown);
    assert_eq!(y(&h, &id), 0.0);
}

#[test]
fn a_key_with_a_modifier_is_not_the_areas() {
    let (mut h, id) = with(text);
    hover(&mut h);
    h.key_press_modifiers(Modifiers::CTRL, Key::End);
    h.run();
    assert_eq!(y(&h, &id), 0.0);
}

// A Tab stop only when nothing else can be.

#[test]
fn an_area_with_nothing_focusable_is_a_stop_that_takes_the_keys() {
    let (mut h, id) = with(text);
    let stop = walk::reach(&mut h, TAB, SCROLLABLE, 2).expect("Tab gets there");
    assert_eq!(stop.role, egui::accesskit::Role::ScrollView);
    assert!(stop.visible);
    // The pointer is nowhere: the focus is enough, the arrows too.
    press(&mut h, Key::PageDown);
    let down = y(&h, &id);
    assert!(down > 100.0, "{down}");
    press(&mut h, Key::ArrowDown);
    assert!(y(&h, &id) > down);
    // And they keep the focus where it is.
    assert_eq!(
        walk::focused(&h).map(|s| s.name).as_deref(),
        Some(SCROLLABLE)
    );
    egui_pop_test::assert_all_named(&h);
}

#[test]
fn a_focused_area_shows_it() {
    let ring = |h: &Harness<'_>| {
        let colour = Tokens::of(&h.ctx).focus_ring;
        h.output().shapes.iter().any(|s| {
            matches!(&s.shape, egui::Shape::Rect(r) if r.stroke.color == colour && r.stroke.width > 0.0)
        })
    };
    let (mut h, _) = with(text);
    assert!(!ring(&h));
    walk::reach(&mut h, TAB, SCROLLABLE, 2).expect("Tab gets there");
    assert!(ring(&h), "a stop that shows nothing");
}

#[test]
fn an_area_with_nothing_to_scroll_is_no_stop() {
    let (mut h, _) = with(|ui| {
        ui.label("Short");
    });
    let walked = walk::walk(&mut h, TAB, 5);
    assert_eq!(walked.names(), Vec::<&str>::new());
    assert_eq!(walked.end, End::Lost);
}

#[test]
fn an_area_with_something_focusable_is_no_stop() {
    let (mut h, _) = with(|ui| {
        text(ui);
        let _ = ui.button("Inside");
    });
    let walked = walk::walk(&mut h, TAB, 5);
    assert_eq!(walked.names(), ["Inside"]);
    let back = walk::walk(&mut h, SHIFT_TAB, 5);
    assert_eq!(back.names(), ["Inside"]);
}

#[test]
fn an_area_that_starts_to_fit_stops_being_a_stop() {
    let tall = Rc::new(Cell::new(true));
    let flag = tall.clone();
    let (mut h, _) = with(move |ui| {
        if flag.get() {
            text(ui);
        } else {
            ui.label("Short");
        }
    });
    walk::reach(&mut h, TAB, SCROLLABLE, 2).expect("Tab gets there");
    tall.set(false);
    h.run();
    assert_eq!(walk::focused(&h), None);
}

// A widget inside goes first.

#[test]
fn a_focused_button_inside_lets_the_area_page_and_keeps_the_arrows() {
    let (mut h, id) = with(|ui| {
        let _ = ui.button("Inside");
        text(ui);
    });
    walk::reach(&mut h, TAB, "Inside", 2).expect("Tab gets there");
    press(&mut h, Key::ArrowDown);
    assert_eq!(y(&h, &id), 0.0, "the arrows are egui's");
    press(&mut h, Key::PageDown);
    assert!(y(&h, &id) > 100.0);
    press(&mut h, Key::Home);
    assert_eq!(y(&h, &id), 0.0);
}

#[test]
fn a_focused_text_field_inside_keeps_the_keys() {
    let mut value = String::from("text");
    let (mut h, id) = with(move |ui| {
        ui.add(TextEdit::singleline(&mut value));
        text(ui);
    });
    walk::press(&mut h, TAB).expect("Tab gets there");
    press(&mut h, Key::PageDown);
    press(&mut h, Key::End);
    assert_eq!(y(&h, &id), 0.0);
}

#[test]
fn a_focused_slider_inside_keeps_the_keys() {
    let mut value = 5.0_f32;
    let (mut h, id) = with(move |ui| {
        ui.add(Slider::new(&mut value, 0.0..=10.0).show_value(false));
        text(ui);
    });
    walk::press(&mut h, TAB).expect("Tab gets there");
    press(&mut h, Key::PageDown);
    press(&mut h, Key::End);
    assert_eq!(y(&h, &id), 0.0);
}

#[test]
fn a_widget_that_says_it_takes_the_keys_keeps_them() {
    let (mut h, id) = with(|ui| {
        let r = ui.button("A list of its own");
        focus::takes_keys(ui.ctx(), r.id);
        text(ui);
    });
    walk::reach(&mut h, TAB, "A list of its own", 2).expect("Tab gets there");
    press(&mut h, Key::PageDown);
    assert_eq!(y(&h, &id), 0.0);
}

#[test]
fn a_focus_elsewhere_keeps_the_area_under_the_pointer_still() {
    let id = Rc::new(Cell::new(Id::NULL));
    let out = id.clone();
    let mut h = themed(&ThemeSpec::default(), vec2(300.0, 300.0), move |ui| {
        let _ = ui.button("Outside");
        out.set(ui.scroll_area(area(), text).id);
    });
    h.run();
    hover(&mut h);
    walk::reach(&mut h, TAB, "Outside", 2).expect("Tab gets there");
    press(&mut h, Key::PageDown);
    assert_eq!(y(&h, &id), 0.0);
}

#[test]
fn an_open_popup_keeps_the_area_under_it_still() {
    let (mut h, id) = with(|ui| {
        egui::Popup::menu(&ui.button("Menu")).show(|ui| {
            let _ = ui.label("In the menu");
        });
        text(ui);
    });
    h.get_by_label("Menu").click();
    h.run();
    h.ctx.memory_mut(|m| {
        if let Some(f) = m.focused() {
            m.surrender_focus(f);
        }
    });
    hover(&mut h);
    press(&mut h, Key::PageDown);
    assert_eq!(y(&h, &id), 0.0);
}

// Nesting: the innermost first, then what is around it.

#[test]
fn the_innermost_area_goes_first_then_the_one_around_it() {
    let inner = Rc::new(Cell::new(Id::NULL));
    let outer = Rc::new(Cell::new(Id::NULL));
    let (i, o) = (inner.clone(), outer.clone());
    let mut h = themed(&ThemeSpec::default(), vec2(300.0, 300.0), move |ui| {
        let drawn = ui.scroll_area(area(), |ui| {
            let area = ScrollArea::vertical()
                .id_salt("inner")
                .max_height(100.0)
                .auto_shrink(false);
            i.set(ui.scroll_area(area, text).id);
            text(ui);
        });
        o.set(drawn.id);
    });
    h.run();
    h.hover_at(pos2(100.0, 50.0));
    h.run();
    press(&mut h, Key::PageDown);
    assert!(y(&h, &inner) > 50.0);
    assert_eq!(y(&h, &outer), 0.0);
    press(&mut h, Key::End);
    assert_eq!(y(&h, &outer), 0.0, "the inner one still had a way to go");
    press(&mut h, Key::PageDown);
    assert!(y(&h, &outer) > 50.0, "at its end, the key goes on");
}

// A binding goes first.

/// An area and the dispatcher, as an application runs them, with `key` bound.
fn bound<'a>(key: &str) -> (Harness<'a>, Rc<Cell<Id>>, Rc<Cell<u32>>) {
    let mut keymap = Keymap::new();
    keymap
        .bind(
            &OwnerId::FRAMEWORK,
            Layer::Default,
            key,
            Action::run("test.bound"),
            "",
        )
        .unwrap();
    let mut dispatcher = KeyDispatcher::new();
    let (id, ran) = (Rc::new(Cell::new(Id::NULL)), Rc::new(Cell::new(0)));
    let (out, count) = (id.clone(), ran.clone());
    let mut h = themed(&ThemeSpec::default(), vec2(300.0, 300.0), move |ui| {
        let done = dispatcher.process(ui.ctx(), &keymap, &WhenContext::default());
        count.set(count.get() + done.len() as u32);
        out.set(ui.scroll_area(area(), text).id);
    });
    h.run();
    (h, id, ran)
}

#[test]
fn a_binding_wins_over_the_area() {
    let (mut h, id, ran) = bound("PageDown");
    hover(&mut h);
    press(&mut h, Key::PageDown);
    assert_eq!(ran.get(), 1);
    assert_eq!(y(&h, &id), 0.0);
    // What is not bound still goes to the area.
    press(&mut h, Key::End);
    assert!(y(&h, &id) > 100.0);
}

#[test]
fn a_binding_wins_while_its_key_is_held() {
    let (mut h, id, ran) = bound("PageDown");
    hover(&mut h);
    let key = Press::key(Key::PageDown);
    walk::hold(&mut h, key);
    walk::hold(&mut h, key);
    walk::hold(&mut h, key);
    walk::release(&mut h, key);
    assert_eq!(ran.get(), 1, "a press is one run");
    assert_eq!(y(&h, &id), 0.0, "and what it repeats reaches nothing");
}

#[test]
fn a_held_key_moves_the_area_on() {
    let (mut h, id) = with(text);
    hover(&mut h);
    let key = Press::key(Key::PageDown);
    walk::hold(&mut h, key);
    let once = y(&h, &id);
    walk::hold(&mut h, key);
    assert!(y(&h, &id) > once);
    walk::release(&mut h, key);
}

#[test]
fn a_disabled_area_takes_no_key_and_is_no_stop() {
    let id = Rc::new(Cell::new(Id::NULL));
    let out = id.clone();
    let mut h = themed(&ThemeSpec::default(), vec2(300.0, 300.0), move |ui| {
        ui.disable();
        out.set(ui.scroll_area(area(), text).id);
    });
    h.run();
    hover(&mut h);
    press(&mut h, Key::PageDown);
    assert_eq!(y(&h, &id), 0.0);
    assert_eq!(walk::walk(&mut h, TAB, 3).names(), Vec::<&str>::new());
}
