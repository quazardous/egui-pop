//! Pointer profiles on the image viewport and the node graph: the wheel
//! in lines, a scroll in points, a pinch, each modifier, the middle
//! button, clicks and double clicks, as each profile binds them; and the
//! hint for the keys held.

use egui::{Event, Modifiers, MouseWheelUnit, PointerButton, Pos2, Vec2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_core::model::pointer::{self, PointerMap};
use egui_pop_viewport::graph::{Edge, GraphEditor, GraphModel, GraphState, Node};
use egui_pop_viewport::{Mapping, Signal, View, Viewport, ViewportState, Zoom};

fn use_profile(ctx: &egui::Context, id: &str) {
    let p = pointer::profile(id).unwrap_or_else(|| panic!("no profile `{id}`"));
    egui_pop_core::pointer::set_map(ctx, PointerMap::from_profile(p));
}

#[derive(Default)]
struct Scene {
    state: ViewportState,
    view: View,
    signals: Vec<Signal>,
    hint: String,
}

fn viewport(profile: &str, view: View) -> Harness<'static, Scene> {
    viewport_with(profile, view, ViewportState::default())
}

fn viewport_with(profile: &str, view: View, state: ViewportState) -> Harness<'static, Scene> {
    let mut h = Harness::builder()
        .with_size(vec2(400.0, 300.0))
        .build_ui_state(
            |ui, s: &mut Scene| {
                let out =
                    Viewport::new("v")
                        .image_size([1000, 800])
                        .show(ui, &mut s.state, &mut s.view);
                s.signals.extend(out.signals);
                s.hint = out.hint;
            },
            Scene {
                state,
                view,
                ..Scene::default()
            },
        );
    use_profile(&h.ctx, profile);
    h.run();
    h
}

fn centre(h: &Harness<'_, Scene>) -> Pos2 {
    h.get_by_label("Image viewport").rect().center()
}

fn wheel<S>(h: &mut Harness<'_, S>, unit: MouseWheelUnit, delta: Vec2, modifiers: Modifiers) {
    h.input_mut().events.push(Event::MouseWheel {
        unit,
        delta,
        modifiers,
        phase: egui::TouchPhase::Move,
    });
    h.run();
}

fn line<S>(h: &mut Harness<'_, S>, dy: f32, modifiers: Modifiers) {
    wheel(h, MouseWheelUnit::Line, vec2(0.0, dy), modifiers);
}

/// Sixty notches at once; the view settles in the frame they come and
/// does not move while egui's smoothing of them runs out.
fn burst(h: &mut Harness<'_, Scene>, modifiers: Modifiers) {
    h.input_mut().events.push(Event::MouseWheel {
        unit: MouseWheelUnit::Line,
        delta: vec2(0.0, 60.0),
        modifiers,
        phase: egui::TouchPhase::Move,
    });
    h.step();
    let settled = h.state().view;
    h.run_steps(30);
    assert_eq!(h.state().view, settled, "the view moved after the burst");
}

fn zoom_of(view: View) -> f64 {
    match view.zoom {
        Zoom::Px(z) => z,
        Zoom::Fit => f64::NAN,
    }
}

fn click<S>(h: &mut Harness<'_, S>, at: Pos2, button: PointerButton) {
    for pressed in [true, false] {
        h.input_mut().events.push(Event::PointerButton {
            pos: at,
            button,
            pressed,
            modifiers: Modifiers::NONE,
        });
        h.step();
    }
    h.run();
}

