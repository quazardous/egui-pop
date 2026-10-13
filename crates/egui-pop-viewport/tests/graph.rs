//! The node graph editor: a wire dragged between ports, a wire of the
//! wrong type refused, a node deleted, a veto, a wire made from the
//! keyboard, and the overview against the detail view.

use egui::accesskit::Role;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_viewport::change::{Cause, Verdict};
use egui_pop_viewport::graph::{
    Edge, Edit, GraphEditor, GraphEvent, GraphModel, GraphState, Level, Node, Param, Port, PortRef,
    PortType, Request,
};

#[derive(Debug, Default)]
struct Pipeline {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    radius: f64,
}

impl GraphModel for Pipeline {
    fn nodes(&self) -> Vec<Node> {
        self.nodes.clone()
    }

    fn edges(&self) -> Vec<Edge> {
        self.edges.clone()
    }

    fn params(&self, node: &str) -> Vec<Param> {
        if node == "blur" {
            vec![Param::new("radius", "Radius", self.radius, 0.0..=10.0).step(0.1)]
        } else {
            Vec::new()
        }
    }
}

impl Pipeline {
    fn new() -> Pipeline {
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
            ],
            edges: Vec::new(),
            radius: 2.0,
        }
    }

    fn apply(&mut self, e: &Edit) {
        match e {
            Edit::Connect { from, to, replaces } => {
                self.edges.retain(|x| !replaces.contains(x));
                self.edges.push(Edge::new(from.clone(), to.clone()));
            }
            Edit::Disconnect { edges } => self.edges.retain(|x| !edges.contains(x)),
            Edit::Move { moves } => {
                for m in moves {
                    if let Some(n) = self.nodes.iter_mut().find(|n| n.id == m.node) {
                        n.pos = m.to;
                    }
                }
            }
            Edit::Delete { nodes, edges } => {
                self.nodes.retain(|n| !nodes.contains(&n.id));
                self.edges.retain(|x| !edges.contains(x));
            }
            Edit::Add { .. } => {}
            Edit::Param { to, .. } => self.radius = *to,
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
    level: Option<Level>,
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
                    .on_edit(|e: &Edit, cause| {
                        causes.push(cause);
                        if veto && matches!(e, Edit::Connect { .. }) {
                            Verdict::Veto
                        } else {
                            Verdict::Allow
                        }
                    })
                    .show(ui);
                for e in &out.edits {
                    s.graph.apply(e);
                }
                s.edits.extend(out.edits);
                s.causes.extend(causes);
                s.events.extend(out.events);
                s.level = Some(out.level);
            },
            State {
                graph: Pipeline::new(),
                view: GraphState::new(),
                veto: false,
                edits: Vec::new(),
                causes: Vec::new(),
                events: Vec::new(),
                level: None,
            },
        );
    h.run();
    h
}

