//! A node editor's edits run their `graph.*` steps (a veto, a
//! redirect), and agents read and edit a graph through commands.

use egui_pop_api::steps::Before;
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_imaging::graph::{
    CMD_GRAPH_EDIT, CMD_GRAPH_READ, CMD_GRAPH_SELECT, STEP_GRAPH_ADD, STEP_GRAPH_CONNECT,
};
use egui_pop_imaging::{IMAGING_PLUGIN, ImagingPlugin};
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::PluginId;
use egui_pop_viewport::change::{Cause, Verdict};
use egui_pop_viewport::graph::{
    Edge, Edit, GraphEditor, GraphEvent, GraphModel, GraphOutput, GraphState, NewNode, Node, Port,
    PortRef, PortType,
};
use serde_json::json;

/// Refuses wires into the `locked` node, and adds a `gaussian` where a
/// `blur` is asked for.
struct Rules;

impl Plugin for Rules {
    fn id(&self) -> PluginId {
        PluginId::from_static("rules")
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![IMAGING_PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.before(STEP_GRAPH_CONNECT, |_, args| {
            if args["to"]["node"] == "locked" {
                Before::Veto("locked".into())
            } else {
                Before::Go
            }
        });
        ctx.before(STEP_GRAPH_ADD, |_, args| {
            let mut args = args.clone();
            for n in args["nodes"].as_array_mut().into_iter().flatten() {
                if n["kind"] == "blur" {
                    n["kind"] = json!("gaussian");
                }
            }
            Before::Redirect(args)
        });
        Ok(())
    }
}

fn port(node: &str, port: &str) -> PortRef {
    PortRef::new(node, port)
}

#[test]
fn edits_run_their_steps() {
    let app = App::builder()
        .plugin(ImagingPlugin)
        .plugin(Rules)
        .build(&egui::Context::default())
        .unwrap();
    let h = app.handle();
    assert!(h.diagnostics().is_empty(), "{:?}", h.diagnostics());
    let mut hook = egui_pop_imaging::graph::graph(h, "main");
    let connect = |to: &str| Edit::Connect {
        from: port("load", "image"),
        to: port(to, "image"),
        replaces: Vec::new(),
    };
    assert_eq!(hook(&connect("blur"), Cause::Drag), Verdict::Allow);
    let trace = h.step_traces().pop().unwrap();
    assert_eq!(trace.step, STEP_GRAPH_CONNECT);
    assert_eq!(trace.asked["graph"], "main");
    assert_eq!(trace.asked["cause"], "drag");
    assert_eq!(
        trace.asked["to"],
        json!({ "node": "blur", "port": "image" })
    );
    assert_eq!(hook(&connect("locked"), Cause::Drag), Verdict::Veto);

    let add = Edit::Add {
        nodes: vec![NewNode {
            kind: "blur".into(),
            at: [10.0, 20.0],
            copy_of: None,
        }],
    };
    match hook(&add, Cause::Command) {
        Verdict::Redirect(Edit::Add { nodes }) => {
            assert_eq!(nodes[0].kind, "gaussian");
            assert_eq!(nodes[0].at, [10.0, 20.0]);
        }
        v => panic!("{v:?}"),
    }

    // A parameter's slider is not a step.
    let runs = h.step_traces().len();
    let param = Edit::Param {
        node: "blur".into(),
        param: "radius".into(),
        from: 1.0,
        to: 2.0,
        done: true,
    };
    assert_eq!(hook(&param, Cause::Drag), Verdict::Allow);
    assert_eq!(h.step_traces().len(), runs);
}

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

/// One frame of the editor, shown as part of the application.
fn frame(app: &App, model: &Pipeline, state: &mut GraphState) -> GraphOutput {
    let ctx = egui::Context::default();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(800.0, 600.0),
        )),
        ..Default::default()
    };
    let mut out = None;
    let _ = ctx.run_ui(input, |ui| {
        out = Some(egui_pop_imaging::show_graph(
            app.handle(),
            "main",
            GraphEditor::new(model, &mut *state),
            ui,
        ));
    });
    out.expect("drawn")
}

#[test]
fn agents_read_and_edit_a_graph_through_commands() {
    let mut app = App::builder()
        .plugin(ImagingPlugin)
        .build(&egui::Context::default())
        .unwrap();
    let image = || PortType::new("image");
    let mut model = Pipeline {
        nodes: vec![
            Node::new("load", "load", "Load", [0.0, 0.0]).output(Port::new(
                "image",
                "Image",
                image(),
            )),
            Node::new("blur", "blur", "Blur", [260.0, 0.0])
                .input(Port::new("image", "Image", image()))
                .input(Port::new("mask", "Mask", PortType::new("mask"))),
        ],
        edges: Vec::new(),
    };
    let mut state = GraphState::new();
    let agent = || Origin::Agent("test".into());
    frame(&app, &model, &mut state);

    let read = app
        .run(&CMD_GRAPH_READ, json!({}), agent())
        .now()
        .unwrap()
        .unwrap();
    assert_eq!(read["graph"], "main");
    assert_eq!(read["nodes"].as_array().map(Vec::len), Some(2));
    assert_eq!(read["nodes"][1]["inputs"][1]["type"], "mask");

    let queued = app
        .run(
            &CMD_GRAPH_EDIT,
            json!({
                "edit": "connect",
                "from": { "node": "load", "port": "image" },
                "to": { "node": "blur", "port": "image" },
            }),
            agent(),
        )
        .now()
        .unwrap()
        .unwrap();
    assert_eq!(queued["queued"], "connect");
    let out = frame(&app, &model, &mut state);
    let wire = Edge::new(port("load", "image"), port("blur", "image"));
    assert_eq!(
        out.edits,
        vec![Edit::Connect {
            from: wire.from.clone(),
            to: wire.to.clone(),
            replaces: Vec::new(),
        }]
    );
    let trace = app.handle().step_traces().pop().unwrap();
    assert_eq!(trace.step, STEP_GRAPH_CONNECT);
    assert_eq!(trace.asked["cause"], "command");
    model.edges.push(wire);

    // A wire of another type is refused before any step.
    let runs = app.handle().step_traces().len();
    let _ = app.run(
        &CMD_GRAPH_EDIT,
        json!({
            "edit": "connect",
            "from": { "node": "load", "port": "image" },
            "to": { "node": "blur", "port": "mask" },
        }),
        agent(),
    );
    let out = frame(&app, &model, &mut state);
    assert!(out.edits.is_empty());
    assert!(
        out.events
            .iter()
            .any(|e| matches!(e, GraphEvent::Refused { .. })),
        "{:?}",
        out.events
    );
    assert_eq!(app.handle().step_traces().len(), runs);

    let _ = app.run(&CMD_GRAPH_SELECT, json!({ "nodes": ["blur"] }), agent());
    let out = frame(&app, &model, &mut state);
    assert!(state.selection.contains("blur"));
    assert!(
        out.events
            .iter()
            .any(|e| matches!(e, GraphEvent::Selection(_)))
    );
    let read = app
        .run(&CMD_GRAPH_READ, json!({ "graph": "main" }), agent())
        .now()
        .unwrap()
        .unwrap();
    assert_eq!(read["selection"], json!(["blur"]));
    assert_eq!(read["edges"].as_array().map(Vec::len), Some(1));

    // Unknown edits are refused by the command itself.
    let bad = app
        .run(&CMD_GRAPH_EDIT, json!({ "edit": "explode" }), agent())
        .now()
        .unwrap();
    assert!(bad.is_err(), "{bad:?}");
}
