//! The graph as one Tab stop: Tab enters and leaves it, the arrows go
//! between nodes, Enter goes into a node's parts, Tab goes round them and
//! Escape comes back, with the focus plugin.

use egui::{Key, Ui, vec2};
use egui_kittest::Harness;
use egui_pop_core::focus;
use egui_pop_test::walk::{self, ESCAPE, End, Press, SHIFT_TAB, TAB};
use egui_pop_viewport::graph::{
    Edge, GraphEditor, GraphModel, GraphState, Node, Param, Port, PortType, TabStops,
};

#[derive(Debug)]
struct Pipeline(Vec<Node>);

impl GraphModel for Pipeline {
    fn nodes(&self) -> Vec<Node> {
        self.0.clone()
    }

    fn edges(&self) -> Vec<Edge> {
        Vec::new()
    }

    fn params(&self, node: &str) -> Vec<Param> {
        if node == "blur" {
            vec![Param::new("radius", "Radius", 2.0, 0.0..=10.0)]
        } else {
            Vec::new()
        }
    }
}

fn pipeline() -> Pipeline {
    let image = || PortType::new("image");
    Pipeline(vec![
        Node::new("load", "load", "Load", [0.0, 0.0]).output(Port::new("image", "Image", image())),
        Node::new("blur", "blur", "Blur", [260.0, 0.0])
            .input(Port::new("image", "Image", image()))
            .output(Port::new("image", "Image", image())),
        Node::new("save", "save", "Save", [520.0, 0.0]).input(Port::new("image", "Image", image())),
    ])
}

fn page<'a>(stops: TabStops) -> Harness<'a, (Pipeline, GraphState)> {
    let mut h = Harness::builder()
        .with_size(vec2(760.0, 420.0))
        .build_ui_state(
            move |ui: &mut Ui, s: &mut (Pipeline, GraphState)| {
                let _ = ui.button("Before");
                GraphEditor::new(&s.0, &mut s.1)
                    .size(vec2(740.0, 340.0))
                    .label("Pipeline")
                    .tab_stops(stops)
                    .show(ui);
                let _ = ui.button("After");
            },
            (pipeline(), GraphState::new()),
        );
    focus::install(&h.ctx);
    let _ = h.run_ok();
    h
}

fn name(stop: Option<walk::Stop>) -> String {
    stop.map(|s| s.name).unwrap_or_default()
}

fn press<S>(h: &mut Harness<'_, S>, p: Press) -> String {
    name(walk::press(h, p))
}

#[test]
fn the_graph_is_one_stop() {
    let mut h = page(TabStops::One);
    let walked = walk::walk(&mut h, TAB, 20);
    assert_eq!(walked.names(), ["Before", "Pipeline", "After"], "{walked}");
}

#[test]
fn every_part_is_a_stop_otherwise() {
    let mut h = page(TabStops::Each);
    let walked = walk::walk(&mut h, TAB, 40);
    assert!(walked.stops.len() > 8, "{walked}");
}

#[test]
fn tab_comes_back_to_the_node_focused_last() {
    let mut h = page(TabStops::One);
    walk::reach(&mut h, TAB, "Pipeline", 4).expect("the canvas");
    // Into the graph: the node nearest the view's centre, then that way.
    assert_eq!(press(&mut h, Press::key(Key::ArrowRight)), "Blur");
    assert_eq!(press(&mut h, Press::key(Key::ArrowRight)), "Save");
    assert_eq!(press(&mut h, TAB), "After");
    assert_eq!(press(&mut h, SHIFT_TAB), "Save");
    assert_eq!(press(&mut h, SHIFT_TAB), "Before");
    assert_eq!(press(&mut h, TAB), "Save");
}

#[test]
fn enter_goes_into_a_nodes_parts_and_escape_back() {
    let mut h = page(TabStops::One);
    walk::reach(&mut h, TAB, "Pipeline", 4).expect("the canvas");
    assert_eq!(press(&mut h, Press::key(Key::ArrowRight)), "Blur");
    let first = press(&mut h, Press::key(Key::Enter));
    assert_ne!(first, "Blur");
    // Tab goes round the node's parts, never out of it.
    let walked = walk::walk(&mut h, TAB, 20);
    assert!(matches!(walked.end, End::Looped { .. }), "{walked}");
    let names = walked.names();
    assert!(names.contains(&"Blur, input Image"), "{walked}");
    assert!(names.contains(&"Radius"), "{walked}");
    assert!(
        !names.contains(&"After") && !names.contains(&"Load"),
        "{walked}"
    );
    // Escape from a slider, as from a port, goes back to the node.
    walk::reach(&mut h, TAB, "Radius", 8).expect("the slider");
    assert_eq!(press(&mut h, ESCAPE), "Blur");
    // And Tab leaves the graph from there.
    assert_eq!(press(&mut h, TAB), "After");
}