fn drag(h: &mut Harness<'_, State>, from: Pos2, to: Pos2) {
    h.event(Event::PointerMoved(from));
    h.event(Event::PointerButton {
        pos: from,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.step();
    for i in 1..=6 {
        h.event(Event::PointerMoved(from + (to - from) * (i as f32 / 6.0)));
        h.step();
    }
    h.event(Event::PointerButton {
        pos: to,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run();
}

fn centre(h: &Harness<'_, State>, label: &str) -> Pos2 {
    h.get_by_label(label).rect().center()
}

fn wire(from: (&str, &str), to: (&str, &str)) -> Edge {
    Edge::new(PortRef::new(from.0, from.1), PortRef::new(to.0, to.1))
}

#[test]
fn a_wire_dragged_from_an_output_to_an_input_connects() {
    let mut h = editor();
    assert_eq!(h.state().level, Some(Level::Ports));
    egui_pop_test::assert_all_named(&h);
    let canvas = h.get_by_label("Node graph");
    assert_eq!(canvas.accesskit_node().role(), Role::Group);
    let port = h.get_by_label("Load, output Image");
    assert_eq!(port.accesskit_node().role(), Role::Button);
    let (a, b) = (
        centre(&h, "Load, output Image"),
        centre(&h, "Blur, input Image"),
    );
    drag(&mut h, a, b);
    let s = h.state();
    assert_eq!(
        s.graph.edges,
        vec![wire(("load", "image"), ("blur", "image"))],
        "{:?}",
        s.edits
    );
    assert_eq!(s.causes, vec![Cause::Drag]);
    // The input's value counts its wire now.
    let value = h
        .get_by_label("Blur, input Image")
        .value()
        .unwrap_or_default();
    assert!(value.contains('1'), "{value}");
}

#[test]
fn a_wire_of_another_type_is_refused() {
    let mut h = editor();
    let (a, b) = (
        centre(&h, "Mask, output Mask"),
        centre(&h, "Blur, input Image"),
    );
    drag(&mut h, a, b);
    let s = h.state();
    assert!(s.graph.edges.is_empty(), "{:?}", s.graph.edges);
    assert!(s.edits.is_empty(), "{:?}", s.edits);
    assert!(
        s.events
            .iter()
            .any(|e| matches!(e, GraphEvent::Refused { reason, .. } if reason.contains("image"))),
        "{:?}",
        s.events
    );
    // The mask goes where a mask is wanted.
    let (a, b) = (
        centre(&h, "Mask, output Mask"),
        centre(&h, "Mix, input Mask"),
    );
    drag(&mut h, a, b);
    assert_eq!(
        h.state().graph.edges,
        vec![wire(("mask", "mask"), ("mix", "m"))]
    );
}

#[test]
fn a_selected_node_is_deleted_with_its_wires() {
    let mut h = editor();
    h.state_mut().graph.edges = vec![wire(("load", "image"), ("blur", "image"))];
    h.run();
    h.get_by_label("Blur").click();
    h.run();
    assert!(h.state().view.selection.contains("blur"));
    assert!(
        h.state()
            .events
            .iter()
            .any(|e| matches!(e, GraphEvent::Selection(n) if n == &["blur".to_string()]))
    );
    h.key_press(Key::Delete);
    h.run();
    let s = h.state();
    assert_eq!(
        s.edits.last(),
        Some(&Edit::Delete {
            nodes: vec!["blur".into()],
            edges: vec![wire(("load", "image"), ("blur", "image"))],
        })
    );
    assert!(s.graph.nodes.iter().all(|n| n.id != "blur"));
    assert!(s.graph.edges.is_empty());
    assert!(h.query_by_label("Blur").is_none());
}

#[test]
fn a_vetoed_connection_changes_nothing() {
    let mut h = editor();
    h.state_mut().veto = true;
    let (a, b) = (
        centre(&h, "Load, output Image"),
        centre(&h, "Blur, input Image"),
    );
    drag(&mut h, a, b);
    let s = h.state();
    assert_eq!(s.causes, vec![Cause::Drag], "the hook saw it");
    assert!(s.edits.is_empty());
    assert!(s.graph.edges.is_empty());
}

#[test]
fn the_keyboard_picks_two_ports_and_connects_them() {
    let mut h = editor();
    h.get_by_label("Load, output Image").focus();
    h.run();
    h.key_press(Key::Enter);
    h.run();
    assert!(h.state().view.wiring(), "a wire is begun");
    h.get_by_label("Blur, input Image").focus();
    h.run();
    h.key_press(Key::Enter);
    h.run();
    let s = h.state();
    assert!(!s.view.wiring());
    assert_eq!(
        s.graph.edges,
        vec![wire(("load", "image"), ("blur", "image"))]
    );
    // Delete on the focused port disconnects it.
    h.key_press(Key::Delete);
    h.run();
    assert!(h.state().graph.edges.is_empty());
    // The arrows go through the node's ports; Escape goes back to the
    // node.
    h.key_press(Key::ArrowDown);
    h.run();
    assert!(h.get_by_label("Blur, output Image").is_focused());
    h.key_press(Key::Escape);
    h.run();
    assert!(h.get_by_label("Blur").is_focused());
    // From a node, the arrows go to the nearest node that way.
    h.key_press(Key::ArrowRight);
    h.run();
    assert!(h.get_by_label("Mix").is_focused());
}

#[test]
fn the_overview_is_compact_and_the_detail_edits_parameters() {
    let mut h = editor();
    h.state_mut().view.request(Request::Level(Level::Detail));
    h.run();
    assert_eq!(h.state().level, Some(Level::Detail));
    assert!(h.state().events.contains(&GraphEvent::Level(Level::Detail)));
    let radius = h.get_by_label("Radius");
    assert_eq!(radius.accesskit_node().role(), Role::Slider);
    egui_pop_test::assert_all_named(&h);
    radius.focus();
    h.run();
    h.key_press(Key::ArrowRight);
    h.run();
    let s = h.state();
    assert!((s.graph.radius - 2.1).abs() < 1e-9, "{}", s.graph.radius);
    assert!(
        s.edits
            .iter()
            .any(|e| matches!(e, Edit::Param { node, param, done: true, .. } if node == "blur" && param == "radius"))
    );

    h.state_mut().view.request(Request::Level(Level::Overview));
    h.run();
    assert_eq!(h.state().level, Some(Level::Overview));
    assert!(h.query_by_label("Radius").is_none(), "no parameters");
    assert!(h.query_by_label("Load, output Image").is_none(), "no ports");
    assert!(h.query_by_label("Blur").is_some(), "the nodes stay");
}
