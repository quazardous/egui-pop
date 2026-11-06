//! egui 0.37's kinetic scrolling under egui-pop's theme: a trackpad stretches a
//! scroll area past its end and a touch drag coasts, as egui does, and under
//! reduced motion neither (`theme::style_with`). A mouse wheel has neither.

use std::cell::Cell;
use std::rc::Rc;

use egui::scroll_area::{ScrollSource, State};
use egui::{Event, Id, MouseWheelSource, MouseWheelUnit, Pos2, ScrollArea, TouchPhase, pos2, vec2};
use egui_kittest::Harness;
use egui_pop_core::theme::{self, ThemeSpec};
use egui_pop_core::ui::UiPop as _;

const OVER: Pos2 = pos2(100.0, 100.0);

/// A tall list in a scroll area filling the window, dragged by its content too
/// (as on a touch screen); where to read its state.
fn list(spec: &ThemeSpec) -> (Harness<'static>, Rc<Cell<Id>>) {
    let id = Rc::new(Cell::new(Id::NULL));
    let out = id.clone();
    // Frames at 60 a second: egui reads the pointer's speed over its last
    // tenth of a second.
    let h = Harness::builder()
        .with_size(vec2(300.0, 300.0))
        .with_step_dt(1.0 / 60.0)
        .with_max_steps(120)
        .build_ui(move |ui| {
            let area = ScrollArea::vertical()
                .id_salt("list")
                .auto_shrink(false)
                .scroll_source(ScrollSource::ALL);
            let drawn = ui.scroll_area(area, |ui| {
                ui.allocate_space(vec2(100.0, 3000.0));
            });
            out.set(drawn.id);
        });
    theme::install(&h.ctx, spec);
    (h, id)
}

fn state(h: &Harness<'_>, id: &Rc<Cell<Id>>) -> State {
    State::load(&h.ctx, id.get()).expect("the area was drawn")
}

/// Fingers on a trackpad pushing the list down past its start, and held there.
fn pull_past_the_start(h: &mut Harness<'_>, source: MouseWheelSource) {
    let event = |delta, phase| Event::MouseWheel {
        unit: MouseWheelUnit::Point,
        delta,
        phase,
        source,
        modifiers: egui::Modifiers::NONE,
    };
    h.hover_at(OVER);
    h.run();
    h.event(event(egui::Vec2::ZERO, TouchPhase::Start));
    h.run();
    for _ in 0..4 {
        h.event(event(vec2(0.0, 40.0), TouchPhase::Move));
        h.step();
    }
}

#[test]
fn a_trackpad_stretches_the_list_past_its_start_as_egui_does() {
    let (mut h, id) = list(&ThemeSpec::new("pop-dark"));
    h.run();
    pull_past_the_start(&mut h, MouseWheelSource::Trackpad);
    assert!(
        state(&h, &id).overscroll().y < 0.0,
        "{:?}",
        state(&h, &id).overscroll()
    );
}

#[test]
fn under_reduced_motion_a_trackpad_stops_at_the_start() {
    let (mut h, id) = list(&ThemeSpec::new("pop-dark").reduced_motion(true));
    h.run();
    pull_past_the_start(&mut h, MouseWheelSource::Trackpad);
    assert_eq!(state(&h, &id).overscroll(), egui::Vec2::ZERO);
}

#[test]
fn a_mouse_wheel_never_stretches_the_list() {
    let (mut h, id) = list(&ThemeSpec::new("pop-dark"));
    h.run();
    pull_past_the_start(&mut h, MouseWheelSource::Wheel);
    assert_eq!(state(&h, &id).overscroll(), egui::Vec2::ZERO);
}

/// A quick drag up the list's content, then released: how far the list went on
/// after the release.
fn coast(spec: &ThemeSpec) -> f32 {
    let (mut h, id) = list(spec);
    h.run();
    h.event(Event::PointerMoved(OVER));
    h.event(Event::PointerButton {
        pos: OVER,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    h.step();
    let mut at = OVER;
    for _ in 0..6 {
        at.y -= 15.0;
        h.event(Event::PointerMoved(at));
        h.step();
    }
    h.event(Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.step();
    let released = state(&h, &id).clamped_offset().y;
    assert!(released > 0.0, "the drag moved the list: {released}");
    for _ in 0..60 {
        h.step();
    }
    state(&h, &id).clamped_offset().y - released
}

#[test]
fn a_touch_drag_coasts_after_the_release_as_egui_does() {
    let went_on = coast(&ThemeSpec::new("pop-dark"));
    assert!(went_on > 1.0, "{went_on}");
}

#[test]
fn under_reduced_motion_a_touch_drag_stops_at_the_release() {
    let went_on = coast(&ThemeSpec::new("pop-dark").reduced_motion(true));
    assert_eq!(went_on, 0.0);
}

/// The theme gives egui's kinetic style back once motion is on again.
#[test]
fn motion_back_on_brings_the_kinetic_style_back() {
    let ctx = egui::Context::default();
    theme::install(&ctx, &ThemeSpec::new("pop-dark").reduced_motion(true));
    assert!(!ctx.global_style().spacing.scroll.kinetic.rubber_band);
    theme::install(&ctx, &ThemeSpec::new("pop-dark"));
    assert_eq!(
        ctx.global_style().spacing.scroll.kinetic,
        egui::style::KineticScrollStyle::default()
    );
}
