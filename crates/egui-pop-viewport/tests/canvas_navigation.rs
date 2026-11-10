//! The navigation model on the image viewport, the canvas zoom control
//! and touch screens (`docs/design/navigation.md`): hand mode, the
//! `view.*` commands and their keys per profile, the canvas in use, two
//! fingers, and a zoom control driving the viewport and the graph.

use egui::{Event, Key, Modifiers, PointerButton, Pos2, TouchDeviceId, TouchId, TouchPhase};
use egui::{Vec2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_core::canvas;
use egui_pop_core::model::pointer::{self, PointerMap};
use egui_pop_test::viewport::TestSource;
use egui_pop_viewport::graph::{Edge, GraphEditor, GraphModel, GraphState, Node};
use egui_pop_viewport::navigation::{self, CanvasZoom, NavCommand};
use egui_pop_viewport::overlay::{Overlay, Shape};
use egui_pop_viewport::{View, Viewport, ViewportState, Zoom};

fn use_profile(ctx: &egui::Context, id: &str) {
    let p = pointer::profile(id).unwrap_or_else(|| panic!("no profile `{id}`"));
    egui_pop_core::pointer::set_map(ctx, PointerMap::from_profile(p));
}

const HAND: &str = "Hand: drag to pan (H to leave)";

struct Image {
    state: ViewportState,
    view: View,
    source: TestSource,
    overlays: Vec<Overlay>,
    canvas: Option<egui::Id>,
    control: bool,
}

fn image(profile: &str, control: bool) -> Harness<'static, Image> {
    let mut state = ViewportState::new();
    state.set_settle(0.0);
    let source = TestSource::immediate(state.sink(), [1000, 800]);
    let mut h = Harness::builder()
        .with_size(vec2(420.0, 340.0))
        .build_ui_state(
            |ui, s: &mut Image| {
                if s.control
                    && let Some(id) = s.canvas
                {
                    CanvasZoom::new(id).frame_selection(false).show(ui);
                }
                let out = Viewport::new("nav")
                    .size(vec2(400.0, 280.0))
                    .source(&s.source)
                    .overlays(&s.overlays)
                    .show(ui, &mut s.state, &mut s.view);
                s.canvas = Some(out.canvas);
            },
            Image {
                state,
                view: View::actual([500.0, 400.0]),
                source,
                overlays: vec![Overlay::new(
                    "crop",
                    Shape::Rect([400.0, 300.0, 600.0, 500.0]),
                )],
                canvas: None,
                control,
            },
        );
    use_profile(&h.ctx, profile);
    h.run();
    h
}

fn screen(h: &Harness<'_, Image>, p: [f64; 2]) -> Pos2 {
    h.state().state.mapping().unwrap().image_to_screen(p)
}

fn zoom(h: &Harness<'_, Image>) -> f64 {
    match h.state().view.zoom {
        Zoom::Px(z) => z,
        Zoom::Fit => f64::NAN,
    }
}

