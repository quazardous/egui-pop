//! `gestures`' contract (docs/viewport.md, docs/pointer-gestures.md):
//! the view's gesture tables read from a pointer profile, the plain
//! double click from the profile's option, the default keys, and the
//! gestures of a context (the profile's, or those set over it). No
//! frame is run: the contexts are bare.

use egui::{Context, Key, Modifiers};
use egui_pop_core::model::keymap::{Button, Pointer};
use egui_pop_core::model::pointer::{
    self as model, ImageDoubleClick, PointerMap, SCOPE_GRAPH, SCOPE_IMAGE, gesture,
};
use egui_pop_core::pointer::{self as pointer_map, WheelInput};
use egui_pop_viewport::gestures::{
    Gestures, ViewAction, ViewGestures, gestures, graph_gestures, hint, set_gestures,
};

fn every_profile() -> Vec<PointerMap> {
    model::builtin()
        .iter()
        .map(PointerMap::from_profile)
        .collect()
}

#[test]
fn each_gesture_command_names_its_action_and_others_name_nothing() {
    use ViewAction::*;
    for (command, action) in [
        (gesture::IMAGE_ZOOM, Zoom),
        (gesture::IMAGE_ZOOM_FREE, ZoomFree),
        (gesture::IMAGE_PAN, Pan),
        (gesture::IMAGE_PAN_X, PanX),
        (gesture::IMAGE_PAN_Y, PanY),
        (gesture::IMAGE_ZOOM_CYCLE, ZoomCycle),
        (gesture::IMAGE_FIT, Fit),
        (gesture::IMAGE_ZOOM_TOGGLE, ZoomToggle),
        (gesture::GRAPH_ZOOM, Zoom),
        (gesture::GRAPH_PAN, Pan),
        (gesture::GRAPH_PAN_X, PanX),
        (gesture::GRAPH_PAN_Y, PanY),
        (gesture::GRAPH_FIT, Fit),
        (gesture::VALUE_STEP, Nothing),
    ] {
        assert_eq!(ViewAction::of(&command), action, "{command}");
    }
    let zooms: Vec<_> = [
        Nothing, Zoom, ZoomFree, Pan, PanX, PanY, ZoomCycle, Fit, ZoomToggle, Leave,
    ]
    .into_iter()
    .filter(|a| a.zooms())
    .collect();
    assert_eq!(zooms, [Zoom, ZoomFree], "a zoom step or a fit is no zoom");
    let pans: Vec<_> = [
        Nothing, Zoom, ZoomFree, Pan, PanX, PanY, ZoomCycle, Fit, ZoomToggle, Leave,
    ]
    .into_iter()
    .filter(|a| a.pans())
    .collect();
    assert_eq!(pans, [Pan, PanX, PanY]);
    assert_eq!(ViewAction::default(), Nothing);
}

/// What a table says for one binding's chord.
fn looked_up(g: &ViewGestures, chord: &egui_pop_core::model::keymap::Chord) -> ViewAction {
    let modifiers = pointer_map::modifiers_of(chord);
    let input = |pointer| WheelInput {
        pointer,
        modifiers,
        delta: egui::Vec2::ZERO,
        zoom: 1.0,
    };
    match chord.pointer().expect("a pointer chord") {
        p @ (Pointer::Wheel | Pointer::Scroll | Pointer::Pinch) => g.wheel_action(&input(p)),
        Pointer::Click(b) => g.click_action(pointer_map::egui_button(b), modifiers),
        Pointer::DoubleClick(b) => g.double_click_action(pointer_map::egui_button(b), modifiers),
        Pointer::Drag(b) => {
            if g.drag_pans(pointer_map::egui_button(b), modifiers) {
                ViewAction::Pan
            } else {
                ViewAction::Nothing
            }
        }
    }
}

#[test]
fn the_tables_say_what_each_profiles_bindings_say() {
    for map in every_profile() {
        for scope in [SCOPE_IMAGE, SCOPE_GRAPH] {
            let g = ViewGestures::from_map(&map, scope);
            for b in map.scope(scope) {
                if b.chord.pointer().is_none() {
                    continue;
                }
                let mut want = ViewAction::of(&b.command);
                if matches!(b.chord.pointer(), Some(Pointer::Drag(_))) && want != ViewAction::Pan {
                    // A drag only pans or does nothing.
                    want = ViewAction::Nothing;
                }
                assert_eq!(
                    looked_up(&g, &b.chord),
                    want,
                    "{} {scope} {:?} {}",
                    map.profile,
                    b.chord,
                    b.command
                );
            }
        }
        // The graph scope never gets an image-only action.
        let g = ViewGestures::from_map(&map, SCOPE_GRAPH);
        let all = g
            .wheel
            .iter()
            .chain(&g.scroll)
            .chain(&g.pinch)
            .chain(g.click.iter().flatten())
            .chain(g.double_click.iter().flatten());
        for a in all {
            assert!(
                !matches!(
                    a,
                    ViewAction::ZoomFree
                        | ViewAction::ZoomCycle
                        | ViewAction::ZoomToggle
                        | ViewAction::Leave
                ),
                "{}: {a:?}",
                map.profile
            );
        }
    }
    // A scope nothing binds: nothing at all.
    let empty = ViewGestures::from_map(&PointerMap::default(), "canvas.none");
    assert_eq!(empty, ViewGestures::default());
}

