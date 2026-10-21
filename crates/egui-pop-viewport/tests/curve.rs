//! The curve editor: points added on the curve, dragged, deleted and
//! reset, the keys, the value picked in the image, and the hook.

use egui::{Event, Key, Modifiers, PointerButton, Pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_viewport::change::{Cause, Change, Verdict};
use egui_pop_viewport::curve::{CurveEditor, Pick, PickAction, identity};

#[derive(Debug, Default)]
struct State {
    points: Vec<[f32; 2]>,
    pick: Option<Pick>,
    veto: bool,
    done: Vec<Cause>,
}

fn editor() -> Harness<'static, State> {
    let mut h = Harness::builder()
        .with_size(vec2(240.0, 240.0))
        .build_ui_state(
            |ui, s: &mut State| {
                let veto = s.veto;
                let out = CurveEditor::new(&mut s.points, "Tone curve")
                    .size(vec2(200.0, 200.0))
                    .pick(s.pick.take())
                    .on_change(move |c: &Change<Vec<[f32; 2]>>| {
                        if veto && c.done {
                            Verdict::Veto
                        } else {
                            Verdict::Allow
                        }
                    })
                    .show(ui);
                s.done
                    .extend(out.changes.iter().filter(|c| c.done).map(|c| c.cause));
            },
            State {
                points: identity(),
                ..State::default()
            },
        );
    h.run();
    h
}