fn drag<S>(h: &mut Harness<'_, S>, from: Pos2, by: Vec2) {
    h.hover_at(from);
    h.step();
    h.input_mut().events.push(Event::PointerButton {
        pos: from,
        button: PointerButton::Primary,
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
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run();
}

fn press<S>(h: &mut Harness<'_, S>, modifiers: Modifiers, key: Key) {
    h.key_press_modifiers(modifiers, key);
    h.run();
}

#[test]
fn hand_mode_pans_the_image_from_a_handle_and_shows_itself() {
    let mut h = image("document", false);
    let corner = screen(&h, [600.0, 500.0]);
    // Off: a drag on a handle is the handle's, the view stays.
    drag(&mut h, corner, vec2(20.0, 0.0));
    assert_eq!(h.state().view, View::actual([500.0, 400.0]));
    // H over the image: hand mode, its indicator.
    h.hover_at(corner);
    press(&mut h, Modifiers::NONE, Key::H);
    assert!(navigation::hand(&h.ctx));
    assert!(h.query_by_label(HAND).is_some());
    let corner = screen(&h, [600.0, 500.0]);
    drag(&mut h, corner, vec2(20.0, 0.0));
    assert!(h.state().view.center[0] < 490.0, "{:?}", h.state().view);
    assert_eq!(
        h.state().overlays[0].shape,
        Shape::Rect([400.0, 300.0, 600.0, 500.0]),
        "the handle stays"
    );
    h.hover_at(corner);
    press(&mut h, Modifiers::NONE, Key::H);
    assert!(!navigation::hand(&h.ctx));
    assert!(h.query_by_label(HAND).is_none());
}

#[test]
fn the_image_takes_the_view_commands_and_their_keys_and_is_the_canvas_in_use() {
    let mut h = image("document", false);
    let centre = h.get_by_label("Image viewport").rect().center();
    h.hover_at(centre);
    h.run();
    let id = h.state().canvas.unwrap();
    assert_eq!(canvas::active(&h.ctx), Some(id), "canvasActive");
    assert_eq!(canvas::zoom(&h.ctx, id), Some(1.0), "the zoom published");
    press(&mut h, Modifiers::COMMAND, Key::Plus);
    assert_eq!(zoom(&h), 2.0, "Ctrl+Plus: the ladder's next step");
    press(&mut h, Modifiers::COMMAND, Key::Minus);
    assert_eq!(zoom(&h), 1.0);
    let x = h.state().view.center[0];
    press(&mut h, Modifiers::COMMAND, Key::ArrowRight);
    assert!(h.state().view.center[0] > x, "Ctrl+Right pans right");
    press(&mut h, Modifiers::NONE, Key::Home);
    assert!(h.state().view.is_fit(), "Home fits");
    press(&mut h, Modifiers::COMMAND, Key::Num0);
    assert_eq!(zoom(&h), 1.0, "Ctrl+0: 100 %");
    // The application's commands, a zoom to a percent included.
    canvas::request_to(&h.ctx, id, &NavCommand::ZoomTo(400).request());
    h.run();
    assert_eq!(zoom(&h), 4.0);
    assert_eq!(canvas::zoom(&h.ctx, id), Some(4.0));
    canvas::request_to(&h.ctx, id, "view.frame_selection");
    h.run();
    assert!(h.state().view.is_fit(), "the image is the selection");
    // Elsewhere, its keys do nothing.
    h.hover_at(Pos2::new(415.0, 335.0));
    h.run();
    press(&mut h, Modifiers::COMMAND, Key::Num0);
    assert!(h.state().view.is_fit());
}

#[test]
fn each_profile_binds_its_own_navigation_keys_on_the_image() {
    for (profile, modifiers, key, fit, zoom_to) in [
        ("photoshop", Modifiers::COMMAND, Key::Num1, false, 1.0),
        ("photoshop", Modifiers::COMMAND, Key::Num0, true, 0.0),
        ("darktable", Modifiers::ALT, Key::Num1, false, 1.0),
        ("darktable", Modifiers::ALT, Key::Num3, true, 0.0),
    ] {
        let mut h = image(profile, false);
        let centre = h.get_by_label("Image viewport").rect().center();
        h.hover_at(centre);
        h.run();
        canvas::request_to(
            &h.ctx,
            h.state().canvas.unwrap(),
            if fit { "view.zoom_in" } else { "view.fit" },
        );
        h.run();
        press(&mut h, modifiers, key);
        if fit {
            assert!(h.state().view.is_fit(), "{profile} {key:?}");
        } else {
            assert_eq!(zoom(&h), zoom_to, "{profile} {key:?}");
        }
    }
}

fn touch<S>(h: &mut Harness<'_, S>, at: [Pos2; 2], phase: TouchPhase) {
    for (i, pos) in at.into_iter().enumerate() {
        h.input_mut().events.push(Event::Touch {
            device_id: TouchDeviceId(1),
            id: TouchId(i as u64),
            phase,
            pos,
            force: None,
        });
    }
    h.step();
}

/// Two fingers from `at`, moved by `by` in four steps, lifted.
fn two_fingers<S>(h: &mut Harness<'_, S>, at: Pos2, by: Vec2) {
    let fingers = |d: Vec2| [at + d - vec2(30.0, 0.0), at + d + vec2(30.0, 0.0)];
    // egui starts a gesture where the pointer is (its first finger, on
    // a real touch screen).
    h.hover_at(at);
    h.step();
    touch(h, fingers(Vec2::ZERO), TouchPhase::Start);
    for i in 1..=4 {
        touch(h, fingers(by * (i as f32 / 4.0)), TouchPhase::Move);
    }
    touch(h, fingers(by), TouchPhase::End);
    h.run();
}

#[test]
fn two_fingers_on_a_touch_screen_pan_the_image_and_the_graph() {
    let mut h = image("document", false);
    let centre = h.get_by_label("Image viewport").rect().center();
    two_fingers(&mut h, centre, vec2(40.0, 0.0));
    assert!(
        h.state().view.center[0] < 470.0,
        "the image follows the fingers: {:?}",
        h.state().view
    );
    assert_eq!(
        h.state().overlays[0].shape,
        Shape::Rect([400.0, 300.0, 600.0, 500.0])
    );

    let mut g = graph();
    let at = g.get_by_label("Node graph").rect().left_bottom() + vec2(80.0, -40.0);
    let before = g.state().view.view;
    two_fingers(&mut g, at, vec2(0.0, -40.0));
    assert!(
        g.state().view.view.min.y > before.min.y + 1.0,
        "the graph follows the fingers: {before:?} -> {:?}",
        g.state().view.view
    );
}

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

fn graph() -> Harness<'static, GraphScene> {
    let mut h = Harness::builder()
        .with_size(vec2(720.0, 460.0))
        .build_ui_state(
            |ui, s: &mut GraphScene| {
                let out = GraphEditor::new(&s.graph, &mut s.view)
                    .size(vec2(700.0, 400.0))
                    .hint(false)
                    .show(ui);
                CanvasZoom::new(out.canvas).show(ui);
            },
            GraphScene {
                graph: Graph {
                    nodes: vec![
                        Node::new("a", "a", "A", [0.0, 0.0]),
                        Node::new("b", "b", "B", [400.0, 200.0]),
                        Node::new("c", "c", "C", [900.0, 500.0]),
                    ],
                },
                view: GraphState::new(),
            },
        );
    use_profile(&h.ctx, "document");
    h.run();
    h
}

fn pick(h: &mut Harness<'_, impl Sized>, item: &str) {
    h.get_by_label("Zoom level").click();
    h.run();
    h.get_by_label(item).click();
    h.run();
}

#[test]
fn the_zoom_control_drives_the_image() {
    let mut h = image("document", true);
    h.run();
    assert_eq!(
        h.get_by_label("Zoom level").value().as_deref(),
        Some("100 %")
    );
    h.get_by_label("Zoom in").click();
    h.run();
    assert_eq!(zoom(&h), 2.0);
    h.get_by_label("Zoom out").click();
    h.get_by_label("Zoom out").click();
    h.run();
    assert!(
        (zoom(&h) - 2.0 / 3.0).abs() < 1e-9,
        "the ladder's step below 100 %"
    );
    pick(&mut h, "400 %");
    assert_eq!(zoom(&h), 4.0);
    h.run();
    assert_eq!(
        h.get_by_label("Zoom level").value().as_deref(),
        Some("400 %")
    );
    pick(&mut h, "Fit");
    assert!(h.state().view.is_fit());
    pick(&mut h, "100 %");
    assert_eq!(zoom(&h), 1.0);
    // An image has no selection to frame.
    h.get_by_label("Zoom level").click();
    h.run();
    assert!(h.query_by_label("Frame the selection").is_none());
    h.key_press(Key::Escape);
    h.run();
    // Hand mode, from its button.
    h.get_by_label("Hand").click();
    h.run();
    assert!(navigation::hand(&h.ctx));
    assert!(h.query_by_label(HAND).is_some());
    h.get_by_label("Hand").click();
    h.run();
    assert!(!navigation::hand(&h.ctx));
}

#[test]
fn the_zoom_control_drives_the_graph() {
    let mut h = graph();
    let fitted = h.state().view.view;
    pick(&mut h, "100 %");
    assert!((h.state().view.zoom() - 1.0).abs() < 0.01);
    pick(&mut h, "50 %");
    assert!((h.state().view.zoom() - 0.5).abs() < 0.01);
    let z = h.state().view.zoom();
    h.get_by_label("Zoom in").click();
    h.run();
    assert!(h.state().view.zoom() > z);
    h.state_mut().view.selection.insert("a".into());
    pick(&mut h, "Frame the selection");
    assert!(h.state().view.view.width() < fitted.width() * 0.6);
    pick(&mut h, "Fit");
    assert_eq!(h.state().view.view, fitted);
}
