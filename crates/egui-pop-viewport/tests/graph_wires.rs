//! The node graph editor's wires as targets: hovered, selected by a click
//! (Ctrl adds), deleted as one edit, disconnected from their menu, moved by
//! either end, dropped nowhere, reached from the keyboard, selected by a
//! request.

use egui::accesskit::Role;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_viewport::change::{Cause, Verdict};
use egui_pop_viewport::graph::{
    Edge, Edit, GraphEditor, GraphEvent, GraphModel, GraphState, Node, Port, PortRef, PortType,
    Request,
};

#[derive(Debug, Default)]
struct Pipeline {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
}

impl GraphModel for Pipeline {
    fn nodes(&self) -> Vec<Node> {
        self.nodes.clone()
    }

    fn edges(&self) -> Vec<Edge> {
        self.edges.clone()
    }
}

fn wire(from: (&str, &str), to: (&str, &str)) -> Edge {
    Edge::new(PortRef::new(from.0, from.1), PortRef::new(to.0, to.1))
}

/// Load to Blur (a straight wire), Mask to Mix's mask; a second source
/// below.
fn pipeline() -> Pipeline {
    let image = || PortType::new("image");
    let mask = || PortType::new("mask");
    Pipeline {
        nodes: vec![
            Node::new("load", "load", "Load", [0.0, 0.0]).output(Port::new(
                "image",
                "Image",
                image(),
            )),
            Node::new("blur", "blur", "Blur", [260.0, 0.0])
                .input(Port::new("image", "Image", image()))
                .output(Port::new("image", "Image", image())),
            Node::new("mask", "mask", "Mask", [0.0, 180.0]).output(Port::new(
                "mask",
                "Mask",
                mask(),
            )),
            Node::new("mix", "mix", "Mix", [560.0, 90.0])
                .input(Port::new("a", "Base", image()))
                .input(Port::new("m", "Mask", mask()))
                .output(Port::new("image", "Image", image())),
            Node::new("load2", "load", "Second", [0.0, 300.0]).output(Port::new(
                "image",
                "Image",
                image(),
            )),
        ],
        edges: vec![
            wire(("load", "image"), ("blur", "image")),
            wire(("mask", "mask"), ("mix", "m")),
        ],
    }
}

impl Pipeline {
    fn apply(&mut self, e: &Edit) {
        match e {
            Edit::Connect { from, to, replaces } => {
                self.edges.retain(|x| !replaces.contains(x));
                self.edges.push(Edge::new(from.clone(), to.clone()));
            }
            Edit::Disconnect { edges } => self.edges.retain(|x| !edges.contains(x)),
            Edit::Delete { nodes, edges } => {
                self.nodes.retain(|n| !nodes.contains(&n.id));
                self.edges.retain(|x| !edges.contains(x));
            }
            _ => {}
        }
    }
}

#[derive(Debug)]
struct State {
    graph: Pipeline,
    view: GraphState,
    veto: bool,
    edits: Vec<Edit>,
    causes: Vec<Cause>,
    events: Vec<GraphEvent>,
}

fn editor() -> Harness<'static, State> {
    let mut h = Harness::builder()
        .with_size(vec2(720.0, 420.0))
        .build_ui_state(
            |ui, s: &mut State| {
                let veto = s.veto;
                let mut causes = Vec::new();
                let out = GraphEditor::new(&s.graph, &mut s.view)
                    .size(vec2(700.0, 400.0))
                    .on_edit(|_: &Edit, cause| {
                        causes.push(cause);
                        if veto { Verdict::Veto } else { Verdict::Allow }
                    })
                    .show(ui);
                for e in &out.edits {
                    s.graph.apply(e);
                }
                s.edits.extend(out.edits);
                s.causes.extend(causes);
                s.events.extend(out.events);
            },
            State {
                graph: pipeline(),
                view: GraphState::new(),
                veto: false,
                edits: Vec::new(),
                causes: Vec::new(),
                events: Vec::new(),
            },
        );
    h.run();
    h
}

const LOAD_BLUR: &str = "Load, output Image to Blur, input Image";
const MASK_MIX: &str = "Mask, output Mask to Mix, input Mask";

fn centre(h: &Harness<'_, State>, label: &str) -> Pos2 {
    h.get_by_label(label).rect().center()
}

/// A point on the straight wire from Load to Blur, `t` of the way.
fn on_load_blur(h: &Harness<'_, State>, t: f32) -> Pos2 {
    let (a, b) = (
        centre(h, "Load, output Image"),
        centre(h, "Blur, input Image"),
    );
    a + (b - a) * t
}

fn press(h: &mut Harness<'_, State>, at: Pos2, button: PointerButton, pressed: bool, m: Modifiers) {
    h.event(Event::PointerButton {
        pos: at,
        button,
        pressed,
        modifiers: m,
    });
}

fn click(h: &mut Harness<'_, State>, at: Pos2, button: PointerButton, m: Modifiers) {
    h.event(Event::PointerMoved(at));
    h.run();
    h.event(Event::ModifiersChanged(m));
    press(h, at, button, true, m);
    h.step();
    press(h, at, button, false, m);
    h.run();
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.run();
}

