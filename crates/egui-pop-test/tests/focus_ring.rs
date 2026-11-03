//! A focus you can see: the focus plugin rings what the keyboard focused, not
//! what a click did, and leaves alone a widget showing its own focus.

use egui::{Id, Rect, Sense, Ui, vec2};
use egui_kittest::Harness;
use egui_pop_core::focus;
use egui_pop_core::theme::{ThemeSpec, Tokens};
use egui_pop_test::themed;
use egui_pop_test::walk::{self, TAB};

const SIZE: egui::Vec2 = vec2(240.0, 120.0);

fn page<'a>(ui: impl FnMut(&mut Ui) + 'a) -> Harness<'a> {
    let mut h = themed(&ThemeSpec::default(), SIZE, ui);
    focus::install(&h.ctx);
    let _ = h.run_ok();
    h
}

/// How many outlines of the `focus.ring` colour the last pass painted on a
/// rectangle: a focused button's own look may be one.
fn rings(h: &Harness<'_>, rect: Rect) -> usize {
    let ring = Tokens::of(&h.ctx).focus_ring;
    h.output()
        .shapes
        .iter()
        .filter(|c| match &c.shape {
            egui::Shape::Rect(r) => {
                r.stroke.color == ring && r.stroke.width > 0.0 && r.rect == rect
            }
            _ => false,
        })
        .count()
}

/// A press and a release of the primary button at `pos`.
fn click(h: &mut Harness<'_>, pos: egui::Pos2) {
    h.event(egui::Event::PointerMoved(pos));
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        let _ = h.run_ok();
    }
}

fn buttons(ui: &mut Ui) {
    let _ = ui.button("First");
    let _ = ui.button("Second");
}

#[test]
fn tab_rings_a_plain_button() {
    let mut h = page(buttons);
    let stop = walk::press(&mut h, TAB).expect("a stop");
    assert_eq!(stop.name, "First");
    assert!(focus::ring_shown(&h.ctx));
    // A focused button's own look may be of the same colour: one more than
    // without the plugin.
    let mut plain = themed(&ThemeSpec::default(), SIZE, buttons);
    let _ = plain.run_ok();
    let bare = walk::press(&mut plain, TAB).expect("a stop");
    assert_eq!(
        rings(&h, stop.rect),
        rings(&plain, bare.rect) + 1,
        "Tab shows a ring"
    );
}

/// Two boxes taking the focus on a click, as a canvas does.
fn boxes(ui: &mut Ui) {
    for name in ["One", "Two"] {
        let r = ui.allocate_response(vec2(60.0, 30.0), Sense::click());
        if r.clicked() {
            r.request_focus();
        }
        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, name));
    }
}

#[test]
fn a_click_focuses_without_a_ring_and_the_keyboard_brings_it_back() {
    let mut h = page(boxes);
    let one = walk::press(&mut h, TAB).expect("a stop");
    assert_eq!(rings(&h, one.rect), 1, "Tab shows a ring");
    let two = walk::press(&mut h, TAB).expect("a stop");
    click(&mut h, one.rect.center());
    assert_eq!(h.ctx.memory(|m| m.focused()), Some(one.id));
    assert!(!focus::ring_shown(&h.ctx));
    assert_eq!(rings(&h, one.rect), 0, "a click shows none");
    // The keyboard again: the ring is back.
    let next = walk::press(&mut h, TAB).expect("a stop");
    assert_eq!(next.id, two.id);
    assert_eq!(rings(&h, two.rect), 1);
}

#[test]
fn a_widget_showing_its_own_focus_gets_no_ring() {
    let mut h = page(|ui| {
        let r = ui.allocate_response(vec2(60.0, 30.0), Sense::click());
        if r.has_focus() {
            focus::own_ring(ui.ctx(), r.id);
        }
        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Own"));
    });
    let stop = walk::press(&mut h, TAB).expect("a stop");
    assert_eq!(stop.name, "Own");
    assert_eq!(rings(&h, stop.rect), 0);
}

#[test]
fn a_text_field_shows_its_focus_itself() {
    let mut text = String::new();
    let mut h = page(move |ui| {
        let r = ui.add(egui::TextEdit::singleline(&mut text).id(Id::unique("field")));
        egui_pop_core::a11y::name(&r, egui::Role::TextInput, "Field");
    });
    let stop = walk::press(&mut h, TAB).expect("a stop");
    assert_eq!(stop.id, Id::unique("field"));
    let ring = Tokens::of(&h.ctx).focus_ring;
    // egui's own outline for a focused field is the selection stroke; the
    // plugin adds nothing of the ring's colour unless that is the same colour.
    let selection = h.ctx.global_style().visuals.selection.stroke.color;
    if selection != ring {
        assert_eq!(rings(&h, stop.rect), 0);
    }
}