#[test]
fn a_plain_double_click_the_scope_leaves_unbound_follows_the_profiles_option() {
    let primary_bound = |map: &PointerMap| {
        map.scope(SCOPE_IMAGE).any(|b| {
            b.chord.pointer() == Some(Pointer::DoubleClick(Button::Primary))
                && pointer_map::modifiers_of(&b.chord) == Modifiers::NONE
        })
    };
    for option in [
        ImageDoubleClick::Fit,
        ImageDoubleClick::Leave,
        ImageDoubleClick::Nothing,
    ] {
        for mut map in every_profile() {
            map.options.image_double_click = option;
            let g = Gestures::from_map(&map);
            let action = g
                .pointer
                .double_click_action(egui::PointerButton::Primary, Modifiers::NONE);
            if primary_bound(&map) {
                // The scope's own binding wins over the option.
                assert_eq!(
                    action,
                    ViewGestures::from_map(&map, SCOPE_IMAGE)
                        .double_click_action(egui::PointerButton::Primary, Modifiers::NONE),
                    "{}",
                    map.profile
                );
            } else {
                let want = match option {
                    ImageDoubleClick::Fit => ViewAction::Fit,
                    ImageDoubleClick::Leave => ViewAction::Leave,
                    ImageDoubleClick::Nothing => ViewAction::Nothing,
                };
                assert_eq!(action, want, "{} {option:?}", map.profile);
            }
            // Only the plain primary double click takes the option.
            assert_eq!(
                g.pointer
                    .double_click_action(egui::PointerButton::Primary, Modifiers::SHIFT),
                ViewGestures::from_map(&map, SCOPE_IMAGE)
                    .double_click_action(egui::PointerButton::Primary, Modifiers::SHIFT),
            );
        }
    }
}

#[test]
fn the_keys_are_the_documented_ones_whatever_the_profile() {
    for map in every_profile() {
        let g = Gestures::from_map(&map);
        assert_eq!(g.pan_button, egui::PointerButton::Primary);
        assert_eq!(g.pan_key, Key::Space);
        assert_eq!(g.cancel, Key::Escape);
        assert_eq!(g.zoom_in, &[Key::Plus, Key::Equals]);
        assert_eq!(g.zoom_out, &[Key::Minus]);
        assert_eq!(
            (g.fit, g.actual, g.double),
            (Key::Num0, Key::Num1, Key::Num2)
        );
        assert_eq!(g.coarse, Modifiers::SHIFT);
        assert_eq!(g.before, Key::Backslash);
    }
    assert_eq!(
        Gestures::default(),
        Gestures::from_map(&pointer_map::default_map())
    );
}

#[test]
fn a_contexts_gestures_follow_its_profile_until_set() {
    let ctx = Context::default();
    assert_eq!(gestures(&ctx), Gestures::from_map(&pointer_map::map(&ctx)));
    assert_eq!(
        graph_gestures(&ctx),
        ViewGestures::from_map(&pointer_map::map(&ctx), SCOPE_GRAPH)
    );

    // Another profile: the gestures follow it.
    let photoshop = PointerMap::from_profile(model::profile("photoshop").unwrap());
    pointer_map::set_map(&ctx, photoshop.clone());
    assert_eq!(gestures(&ctx), Gestures::from_map(&photoshop));
    assert_eq!(
        graph_gestures(&ctx),
        ViewGestures::from_map(&photoshop, SCOPE_GRAPH)
    );

    // Set outright: they win over the profile, in this context only.
    let mut mine = Gestures::from_map(&photoshop);
    mine.pan_key = Key::H;
    mine.pointer = ViewGestures::default();
    set_gestures(&ctx, mine);
    assert_eq!(gestures(&ctx), mine);
    pointer_map::set_map(&ctx, PointerMap::default());
    assert_eq!(gestures(&ctx), mine, "a later profile does not undo them");
    // The graph's are always the profile's.
    assert_eq!(
        graph_gestures(&ctx),
        ViewGestures::from_map(&PointerMap::default(), SCOPE_GRAPH)
    );
    let other = Context::default();
    assert_eq!(gestures(&other).pan_key, Key::Space);
}

#[test]
fn the_hint_names_what_the_pointer_does_and_is_empty_for_a_scope_without_bindings() {
    let ctx = Context::default();
    let text = hint(&ctx, SCOPE_IMAGE);
    let map = pointer_map::map(&ctx);
    // One part per plain pointer binding of the scope that has a word.
    let plain = map
        .scope(SCOPE_IMAGE)
        .filter(|b| {
            b.chord.pointer().is_some()
                && pointer_map::modifiers_of(&b.chord) == Modifiers::NONE
                && ViewAction::of(&b.command) != ViewAction::Nothing
        })
        .count();
    if plain == 0 {
        assert_eq!(text, "");
    } else {
        assert_eq!(text.split(" · ").count(), plain, "{text}");
        assert!(text.contains(": "), "{text}");
    }
    assert_eq!(hint(&ctx, "canvas.none"), "");
}