fn drag(h: &mut Harness<'_, State>, from: Pos2, to: Pos2) {
    h.event(Event::PointerMoved(from));
    h.run();
    press(h, from, PointerButton::Primary, true, Modifiers::NONE);
    h.step();
    for i in 1..=6 {
        h.event(Event::PointerMoved(from + (to - from) * (i as f32 / 6.0)));
        h.step();
    }
    press(h, to, PointerButton::Primary, false, Modifiers::NONE);
    h.run();
}

#[test]
fn a_hovered_wire_names_its_ends() {
    let mut h = editor();
    egui_pop_test::assert_all_named(&h);
    let w = h.get_by_label(LOAD_BLUR);
    assert_eq!(w.accesskit_node().role(), Role::Button);
    assert!(h.query_by_label("Load · Image to Blur · Image").is_none());
    let at = on_load_blur(&h, 0.5);
    h.event(Event::PointerMoved(at));
    h.run();
    assert!(
        h.query_by_label("Load · Image to Blur · Image").is_some(),
        "the tooltip"
    );
    // Off the wire, no tooltip.
    h.event(Event::PointerMoved(at + vec2(0.0, 30.0)));
    h.run();
    assert!(h.query_by_label("Load · Image to Blur · Image").is_none());
}

#[test]
fn a_click_selects_a_wire_ctrl_adds_and_delete_disconnects_them_as_one_edit() {
    let mut h = editor();
    h.get_by_label("Load").click();
    h.run();
    assert!(h.state().view.selection.contains("load"));
    let at = on_load_blur(&h, 0.5);
    click(&mut h, at, PointerButton::Primary, Modifiers::NONE);
    let lb = wire(("load", "image"), ("blur", "image"));
    let mm = wire(("mask", "mask"), ("mix", "m"));
    let s = h.state();
    assert_eq!(s.view.selected_edges.iter().collect::<Vec<_>>(), [&lb]);
    assert!(
        s.view.selection.is_empty(),
        "a plain click selects the wire alone"
    );
    assert!(
        s.events
            .iter()
            .any(|e| matches!(e, GraphEvent::WireSelection(w) if w == std::slice::from_ref(&lb)))
    );
    assert_eq!(
        h.get_by_label(LOAD_BLUR).accesskit_node().is_selected(),
        Some(true)
    );
    // Ctrl adds the other.
    let at = centre(&h, MASK_MIX);
    click(&mut h, at, PointerButton::Primary, Modifiers::COMMAND);
    assert_eq!(h.state().view.selected_edges.len(), 2);
    // Delete: one edit for both.
    h.key_press(Key::Delete);
    h.run();
    let s = h.state();
    assert_eq!(
        s.edits,
        vec![Edit::Disconnect {
            edges: vec![lb, mm]
        }]
    );
    assert_eq!(s.causes, vec![Cause::Key]);
    assert!(s.graph.edges.is_empty());
    assert!(
        s.view.selected_edges.is_empty(),
        "gone, so no longer selected"
    );
}

#[test]
fn a_vetoed_wire_delete_keeps_the_wire_selected() {
    let mut h = editor();
    h.state_mut().veto = true;
    let at = on_load_blur(&h, 0.5);
    click(&mut h, at, PointerButton::Primary, Modifiers::NONE);
    h.key_press(Key::Backspace);
    h.run();
    let s = h.state();
    assert_eq!(s.causes, vec![Cause::Key], "the hook saw it");
    assert!(s.edits.is_empty());
    assert_eq!(s.graph.edges.len(), 2);
    assert_eq!(s.view.selected_edges.len(), 1);
}

#[test]
fn a_click_on_the_canvas_clears_the_wires() {
    let mut h = editor();
    let at = on_load_blur(&h, 0.5);
    click(&mut h, at, PointerButton::Primary, Modifiers::NONE);
    assert_eq!(h.state().view.selected_edges.len(), 1);
    let empty = on_load_blur(&h, 0.5) + vec2(0.0, 120.0);
    click(&mut h, empty, PointerButton::Primary, Modifiers::NONE);
    assert!(h.state().view.selected_edges.is_empty());
}

#[test]
fn a_wire_menu_disconnects_it_and_selects_its_ends() {
    let mut h = editor();
    let at = on_load_blur(&h, 0.5);
    click(&mut h, at, PointerButton::Secondary, Modifiers::NONE);
    h.get_by_label("Select the destination").click();
    h.run();
    assert_eq!(
        h.state().view.selection.iter().collect::<Vec<_>>(),
        ["blur"]
    );
    let at = on_load_blur(&h, 0.5);
    click(&mut h, at, PointerButton::Secondary, Modifiers::NONE);
    h.get_by_label("Disconnect").click();
    h.run();
    let s = h.state();
    assert_eq!(
        s.edits,
        vec![Edit::Disconnect {
            edges: vec![wire(("load", "image"), ("blur", "image"))]
        }]
    );
    assert_eq!(s.causes, vec![Cause::Command]);
}

