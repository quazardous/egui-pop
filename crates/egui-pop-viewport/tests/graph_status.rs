//! A node's status in its title bar (`GraphEditor::status`): part of the
//! node's accessible description (its text, then its detail), its detail
//! a tooltip over it, not a node of its own; a node without one says
//! none; the application's header slot still drawn beside it.

use std::cell::Cell;

use egui::{Color32, Event, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_core::kit::Tone;
use egui_pop_viewport::graph::{
    Edge, GraphEditor, GraphModel, GraphState, Level, Node, NodeStatus, Port, PortType, Request,
    StatusColor,
};

#[derive(Debug)]
struct Pipeline {
    nodes: Vec<Node>,
}

impl GraphModel for Pipeline {
    fn nodes(&self) -> Vec<Node> {
        self.nodes.clone()
    }

    fn edges(&self) -> Vec<Edge> {
        Vec::new()
    }
}

struct State {
    graph: Pipeline,
    view: GraphState,
    header: bool,
    headers: u32,
    zoom: f32,
}

fn status(node: &Node) -> Option<NodeStatus> {
    match node.id.as_str() {
        "load" => Some(
            NodeStatus::new("error")
                .tone(Tone::Danger)
                .detail("The file is missing"),
        ),
        "blur" => Some(NodeStatus::new("stale").token("state.stale")),
        _ => None,
    }
}

fn editor(header: bool) -> Harness<'static, State> {
    let image = || PortType::new("image");
    let mut h = Harness::builder()
        .with_size(vec2(720.0, 420.0))
        .build_ui_state(
            |ui, s: &mut State| {
                let headers = Cell::new(0);
                let mut e = GraphEditor::new(&s.graph, &mut s.view)
                    .size(vec2(700.0, 400.0))
                    .status(status);
                if s.header {
                    e = e.header(|ui, _| {
                        headers.set(headers.get() + 1);
                        ui.label("slot");
                    });
                }
                let out = e.show(ui);
                s.headers += headers.get();
                s.zoom = out.zoom;
            },
            State {
                graph: Pipeline {
                    nodes: vec![
                        Node::new("load", "load", "Load", [0.0, 0.0]).output(Port::new(
                            "image",
                            "Image",
                            image(),
                        )),
                        Node::new("blur", "blur", "Blur", [260.0, 0.0]).input(Port::new(
                            "image",
                            "Image",
                            image(),
                        )),
                        Node::new("view", "view", "View", [520.0, 0.0]).input(Port::new(
                            "image",
                            "Image",
                            image(),
                        )),
                    ],
                },
                view: GraphState::new(),
                header,
                headers: 0,
                zoom: 1.0,
            },
        );
    h.run();
    h.state_mut().view.request(Request::Level(Level::Ports));
    h.run();
    h
}

#[test]
fn a_node_s_status_is_part_of_its_description() {
    let h = editor(true);
    let description = |label: &str| {
        h.get_by_label(label)
            .accesskit_node()
            .description()
            .map(|d| d.to_string())
    };
    assert_eq!(
        description("Load").as_deref(),
        Some("error, The file is missing")
    );
    assert_eq!(description("Blur").as_deref(), Some("stale"));
    assert_eq!(description("View"), None);
    // Drawn, not a node of its own.
    assert!(h.query_by_label("error").is_none());
    egui_pop_test::assert_all_named(&h);
    // The application's slot is still drawn, in each title bar.
    assert!(h.state().headers >= 3, "{}", h.state().headers);
}

#[test]
fn a_node_s_status_detail_is_a_tooltip_over_it() {
    let mut h = editor(false);
    let r = h.get_by_label("Load").rect();
    let zoom = h.state().zoom;
    // The status ends at the title bar's right end, less its padding.
    let at = egui::pos2(r.right() - 10.0 * zoom, r.top() + 14.0 * zoom);
    assert!(h.query_by_label_contains("The file is missing").is_none());
    h.event(Event::PointerMoved(at));
    h.run();
    assert!(
        h.query_by_label_contains("The file is missing").is_some(),
        "the tooltip"
    );
}

#[test]
fn a_status_colour_is_a_tone_a_colour_or_a_token() {
    let ctx = egui::Context::default();
    egui_pop_core::theme::install(&ctx, &egui_pop_core::theme::ThemeSpec::new("pop-dark"));
    let tokens = egui_pop_core::theme::Tokens::of(&ctx);
    assert_eq!(
        StatusColor::Tone(Tone::Danger).resolve(&tokens),
        tokens.status.danger
    );
    let red = Color32::from_rgb(200, 10, 10);
    assert_eq!(NodeStatus::new("x").color(red).color.resolve(&tokens), red);
    // A token the theme has, and one it lacks (muted text).
    assert_eq!(
        StatusColor::Token("success".into()).resolve(&tokens),
        tokens.status.success
    );
    assert_eq!(
        StatusColor::Token("state.nowhere".into()).resolve(&tokens),
        tokens.text_role.muted.color
    );
}