fn drag<S>(h: &mut Harness<'_, S>, from: Pos2, by: Vec2, button: PointerButton) {
    h.hover_at(from);
    h.input_mut().events.push(Event::PointerButton {
        pos: from,
        button,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.step();
    for i in 1..=4 {
        h.hover_at(from + by * (i as f32 / 4.0));
        h.step();
    }
    h.input_mut().events.push(Event::PointerButton {
        pos: from + by,
        button,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run();
}

#[test]
fn document_pans_with_the_wheel_and_zooms_with_ctrl() {
    let start = View::at(0.5, [500.0, 400.0]);
    let mut h = viewport("document", start);
    let c = centre(&h);
    h.hover_at(c);
    line(&mut h, -2.0, Modifiers::NONE);
    let v = h.state().view;
    assert_eq!(zoom_of(v), 0.5, "the wheel pans");
    assert_ne!(v.center, start.center);
    line(&mut h, 2.0, Modifiers::COMMAND);
    assert!(zoom_of(h.state().view) > 0.5, "Ctrl and the wheel zoom");
    // Shift: sideways only.
    let before = h.state().view;
    line(&mut h, -2.0, Modifiers::SHIFT);
    let after = h.state().view;
    assert_ne!(after.center[0], before.center[0]);
    assert_eq!(after.center[1], before.center[1]);
}

#[test]
fn darktable_zooms_with_the_wheel_and_ctrl_lifts_the_stops() {
    let mut h = viewport("darktable", View::at(0.5, [500.0, 400.0]));
    let c = centre(&h);
    h.hover_at(c);
    // A long notch stream crossing 100 % stops there. egui smooths such
    // a burst for its scroll areas over the next frames and repaints
    // while it does (`InputState::wants_repaint_after`): the viewport
    // takes the wheel and must stay put meanwhile.
    burst(&mut h, Modifiers::NONE);
    assert_eq!(h.state().view.zoom, Zoom::Px(1.0));
    // Ctrl: no stop.
    let mut h = viewport("darktable", View::at(0.5, [500.0, 400.0]));
    h.hover_at(c);
    burst(&mut h, Modifiers::COMMAND);
    assert!(zoom_of(h.state().view) > 1.0);
}

#[test]
fn blender_pans_up_and_down_with_shift_and_sideways_with_ctrl() {
    let start = View::at(1.0, [500.0, 400.0]);
    let mut h = viewport("blender", start);
    let c = centre(&h);
    h.hover_at(c);
    line(&mut h, -2.0, Modifiers::SHIFT);
    let v = h.state().view;
    assert_eq!(v.center[0], start.center[0]);
    assert_ne!(v.center[1], start.center[1]);
    line(&mut h, -2.0, Modifiers::COMMAND);
    let w = h.state().view;
    assert_ne!(w.center[0], v.center[0]);
    assert_eq!(zoom_of(w), 1.0);
    line(&mut h, 2.0, Modifiers::NONE);
    assert!(zoom_of(h.state().view) > 1.0, "the wheel zooms");
}

#[test]
fn a_scroll_in_points_pans_and_a_pinch_zooms_in_every_profile() {
    for id in pointer::ids() {
        let start = View::at(1.0, [500.0, 400.0]);
        let mut h = viewport(id, start);
        let c = centre(&h);
        h.hover_at(c);
        wheel(
            &mut h,
            MouseWheelUnit::Point,
            vec2(0.0, -30.0),
            Modifiers::NONE,
        );
        let v = h.state().view;
        assert_eq!(zoom_of(v), 1.0, "{id}: two fingers pan");
        assert_ne!(v.center, start.center, "{id}");
        h.input_mut().events.push(Event::Zoom(1.5));
        h.run();
        assert!(zoom_of(h.state().view) > 1.0, "{id}: a pinch zooms");
    }
}

#[test]
fn the_trackpad_profile_never_zooms_with_a_plain_wheel() {
    let mut h = viewport("trackpad", View::at(1.0, [500.0, 400.0]));
    let c = centre(&h);
    h.hover_at(c);
    line(&mut h, 3.0, Modifiers::NONE);
    assert_eq!(zoom_of(h.state().view), 1.0);
    wheel(
        &mut h,
        MouseWheelUnit::Point,
        vec2(0.0, 40.0),
        Modifiers::COMMAND,
    );
    assert!(zoom_of(h.state().view) > 1.0, "Ctrl and two fingers zoom");
}

#[test]
fn the_middle_button_pans_and_darktables_middle_click_cycles_the_zoom() {
    let start = View::at(1.0, [500.0, 400.0]);
    let mut h = viewport("document", start);
    let c = centre(&h);
    drag(&mut h, c, vec2(30.0, 0.0), PointerButton::Middle);
    assert_eq!(h.state().view.center[0], start.center[0] - 30.0);
    let mut h = viewport("darktable", View::fit());
    let c = centre(&h);
    h.hover_at(c);
    click(&mut h, c, PointerButton::Middle);
    assert_eq!(h.state().view.zoom, Zoom::Px(1.0));
    click(&mut h, c, PointerButton::Middle);
    assert_eq!(h.state().view.zoom, Zoom::Px(2.0));
    click(&mut h, c, PointerButton::Middle);
    assert!(h.state().view.is_fit());
}

#[test]
fn a_double_click_fits_or_asks_to_leave_as_the_profile_says() {
    let mut h = viewport("document", View::at(3.0, [10.0, 10.0]));
    let c = centre(&h);
    egui_pop_test::double_click_at(&mut h, c);
    h.run();
    assert!(h.state().view.is_fit());
    assert!(!h.state().signals.contains(&Signal::Leave));
    let start = View::at(3.0, [10.0, 10.0]);
    let mut h = viewport("darktable", start);
    egui_pop_test::double_click_at(&mut h, c);
    h.run();
    assert_eq!(h.state().view, start);
    assert!(h.state().signals.contains(&Signal::Leave));
}

/// A state whose double-click hook passes the click on to the default
/// action (`pass`) or handles it itself, zooming to 200 %.
fn with_double_click_hook(pass: bool) -> ViewportState {
    let mut state = ViewportState::default();
    state.hooks_mut().on_double_click = Some(Box::new(
        move |at: Pos2, m: &Mapping, default: &mut dyn FnMut() -> Option<View>| {
            if pass {
                default()
            } else {
                Some(m.zoom_around(at, 2.0))
            }
        },
    ));
    state
}

#[test]
fn a_double_click_hook_that_passes_the_click_on_still_leaves_the_view() {
    let start = View::at(3.0, [10.0, 10.0]);
    let mut h = viewport_with("darktable", start, with_double_click_hook(true));
    let c = centre(&h);
    egui_pop_test::double_click_at(&mut h, c);
    h.run();
    assert_eq!(h.state().view, start);
    assert_eq!(
        h.state()
            .signals
            .iter()
            .filter(|s| **s == Signal::Leave)
            .count(),
        1,
        "one leave signal: {:?}",
        h.state().signals
    );
}

#[test]
fn a_double_click_hook_that_passes_the_click_on_still_fits() {
    let mut h = viewport_with(
        "document",
        View::at(3.0, [10.0, 10.0]),
        with_double_click_hook(true),
    );
    let c = centre(&h);
    egui_pop_test::double_click_at(&mut h, c);
    h.run();
    assert!(h.state().view.is_fit());
    assert!(!h.state().signals.contains(&Signal::Leave));
}

#[test]
fn a_double_click_hook_that_handles_the_click_does_not_leave_the_view() {
    let mut h = viewport_with("darktable", View::fit(), with_double_click_hook(false));
    let c = centre(&h);
    egui_pop_test::double_click_at(&mut h, c);
    h.run();
    assert_eq!(h.state().view.zoom, Zoom::Px(2.0));
    assert!(!h.state().signals.contains(&Signal::Leave));
}

#[test]
fn adobes_click_toggles_fit_and_100_percent() {
    let mut h = viewport("adobe", View::fit());
    let c = centre(&h);
    h.hover_at(c);
    click(&mut h, c, PointerButton::Primary);
    assert_eq!(h.state().view.zoom, Zoom::Px(1.0));
}

#[test]
fn the_hint_says_what_the_keys_held_change() {
    let mut h = viewport("document", View::at(1.0, [500.0, 400.0]));
    let c = centre(&h);
    h.hover_at(c);
    h.run();
    assert!(h.state().hint.contains("Wheel: pan"), "{}", h.state().hint);
    assert_eq!(
        egui_pop_core::pointer::hint_now(&h.ctx).as_deref(),
        Some(h.state().hint.as_str())
    );
    h.event(Event::ModifiersChanged(Modifiers::COMMAND));
    h.run();
    assert!(h.state().hint.contains("Wheel: zoom"), "{}", h.state().hint);
}

#[derive(Debug, Default)]
struct Graph {
    nodes: Vec<Node>,
}

impl GraphModel for Graph {
    fn nodes(&self) -> Vec<Node> {
        self.nodes.clone()
    }

    fn edges(&self) -> Vec<Edge> {
        Vec::new()
    }
}

struct GraphScene {
    graph: Graph,
    view: GraphState,
}

fn graph(profile: &str) -> Harness<'static, GraphScene> {
    let mut h = Harness::builder()
        .with_size(vec2(720.0, 420.0))
        .build_ui_state(
            |ui, s: &mut GraphScene| {
                GraphEditor::new(&s.graph, &mut s.view)
                    .size(vec2(700.0, 400.0))
                    .show(ui);
            },
            GraphScene {
                graph: Graph {
                    nodes: vec![
                        Node::new("a", "a", "A", [0.0, 0.0]),
                        Node::new("b", "b", "B", [400.0, 200.0]),
                    ],
                },
                view: GraphState::new(),
            },
        );
    use_profile(&h.ctx, profile);
    h.run();
    h
}

/// A point on the canvas away from the nodes.
fn empty_spot(h: &Harness<'_, GraphScene>) -> Pos2 {
    let r = h.get_by_label("Node graph").rect();
    r.left_bottom() + vec2(20.0, -20.0)
}

#[test]
fn the_graph_pans_or_zooms_with_the_wheel_as_the_profile_says() {
    let mut h = graph("document");
    let at = empty_spot(&h);
    h.hover_at(at);
    let (zoom, view) = (h.state().view.zoom(), h.state().view.view);
    line(&mut h, -2.0, Modifiers::NONE);
    assert_eq!(h.state().view.zoom(), zoom, "Document: the wheel pans");
    assert_ne!(h.state().view.view, view);
    line(&mut h, 2.0, Modifiers::COMMAND);
    assert!(h.state().view.zoom() > zoom, "Ctrl and the wheel zoom");
    let mut h = graph("darktable");
    h.hover_at(at);
    let zoom = h.state().view.zoom();
    line(&mut h, 2.0, Modifiers::NONE);
    assert!(h.state().view.zoom() > zoom, "darktable: the wheel zooms");
}

#[test]
fn only_blender_pans_the_graph_with_a_secondary_drag() {
    for (id, pans) in [("document", false), ("blender", true)] {
        let mut h = graph(id);
        let at = empty_spot(&h);
        let view = h.state().view.view;
        drag(&mut h, at, vec2(40.0, 0.0), PointerButton::Secondary);
        h.key_press(egui::Key::Escape);
        h.run();
        assert_eq!(h.state().view.view != view, pans, "{id}");
    }
}

/// The graph of [`graph`], with a widget over `claim` claiming the
/// secondary drag (none: no claim).
fn graph_under_claim(profile: &str, claim: Option<egui::Rect>) -> Harness<'static, GraphScene> {
    use egui_pop_core::pointer::claim::{self, SecondaryClaim};
    let mut h = Harness::builder()
        .with_size(vec2(720.0, 420.0))
        .build_ui_state(
            move |ui, s: &mut GraphScene| {
                GraphEditor::new(&s.graph, &mut s.view)
                    .size(vec2(700.0, 400.0))
                    .show(ui);
                if let Some(rect) = claim {
                    claim::claim_secondary_rect(
                        ui.ctx(),
                        egui::Id::new("pad"),
                        ui.layer_id(),
                        rect,
                        SecondaryClaim::new("Pad").drag("Draws a stroke"),
                    );
                }
            },
            GraphScene {
                graph: Graph {
                    nodes: vec![
                        Node::new("a", "a", "A", [0.0, 0.0]),
                        Node::new("b", "b", "B", [400.0, 200.0]),
                    ],
                },
                view: GraphState::new(),
            },
        );
    use_profile(&h.ctx, profile);
    h.run();
    h
}

#[test]
fn a_secondary_drag_claimed_by_a_widget_does_not_pan_the_graph() {
    let at = empty_spot(&graph("blender"));
    let pad = egui::Rect::from_center_size(at, vec2(30.0, 30.0));
    for (claim, pans) in [(None, true), (Some(pad), false)] {
        let mut h = graph_under_claim("blender", claim);
        h.run();
        let view = h.state().view.view;
        drag(&mut h, at, vec2(40.0, 0.0), PointerButton::Secondary);
        h.key_press(egui::Key::Escape);
        h.run();
        assert_eq!(
            h.state().view.view != view,
            pans,
            "claimed: {}",
            claim.is_some()
        );
    }
    // A claim elsewhere leaves the profile's binding alone.
    let away = egui::Rect::from_center_size(at + vec2(200.0, -200.0), vec2(30.0, 30.0));
    let mut h = graph_under_claim("blender", Some(away));
    let view = h.state().view.view;
    drag(&mut h, at, vec2(40.0, 0.0), PointerButton::Secondary);
    h.run();
    assert_ne!(h.state().view.view, view, "a claim elsewhere");
}
