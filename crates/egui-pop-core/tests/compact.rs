//! Compact controls (docs/compact.md): a choice cell keeps its slot
//! whatever the word, is named by its name and valued by its word,
//! chooses with the arrows, Home and End while its list is closed, and
//! opens with Space; a menu row fills its room, is a button named by its
//! text and note, focused by Tab and pressed by Space or Enter.

use egui::accesskit::Role;
use egui::{Id, Key, Rect, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_core::compact::{ChoiceCell, MenuRow};
use egui_pop_core::theme::{self, ThemeSpec};

const MODES: &[&str] = &[
    "Normal",
    "Multiply",
    "A much longer word than the slot holds",
];
const SLOT: egui::Vec2 = vec2(120.0, 22.0);

struct Cell {
    choices: &'static [&'static str],
    selected: usize,
    changes: u32,
    rect: Option<Rect>,
    id: Option<Id>,
}

fn cell(choices: &'static [&'static str], selected: usize) -> Harness<'static, Cell> {
    let mut h = Harness::builder()
        .with_size(vec2(320.0, 200.0))
        .build_ui_state(
            |ui, s: &mut Cell| {
                ui.horizontal(|ui| {
                    let r = ChoiceCell::new("Blend mode", &mut s.selected, s.choices)
                        .size(SLOT)
                        .show(ui);
                    if r.changed() {
                        s.changes += 1;
                    }
                    s.rect = Some(r.rect);
                    s.id = Some(r.id);
                    ui.label("after");
                });
            },
            Cell {
                choices,
                selected,
                changes: 0,
                rect: None,
                id: None,
            },
        );
    theme::install(&h.ctx, &ThemeSpec::new("pop-dark"));
    h.run();
    h
}

fn word(h: &Harness<'static, Cell>) -> Option<String> {
    h.get_by_label("Blend mode").value()
}

fn press(h: &mut Harness<'static, Cell>, key: Key) {
    h.key_press(key);
    h.run();
}

#[test]
fn a_choice_cell_is_a_combo_box_named_by_its_name_and_valued_by_its_word() {
    let h = cell(MODES, 1);
    let node = h.get_by_label("Blend mode");
    assert_eq!(node.accesskit_node().role(), Role::ComboBox);
    assert_eq!(node.value().as_deref(), Some("Multiply"));
    assert!(
        node.rect().height() <= SLOT.y + 0.5,
        "no taller than its slot: {:?}",
        node.rect()
    );
}

#[test]
fn a_choice_cell_keeps_its_slot_whatever_the_word() {
    let mut h = cell(MODES, 0);
    let short = h.state().rect.unwrap();
    let after = h.get_by_label("after").rect();
    h.state_mut().selected = 2;
    h.run();
    assert_eq!(
        word(&h).as_deref(),
        Some(MODES[2]),
        "the whole word is read"
    );
    assert_eq!(
        h.state().rect.unwrap(),
        short,
        "the long word is cut, not the slot widened"
    );
    assert_eq!(
        h.get_by_label("after").rect(),
        after,
        "nothing after it moves"
    );
}

#[test]
fn the_arrows_home_and_end_choose_while_the_list_is_closed() {
    let mut h = cell(MODES, 0);
    h.get_by_label("Blend mode").focus();
    h.run();
    press(&mut h, Key::ArrowDown);
    assert_eq!(word(&h).as_deref(), Some("Multiply"));
    assert_eq!(h.state().changes, 1, "a choice marks the response changed");
    press(&mut h, Key::End);
    assert_eq!(h.state().selected, 2);
    press(&mut h, Key::ArrowDown);
    assert_eq!(h.state().selected, 2, "stays at the last");
    press(&mut h, Key::Home);
    assert_eq!(h.state().selected, 0);
    press(&mut h, Key::ArrowUp);
    assert_eq!(h.state().selected, 0, "stays at the first");
    assert_eq!(
        h.state().changes,
        3,
        "a key that changes nothing is no change"
    );
    assert!(
        h.get_by_label("Blend mode").is_focused(),
        "the arrows choose, they do not move the focus"
    );
}

