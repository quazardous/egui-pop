//! The zoom control and the navigator share a viewport's state: their
//! changes go through the tool's hook and the viewport's `before_view`.

use egui::{Event, Modifiers, PointerButton, Pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_viewport::change::{Cause, Change, Verdict};
use egui_pop_viewport::navigator::{Navigator, ZoomControl};
use egui_pop_viewport::{View, Viewport, ViewportState, Zoom};

#[derive(Debug, Default)]
struct Scene {
    state: ViewportState,
    view: View,
    veto: bool,
    done: Vec<Cause>,
}

fn harness() -> Harness<'static, Scene> {
    let mut h = Harness::builder()
        .with_size(vec2(420.0, 520.0))
        .build_ui_state(
            |ui, s: &mut Scene| {
                Viewport::new("v")
                    .image_size([1200, 800])
                    .size(vec2(400.0, 260.0))
                    .show(ui, &mut s.state, &mut s.view);
                let veto = s.veto;
                let hook = move |c: &Change<View>| {
                    if veto && c.done {
                        Verdict::Veto
                    } else {
                        Verdict::Allow
                    }
                };
                let a = ZoomControl::new(&mut s.state, &mut s.view)
                    .on_change(hook)
                    .show(ui);
                let b = Navigator::new(&mut s.state, &mut s.view)
                    .size(vec2(240.0, 160.0))
                    .on_change(hook)
                    .show(ui);
                s.done.extend(
                    a.changes
                        .iter()
                        .chain(&b.changes)
                        .filter(|c| c.done)
                        .map(|c| c.cause),
                );
            },
            Scene::default(),
        );
    h.run();
    h
}

fn zoom(h: &Harness<'_, Scene>) -> f64 {
    match h.state().view.zoom {
        Zoom::Px(z) => z,
        Zoom::Fit => h.state().state.mapping().map_or(0.0, |m| m.zoom),
    }
}

#[test]
fn the_zoom_control_steps_the_ladder_and_fits() {
    let mut h = harness();
    let fit = zoom(&h);
    h.get_by_label("Zoom in").click();
    h.run();
    assert!(zoom(&h) > fit, "{} > {fit}", zoom(&h));
    assert!(!h.state().view.is_fit());
    h.get_by_label("100 %").click();
    h.run();
    assert_eq!(h.state().view.zoom, Zoom::Px(1.0));
    h.get_by_label("Fit").click();
    h.run();
    assert!(h.state().view.is_fit());
    assert_eq!(h.state().done, [Cause::Command; 3]);
    egui_pop_test::assert_all_named(&h);
}

fn drag(h: &mut Harness<'_, Scene>, from: Pos2, d: egui::Vec2) {
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
fn the_navigators_frame_pans_and_a_veto_puts_the_view_back() {
    let mut h = harness();
    h.get_by_label("100 %").click();
    h.run();
    let before = h.state().view.center;
    let nav = h.get_by_label("Navigator");
    assert_eq!(
        nav.accesskit_node().role(),
        egui::accesskit::Role::ScrollView
    );
    let rect = nav.rect();
    // The image is 234 points wide in the navigator for 1200 pixels:
    // about 5 pixels a point.
    drag(&mut h, rect.center(), vec2(40.0, 0.0));
    let after = h.state().view.center;
    assert!(
        (after[0] - before[0] - 205.0).abs() < 15.0,
        "{before:?} {after:?}"
    );
    assert_eq!(h.state().done.last(), Some(&Cause::Drag));

    h.state_mut().veto = true;
    let rect = h.get_by_label("Navigator").rect();
    drag(&mut h, rect.center(), vec2(-40.0, 0.0));
    assert_eq!(h.state().view.center, after, "vetoed at the end");
}