#[test]
fn a_wire_dragged_by_either_end_reconnects_in_one_edit() {
    let mut h = editor();
    // Its input end (nearer Blur) to Mix's base.
    let (from, to) = (on_load_blur(&h, 0.75), centre(&h, "Mix, input Base"));
    drag(&mut h, from, to);
    let lb = wire(("load", "image"), ("blur", "image"));
    assert_eq!(
        h.state().edits,
        vec![Edit::Connect {
            from: PortRef::new("load", "image"),
            to: PortRef::new("mix", "a"),
            replaces: vec![lb.clone()],
        }]
    );
    assert_eq!(h.state().causes, vec![Cause::Drag]);
    // Back on Blur, then its output end (nearer Load) to the second source.
    h.state_mut().graph = pipeline();
    h.state_mut().edits.clear();
    h.run();
    let (from, to) = (on_load_blur(&h, 0.25), centre(&h, "Second, output Image"));
    drag(&mut h, from, to);
    assert_eq!(
        h.state().edits,
        vec![Edit::Connect {
            from: PortRef::new("load2", "image"),
            to: PortRef::new("blur", "image"),
            replaces: vec![lb],
        }]
    );
    assert!(
        h.state()
            .graph
            .edges
            .contains(&wire(("load2", "image"), ("blur", "image")))
    );
}

#[test]
fn a_wire_end_dropped_nowhere_disconnects() {
    let mut h = editor();
    let from = on_load_blur(&h, 0.75);
    drag(&mut h, from, from + vec2(0.0, 140.0));
    let s = h.state();
    assert_eq!(
        s.edits,
        vec![Edit::Disconnect {
            edges: vec![wire(("load", "image"), ("blur", "image"))]
        }]
    );
    assert_eq!(s.causes, vec![Cause::Drag]);
}

#[test]
fn the_keyboard_reaches_a_wire_after_its_input() {
    let mut h = editor();
    h.get_by_label("Blur, input Image").focus();
    h.run();
    h.key_press(Key::Tab);
    h.run();
    let w = h.get_by_label(LOAD_BLUR);
    assert!(w.is_focused(), "Tab from the input reaches its wire");
    assert_eq!(
        w.accesskit_node().description().as_deref(),
        Some(
            "Delete to disconnect, Enter to move its input end, Shift+Enter to move its output end"
        )
    );
    // Escape goes back to the port.
    h.key_press(Key::Escape);
    h.run();
    assert!(h.get_by_label("Blur, input Image").is_focused());
    // Enter picks the input end up; Enter on Mix's base moves it there.
    h.get_by_label(LOAD_BLUR).focus();
    h.run();
    h.key_press(Key::Enter);
    h.run();
    assert!(h.state().view.wiring());
    h.get_by_label("Mix, input Base").focus();
    h.run();
    h.key_press(Key::Enter);
    h.run();
    let lb = wire(("load", "image"), ("blur", "image"));
    assert_eq!(
        h.state().edits,
        vec![Edit::Connect {
            from: PortRef::new("load", "image"),
            to: PortRef::new("mix", "a"),
            replaces: vec![lb.clone()],
        }]
    );
    // Shift+Enter picks the output end up; the second source takes it.
    let lm = "Load, output Image to Mix, input Base";
    h.get_by_label(lm).focus();
    h.run();
    h.key_press_modifiers(Modifiers::SHIFT, Key::Enter);
    h.run();
    assert!(h.state().view.wiring());
    h.get_by_label("Second, output Image").focus();
    h.run();
    h.key_press(Key::Enter);
    h.run();
    assert!(
        h.state()
            .graph
            .edges
            .contains(&wire(("load2", "image"), ("mix", "a")))
    );
    // Delete on a focused wire disconnects it, back to its input.
    h.get_by_label("Second, output Image to Mix, input Base")
        .focus();
    h.run();
    h.key_press(Key::Delete);
    h.run();
    assert!(
        !h.state()
            .graph
            .edges
            .iter()
            .any(|e| e.to == PortRef::new("mix", "a"))
    );
    assert!(h.get_by_label("Mix, input Base").is_focused());
}

#[test]
fn a_request_selects_wires() {
    let mut h = editor();
    let mm = wire(("mask", "mask"), ("mix", "m"));
    let gone = wire(("load2", "image"), ("mix", "a"));
    h.state_mut()
        .view
        .request(Request::SelectWires(vec![mm.clone(), gone]));
    h.run();
    assert_eq!(
        h.state().view.selected_edges.iter().collect::<Vec<_>>(),
        [&mm],
        "only wires that are there"
    );
    // A connection asked with `replaces` moves that wire's end: one edit.
    h.state_mut().view.request(Request::Edit(Edit::Connect {
        from: PortRef::new("load", "image"),
        to: PortRef::new("mix", "a"),
        replaces: vec![wire(("load", "image"), ("blur", "image"))],
    }));
    h.run();
    assert_eq!(
        h.state().graph.edges,
        vec![mm, wire(("load", "image"), ("mix", "a"))]
    );
}
