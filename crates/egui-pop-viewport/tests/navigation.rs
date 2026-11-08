//! The shared navigation model on the node graph alone
//! (`docs/design/navigation.md`): hand mode, the navigation keys per
//! profile, fit selection, the view kept by id, the one-time hint and the
//! overview's rule.

use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_core::model::pointer::{self, PointerMap};
use egui_pop_viewport::graph::{Edge, GraphEditor, GraphModel, GraphState, Node};
use egui_pop_viewport::navigation::{self, NavCommand, Overview};

fn use_profile(ctx: &egui::Context, id: &str) {
    let p = pointer::profile(id).unwrap_or_else(|| panic!("no profile `{id}`"));
    egui_pop_core::pointer::set_map(ctx, PointerMap::from_profile(p));
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

struct Scene {
    graph: Graph,
    view: GraphState,
    hint: bool,
    keep: bool,
}

fn graph_with(profile: &str, hint: bool, keep: bool) -> Harness<'static, Scene> {
    let mut h = Harness::builder()
        .with_size(vec2(720.0, 420.0))
        .build_ui_state(
            |ui, s: &mut Scene| {
                let mut e = GraphEditor::new(&s.graph, &mut s.view)
                    .size(vec2(700.0, 400.0))
                    .hint(s.hint);
                if s.keep {
                    e = e.keep_view("nav-test");
                }
                e.show(ui);
            },
            Scene {
                graph: Graph {
                    nodes: vec![
                        Node::new("a", "a", "A", [0.0, 0.0]),
                        Node::new("b", "b", "B", [400.0, 200.0]),
                        Node::new("c", "c", "C", [900.0, 500.0]),
                    ],
                },
                view: GraphState::new(),
                hint,
                keep,
            },
        );
    use_profile(&h.ctx, profile);
    h.run();
    h
}

fn graph(profile: &str) -> Harness<'static, Scene> {
    graph_with(profile, false, false)
}

fn canvas(h: &Harness<'_, Scene>) -> Rect {
    h.get_by_label("Node graph").rect()
}

fn empty_spot(h: &Harness<'_, Scene>) -> Pos2 {
    canvas(h).left_bottom() + vec2(20.0, -20.0)
}

