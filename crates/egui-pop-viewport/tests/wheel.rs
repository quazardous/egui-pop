//! Colour wheels: relative drags, the picker's press, keys, reset, and
//! the luminance slider going through the wheel's hook.

use egui::{Event, Key, Modifiers, PointerButton, Pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_viewport::change::{Cause, Change, Verdict};
use egui_pop_viewport::wheel::{ColorWheel, WheelValue};

#[derive(Debug, Default)]
struct State {
    value: WheelValue,
    picker: bool,
    lock_luminance: bool,
    done: Vec<Cause>,
}

fn wheel(picker: bool) -> Harness<'static, State> {
    let mut h = Harness::builder()
        .with_size(vec2(240.0, 240.0))
        .build_ui_state(
            |ui, s: &mut State| {
                let lock = s.lock_luminance;
                let hook = move |c: &Change<WheelValue>| {
                    if lock && c.to.luminance != c.from.luminance {
                        Verdict::Veto
                    } else {
                        Verdict::Allow
                    }
                };
                let w = if s.picker {
                    ColorWheel::picker(&mut s.value, "Colour")
                } else {
                    ColorWheel::offset(&mut s.value, "Lift")
                };
                let out = w.size(160.0).on_change(hook).show(ui);
                s.done
                    .extend(out.changes.iter().filter(|c| c.done).map(|c| c.cause));
            },
            State {
                picker,
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
fn a_grading_wheel_drags_relatively_steps_and_resets() {
    let mut h = wheel(false);
    let disc = h.get_by_label("Lift");
    assert_eq!(disc.accesskit_node().role(), egui::accesskit::Role::Slider);
    let rect = disc.rect();
    // Pressed off the puck, nothing jumps; the drag moves it by as much.
    drag(&mut h, rect.center() + vec2(0.0, 30.0), vec2(38.0, 0.0));
    let v = h.state().value;
    assert!((v.chroma - 0.5).abs() < 0.05, "{v:?}");
    assert!(v.hue < 5.0 || v.hue > 355.0, "to the right: {v:?}");
    assert_eq!(h.state().done, [Cause::Drag]);
    // Focused by the drag: the up arrow moves the puck up a hundredth.
    h.key_press(Key::ArrowUp);
    h.run();
    let [_, y] = h.state().value.xy();
    assert!((y - 0.01).abs() < 1e-3, "{y}");
    egui_pop_test::double_click_at(&mut h, rect.center());
    h.run();
    assert_eq!(h.state().value.chroma, 0.0);
    assert_eq!(h.state().done.last(), Some(&Cause::Reset));
}

#[test]
fn the_luminance_slider_goes_through_the_wheels_hook() {
    let mut h = wheel(false);
    h.get_by_label("Lift luminance").click();
    h.run();
    h.key_press(Key::ArrowRight);
    h.run();
    assert!((h.state().value.luminance - 0.01).abs() < 1e-6);
    assert_eq!(h.state().done, [Cause::Key]);
    h.state_mut().lock_luminance = true;
    h.key_press(Key::ArrowRight);
    h.run();
    assert!((h.state().value.luminance - 0.01).abs() < 1e-6, "vetoed");
}

#[test]
fn a_picker_takes_the_colour_under_the_pointer() {
    let mut h = wheel(true);
    let rect = h.get_by_label("Colour").rect();
    // Straight up, half way to the rim (the radius is 76).
    let pos = rect.center() - vec2(0.0, 38.0);
    h.event(Event::PointerMoved(pos));
    h.event(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.step();
    h.event(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run();
    let v = h.state().value;
    assert!(
        (v.hue - 90.0).abs() < 1.0 && (v.chroma - 0.5).abs() < 0.02,
        "{v:?}"
    );
    assert_eq!(h.state().done, [Cause::Click]);
    egui_pop_test::assert_all_named(&h);
}