#[test]
fn space_opens_the_list_of_words() {
    let mut h = cell(MODES, 0);
    h.get_by_label("Blend mode").focus();
    h.run();
    let id = h.state().id.unwrap();
    assert!(!egui::ComboBox::is_open(&h.ctx, id));
    press(&mut h, Key::Space);
    assert!(egui::ComboBox::is_open(&h.ctx, id), "Space opens it");
    assert!(
        h.query_by_label("Multiply").is_some(),
        "its words are shown"
    );
    assert_eq!(h.state().selected, 0, "opening chooses nothing");
}

#[test]
fn a_choice_cell_without_words_shows_nothing_and_takes_no_key() {
    let mut h = cell(&[], 0);
    assert_eq!(word(&h).as_deref(), Some(""));
    h.get_by_label("Blend mode").focus();
    h.run();
    for key in [Key::ArrowDown, Key::End, Key::ArrowUp, Key::Home] {
        press(&mut h, key);
    }
    assert_eq!(h.state().selected, 0);
    assert_eq!(h.state().changes, 0);
}

#[derive(Default)]
struct Rows {
    room: f32,
    rects: Vec<Rect>,
    clicks: Vec<&'static str>,
    disabled: bool,
}

fn rows() -> Harness<'static, Rows> {
    let mut h = Harness::builder()
        .with_size(vec2(300.0, 200.0))
        .build_ui_state(
            |ui, s: &mut Rows| {
                s.room = ui.available_width();
                s.rects.clear();
                let rows = [
                    ("contrast", MenuRow::new("Show Contrast").note("hidden")),
                    ("all", MenuRow::new("Show all")),
                    ("tall", MenuRow::new("Tall").height(40.0)),
                ];
                for (key, row) in rows {
                    let r = ui.add_enabled(!s.disabled, row);
                    s.rects.push(r.rect);
                    if r.clicked() {
                        s.clicks.push(key);
                    }
                }
            },
            Rows::default(),
        );
    theme::install(&h.ctx, &ThemeSpec::new("pop-dark"));
    h.run();
    h
}

#[test]
fn a_menu_row_is_a_button_named_by_its_text_then_its_note() {
    let h = rows();
    for name in ["Show Contrast, hidden", "Show all", "Tall"] {
        assert_eq!(h.get_by_label(name).accesskit_node().role(), Role::Button);
    }
}

#[test]
fn a_menu_row_fills_its_room_at_its_height() {
    let h = rows();
    let s = h.state();
    let row_h = h.ctx.global_style().spacing.interact_size.y;
    for r in &s.rects {
        assert!((r.width() - s.room).abs() < 0.5, "{r:?} in {}", s.room);
    }
    assert!(
        s.rects[0].height() >= row_h,
        "at least a row: {:?}",
        s.rects[0]
    );
    assert_eq!(
        s.rects[0].height(),
        s.rects[1].height(),
        "one default height"
    );
    assert_eq!(s.rects[2].height(), 40.0, "a height given is kept");
}

#[test]
fn a_menu_row_is_focused_by_tab_and_pressed_by_space_or_enter() {
    let mut h = rows();
    h.key_press(Key::Tab);
    h.run();
    assert!(h.get_by_label("Show Contrast, hidden").is_focused());
    h.key_press(Key::Space);
    h.run();
    h.key_press(Key::Tab);
    h.run();
    assert!(h.get_by_label("Show all").is_focused());
    h.key_press(Key::Enter);
    h.run();
    h.get_by_label("Tall").click();
    h.run();
    assert_eq!(h.state().clicks, ["contrast", "all", "tall"]);
}

#[test]
fn a_disabled_menu_row_says_so_and_takes_no_press() {
    let mut h = rows();
    h.state_mut().disabled = true;
    h.run();
    let node = h.get_by_label("Show all");
    assert!(node.accesskit_node().is_disabled());
    node.click();
    h.run();
    assert!(h.state().clicks.is_empty());
}