fn drag(h: &mut Harness<'_, State>, from: Pos2, d: egui::Vec2) {
    h.event(Event::PointerMoved(from));
    h.event(Event::PointerButton {
        pos: from,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.step();
    for i in 1..=4 {
        h.event(Event::PointerMoved(from + d * (i as f32 / 4.0)));
        h.step();
    }
    h.event(Event::PointerButton {
        pos: from + d,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run();
}

#[test]
fn a_press_on_the_curve_adds_a_point_and_a_drag_moves_it() {
    let mut h = editor();
    let node = h.get_by_label("Tone curve");
    assert_eq!(node.accesskit_node().role(), egui::accesskit::Role::Slider);
    let rect = node.rect();
    // The identity passes through the centre.
    drag(&mut h, rect.center(), vec2(0.0, -40.0));
    let pts = h.state().points.clone();
    assert_eq!(pts.len(), 3, "{pts:?}");
    assert!((pts[1][0] - 0.5).abs() < 0.02, "{pts:?}");
    assert!(pts[1][1] > 0.65, "dragged up: {pts:?}");
    assert_eq!(h.state().done, [Cause::Add]);
    assert_eq!(
        h.get_by_label("Tone curve")
            .accesskit_node()
            .value()
            .as_deref()
            .map(|v| v.starts_with("point 2 of 3")),
        Some(true)
    );
    // A double click on the point deletes it; elsewhere it resets.
    let at = h.get_by_label("Tone curve").rect();
    let p = pts[1];
    let pos = Pos2::new(
        at.left() + 6.0 + p[0] * (at.width() - 12.0),
        at.bottom() - 6.0 - p[1] * (at.height() - 12.0),
    );
    egui_pop_test::double_click_at(&mut h, pos);
    h.run();
    assert_eq!(h.state().points, identity());
    assert_eq!(h.state().done.last(), Some(&Cause::Delete));
}

#[test]
fn keys_select_move_add_delete_and_reset() {
    let mut h = editor();
    // A click away from the curve focuses the editor (a click on the
    // curve would add a point).
    let rect = h.get_by_label("Tone curve").rect();
    let pos = rect.left_top() + vec2(30.0, 30.0);
    h.event(Event::PointerMoved(pos));
    for pressed in [true, false] {
        h.event(Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        h.step();
    }
    h.run();
    assert_eq!(h.state().points, identity());
    h.key_press(Key::PageDown);
    h.run();
    h.key_press(Key::ArrowUp);
    h.run();
    assert!((h.state().points[0][1] - 0.01).abs() < 1e-5);
    h.key_press(Key::Insert);
    h.run();
    assert_eq!(h.state().points.len(), 3);
    h.key_press(Key::Delete);
    h.run();
    assert_eq!(h.state().points.len(), 2);
    h.key_press(Key::R);
    h.run();
    assert_eq!(h.state().points, identity());
    assert_eq!(
        h.state().done,
        [Cause::Key, Cause::Add, Cause::Delete, Cause::Reset]
    );
}

#[test]
fn a_value_picked_in_the_image_adds_a_point_and_the_drag_moves_it() {
    let mut h = editor();
    let pick = |action| Some(Pick { x: 0.25, action });
    h.state_mut().pick = pick(PickAction::Press);
    h.run();
    assert_eq!(h.state().points.len(), 3);
    h.state_mut().pick = pick(PickAction::Move(0.1));
    h.run();
    h.state_mut().pick = pick(PickAction::Release);
    h.run();
    let p = h.state().points[1];
    assert!(
        (p[0] - 0.25).abs() < 1e-5 && (p[1] - 0.35).abs() < 1e-3,
        "{p:?}"
    );
    assert_eq!(h.state().done, [Cause::Pick]);

    // A veto at the end puts the points back.
    let mut h = editor();
    h.state_mut().veto = true;
    h.state_mut().pick = pick(PickAction::Press);
    h.run();
    h.state_mut().pick = pick(PickAction::Move(0.1));
    h.run();
    h.state_mut().pick = pick(PickAction::Release);
    h.run();
    assert_eq!(h.state().points, identity());
}

/// A curve editor in a card with a context menu, under a plain label
/// that has none of its own.
#[derive(Debug, Default)]
struct Card {
    points: Vec<[f32; 2]>,
    deleted: usize,
}

fn card() -> Harness<'static, Card> {
    let mut h = Harness::builder()
        .with_size(vec2(260.0, 300.0))
        .build_ui_state(
            |ui, s: &mut Card| {
                let card = ui.interact(ui.max_rect(), egui::Id::new("card"), egui::Sense::click());
                ui.add(egui::Label::new("Plain").selectable(false));
                let out = CurveEditor::new(&mut s.points, "Tone curve")
                    .size(vec2(200.0, 200.0))
                    .show(ui);
                s.deleted += out
                    .changes
                    .iter()
                    .filter(|c| c.cause == Cause::Delete)
                    .count();
                egui_pop_core::pointer::claim::context_menu(&card, |ui| {
                    ui.label("Card menu");
                });
            },
            Card {
                points: vec![[0.0, 0.0], [0.5, 0.5], [1.0, 1.0]],
                deleted: 0,
            },
        );
    h.run();
    h
}

#[test]
fn the_curves_right_click_deletes_a_point_and_opens_no_menu_around_it() {
    let mut h = card();
    // On a plain label, the card's menu opens.
    h.get_by_label("Plain").click_secondary();
    h.run();
    assert!(
        h.query_by_label("Card menu").is_some(),
        "the card's menu opens"
    );
    // A click on the card, below the curve, closes it.
    let below = egui::pos2(240.0, 285.0);
    h.event(Event::PointerMoved(below));
    for pressed in [true, false] {
        h.event(Event::PointerButton {
            pos: below,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    h.run();
    assert!(h.query_by_label("Card menu").is_none());
    // On the curve's middle point: deleted, and no menu.
    let middle = h.get_by_label("Tone curve").rect().center();
    h.event(Event::PointerMoved(middle));
    h.run();
    for pressed in [true, false] {
        h.event(Event::PointerButton {
            pos: middle,
            button: PointerButton::Secondary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    h.run();
    assert_eq!(h.state().deleted, 1);
    assert_eq!(h.state().points.len(), 2);
    assert!(
        h.query_by_label("Card menu").is_none(),
        "no menu over the curve"
    );
    // The editor says what its right click does.
    let uses = egui_pop_core::pointer::claim::uses(&h.ctx);
    assert!(
        uses.iter()
            .any(|u| u.widget == "Curve editor" && u.gesture == "SecondaryClick"),
        "{uses:?}"
    );
}