fn drag(h: &mut Harness<'_, Scene>, from: Pos2, by: Vec2, button: PointerButton) {
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

fn press(h: &mut Harness<'_, Scene>, modifiers: Modifiers, key: Key) {
    h.key_press_modifiers(modifiers, key);
    h.run();
}

const HAND: &str = "Hand: drag to pan (H to leave)";
const HINT: &str = "Middle-drag, Space+drag or H to pan · Ctrl+wheel to zoom · F to frame";

#[test]
fn hand_mode_makes_a_primary_drag_pan_in_every_profile_and_shows_itself() {
    for id in pointer::ids() {
        let mut h = graph(id);
        let at = empty_spot(&h);
        // Off: a primary drag on the canvas does not pan (a rubber band,
        // or the profile's own primary drag, which no profile binds on
        // the graph).
        let view = h.state().view.view;
        drag(&mut h, at, vec2(60.0, -30.0), PointerButton::Primary);
        assert_eq!(h.state().view.view, view, "{id}: off, no pan");
        assert!(h.query_by_label(HAND).is_none(), "{id}");
        // H over the canvas turns it on.
        h.hover_at(at);
        press(&mut h, Modifiers::NONE, Key::H);
        assert!(navigation::hand(&h.ctx), "{id}");
        assert!(h.query_by_label(HAND).is_some(), "{id}: the indicator");
        let view = h.state().view.view;
        drag(&mut h, at, vec2(60.0, -30.0), PointerButton::Primary);
        assert!(
            h.state().view.view.min.x < view.min.x - 1.0,
            "{id}: on, pans"
        );
        // From a node too, which stays where it is.
        let a = h.get_by_label("A").rect().center();
        let view = h.state().view.view;
        drag(&mut h, a, vec2(-40.0, 0.0), PointerButton::Primary);
        assert!(
            h.state().view.view.min.x > view.min.x + 1.0,
            "{id}: from a node"
        );
        // H again turns it off.
        h.hover_at(at);
        press(&mut h, Modifiers::NONE, Key::H);
        assert!(!navigation::hand(&h.ctx), "{id}");
        assert!(h.query_by_label(HAND).is_none(), "{id}");
    }
}

/// The view after selecting `a` and pressing a key.
fn frame_with(profile: &str, modifiers: Modifiers, key: Key) -> (Rect, Rect) {
    let mut h = graph(profile);
    let before = h.state().view.view;
    h.state_mut().view.selection.insert("a".into());
    h.hover_at(empty_spot(&h));
    press(&mut h, modifiers, key);
    (before, h.state().view.view)
}

#[test]
fn fit_selection_frames_the_selection_on_f_and_on_the_profile_key() {
    for (id, modifiers, key) in [
        ("document", Modifiers::NONE, Key::F),
        ("document", Modifiers::SHIFT, Key::Num2),
        ("blender", Modifiers::NONE, Key::F),
        ("blender", Modifiers::NONE, Key::Period),
        ("darktable", Modifiers::NONE, Key::F),
    ] {
        let (before, after) = frame_with(id, modifiers, key);
        assert!(
            after.width() < before.width() * 0.6,
            "{id} {key:?}: {before:?} -> {after:?}"
        );
        assert!(after.contains(pos2(10.0, 10.0)), "{id} {key:?}: A in view");
    }
    // A profile's key is its own: Shift+2 does nothing in Blender.
    let (before, after) = frame_with("blender", Modifiers::SHIFT, Key::Num2);
    assert_eq!(before, after);
    // Nothing selected: F frames everything.
    let mut h = graph("document");
    let fitted = h.state().view.view;
    h.hover_at(empty_spot(&h));
    press(&mut h, Modifiers::COMMAND, Key::Plus);
    assert_ne!(h.state().view.view, fitted);
    press(&mut h, Modifiers::NONE, Key::F);
    assert_eq!(h.state().view.view, fitted);
}

#[test]
fn the_zoom_and_pan_keys_move_the_view() {
    let mut h = graph("document");
    h.hover_at(empty_spot(&h));
    let z = h.state().view.zoom();
    press(&mut h, Modifiers::COMMAND, Key::Plus);
    assert!(h.state().view.zoom() > z, "zoom in");
    let z = h.state().view.zoom();
    press(&mut h, Modifiers::COMMAND, Key::Minus);
    assert!(h.state().view.zoom() < z, "zoom out");
    press(&mut h, Modifiers::COMMAND, Key::Num0);
    assert!((h.state().view.zoom() - 1.0).abs() < 0.01, "100 %");
    let v = h.state().view.view;
    press(&mut h, Modifiers::COMMAND, Key::ArrowRight);
    assert!(h.state().view.view.min.x > v.min.x, "Ctrl+Right pans");
    let v = h.state().view.view;
    press(&mut h, Modifiers::COMMAND, Key::ArrowUp);
    assert!(h.state().view.view.min.y < v.min.y, "Ctrl+Up pans");
    // Home fits.
    press(&mut h, Modifiers::NONE, Key::Home);
    let fitted = h.state().view.view;
    press(&mut h, Modifiers::COMMAND, Key::Num0);
    assert_ne!(h.state().view.view, fitted);
    // The keys need the pointer over the canvas or the keyboard on it.
    h.hover_at(pos2(715.0, 415.0));
    let v = h.state().view.view;
    press(&mut h, Modifiers::COMMAND, Key::Minus);
    assert_eq!(h.state().view.view, v, "elsewhere, nothing");
}

#[test]
fn the_applications_commands_reach_the_canvas_in_use() {
    let mut h = graph("document");
    h.hover_at(empty_spot(&h));
    h.run();
    let z = h.state().view.zoom();
    assert!(egui_pop_core::canvas::request(
        &h.ctx,
        NavCommand::ZoomIn.id()
    ));
    h.run();
    assert!(h.state().view.zoom() > z);
    assert!(egui_pop_core::canvas::request(&h.ctx, "view.hand"));
    h.run();
    assert!(navigation::hand(&h.ctx));
}

#[test]
fn a_view_kept_by_id_comes_back_in_a_new_state() {
    let mut h = graph_with("document", false, true);
    h.hover_at(empty_spot(&h));
    press(&mut h, Modifiers::COMMAND, Key::Num0);
    press(&mut h, Modifiers::COMMAND, Key::ArrowRight);
    let kept = h.state().view.view;
    // A new session's state: the view comes back, not a fit.
    h.state_mut().view = GraphState::new();
    h.run();
    assert_eq!(h.state().view.view, kept);
    // Without an id, a new state is fitted.
    let mut h = graph("document");
    let fitted = h.state().view.view;
    h.hover_at(empty_spot(&h));
    press(&mut h, Modifiers::COMMAND, Key::Num0);
    h.state_mut().view = GraphState::new();
    h.run();
    assert_eq!(h.state().view.view, fitted);
}

#[test]
fn the_hint_shows_on_first_use_until_the_first_pan_then_never() {
    let mut h = graph_with("document", true, false);
    assert!(
        h.query_by_label(HINT).is_none(),
        "not before the first hover"
    );
    let at = empty_spot(&h);
    h.hover_at(at);
    h.run();
    assert!(h.query_by_label(HINT).is_some(), "shown on first hover");
    // A wheel or a key does not dismiss it; a pan does.
    drag(&mut h, at, vec2(40.0, -20.0), PointerButton::Middle);
    assert!(h.query_by_label(HINT).is_none(), "dismissed by a pan");
    h.hover_at(at + vec2(5.0, 0.0));
    h.run();
    assert!(h.query_by_label(HINT).is_none(), "never again");
    assert!(navigation::hint_seen(&h.ctx, "graph"));
    // Dismissed by a click on it.
    let mut h = graph_with("document", true, false);
    h.hover_at(empty_spot(&h));
    h.run();
    h.get_by_label(HINT).click();
    h.run();
    assert!(h.query_by_label(HINT).is_none(), "dismissed by a click");
    assert!(navigation::hint_seen(&h.ctx, "graph"));
}

#[test]
fn the_overview_shows_only_when_the_graph_is_much_larger_than_the_view() {
    let r = |w: f32, h: f32| Rect::from_min_size(Pos2::ZERO, vec2(w, h));
    assert!(!navigation::overview_needed(
        r(100.0, 100.0),
        r(100.0, 100.0)
    ));
    assert!(!navigation::overview_needed(
        r(190.0, 100.0),
        r(100.0, 100.0)
    ));
    assert!(navigation::overview_needed(
        r(250.0, 100.0),
        r(100.0, 100.0)
    ));
    assert!(navigation::overview_needed(
        r(100.0, 250.0),
        r(100.0, 100.0)
    ));
    assert!(Overview::Always.shows(r(1.0, 1.0), r(100.0, 100.0)));
    assert!(!Overview::Never.shows(r(900.0, 900.0), r(100.0, 100.0)));

    let mut h = graph("document");
    assert!(h.query_by_label("Graph overview").is_none(), "fitted");
    h.hover_at(empty_spot(&h));
    // Zoomed in to the range's end (150 %): the graph is more than twice
    // the view across.
    press(&mut h, Modifiers::COMMAND, Key::Num0);
    press(&mut h, Modifiers::COMMAND, Key::Plus);
    press(&mut h, Modifiers::COMMAND, Key::Plus);
    h.run();
    assert!(h.query_by_label("Graph overview").is_some(), "zoomed in");
    // A click on it moves the view.
    let v = h.state().view.view;
    let o = h.get_by_label("Graph overview").rect();
    h.hover_at(o.left_top() + vec2(6.0, 6.0));
    h.input_mut().events.push(Event::PointerButton {
        pos: o.left_top() + vec2(6.0, 6.0),
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.step();
    h.input_mut().events.push(Event::PointerButton {
        pos: o.left_top() + vec2(6.0, 6.0),
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run();
    assert_ne!(h.state().view.view, v, "the overview moves the view");
    h.hover_at(empty_spot(&h));
    h.run();
    press(&mut h, Modifiers::NONE, Key::Home);
    h.run();
    assert!(h.query_by_label("Graph overview").is_none(), "fitted again");
}
