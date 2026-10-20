//! The node graph page: a synthetic processing graph of fifty nodes
//! (six sources through tone, colour and detail stages, masks and
//! blends to one output), shown with the node editor and its minimap.
//! The page keeps the graph and applies the editor's edits; each runs
//! its `graph.*` step, and the page's plugin refuses to delete the
//! output.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use egui::{Sense, Ui, vec2};
use egui_pop::api::steps::Before;
use egui_pop::api::stories::Story;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::core::widgets;
use egui_pop::imaging::{self, graph::STEP_GRAPH_DELETE};
use egui_pop::model::ids::{PluginId, StoryId};
use egui_pop::theme::Tokens;
use egui_pop::viewport::graph::{
    Edge, Edit, GraphEditor, GraphMinimap, GraphModel, GraphState, Group, Kind, Level, Node, Param,
    Port, PortRef, PortType, Request,
};

use crate::shell::SHELL;

pub const NODE_GRAPH: PluginId = PluginId::from_static("gallery.node-graph");

/// The name the editor goes by in steps and commands.
pub const GRAPH: &str = "gallery";

/// A parameter: id, label, default, minimum, maximum, step, unit.
type ParamDef = (&'static str, &'static str, f64, f64, f64, f64, &'static str);

/// A kind of node: its ports and parameters.
struct KindDef {
    id: &'static str,
    title: &'static str,
    /// Inputs: id, label, type.
    inputs: &'static [(&'static str, &'static str, &'static str)],
    outputs: &'static [(&'static str, &'static str, &'static str)],
    /// Parameters: id, label, default, minimum, maximum, step, unit.
    params: &'static [ParamDef],
}

const IMAGE_IN: &[(&str, &str, &str)] = &[("image", "Image", "image")];
const IMAGE_OUT: &[(&str, &str, &str)] = &[("image", "Image", "image")];

const KINDS: &[KindDef] = &[
    KindDef {
        id: "load",
        title: "Load",
        inputs: &[],
        outputs: IMAGE_OUT,
        params: &[],
    },
    KindDef {
        id: "exposure",
        title: "Exposure",
        inputs: IMAGE_IN,
        outputs: IMAGE_OUT,
        params: &[("exposure", "Exposure", 0.0, -3.0, 4.0, 0.01, "EV")],
    },
    KindDef {
        id: "curve",
        title: "Tone curve",
        inputs: IMAGE_IN,
        outputs: IMAGE_OUT,
        params: &[("contrast", "Contrast", 0.2, 0.0, 1.0, 0.01, "")],
    },
    KindDef {
        id: "saturation",
        title: "Saturation",
        inputs: IMAGE_IN,
        outputs: IMAGE_OUT,
        params: &[("saturation", "Saturation", 0.0, -1.0, 1.0, 0.01, "")],
    },
    KindDef {
        id: "balance",
        title: "Colour balance",
        inputs: IMAGE_IN,
        outputs: IMAGE_OUT,
        params: &[("warmth", "Warmth", 0.0, -1.0, 1.0, 0.01, "")],
    },
    KindDef {
        id: "blur",
        title: "Blur",
        inputs: IMAGE_IN,
        outputs: IMAGE_OUT,
        params: &[("radius", "Radius", 2.0, 0.0, 20.0, 0.1, "px")],
    },
    KindDef {
        id: "sharpen",
        title: "Sharpen",
        inputs: IMAGE_IN,
        outputs: IMAGE_OUT,
        params: &[
            ("amount", "Amount", 0.5, 0.0, 2.0, 0.01, ""),
            ("radius", "Radius", 1.0, 0.1, 5.0, 0.1, "px"),
        ],
    },
    KindDef {
        id: "mask",
        title: "Luminance mask",
        inputs: IMAGE_IN,
        outputs: &[("mask", "Mask", "mask")],
        params: &[("threshold", "Threshold", 0.5, 0.0, 1.0, 0.01, "")],
    },
    KindDef {
        id: "mix",
        title: "Blend",
        inputs: &[
            ("base", "Base", "image"),
            ("top", "Top", "image"),
            ("mask", "Mask", "mask"),
        ],
        outputs: IMAGE_OUT,
        params: &[("opacity", "Opacity", 1.0, 0.0, 1.0, 0.01, "")],
    },
    KindDef {
        id: "output",
        title: "Output",
        inputs: IMAGE_IN,
        outputs: &[],
        params: &[],
    },
];

fn kind(id: &str) -> Option<&'static KindDef> {
    KINDS.iter().find(|k| k.id == id)
}

/// The page's graph: what the application keeps.
#[derive(Debug, Default)]
pub struct Pipeline {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    /// Parameter values by node and parameter.
    pub values: BTreeMap<(String, String), f64>,
    next: usize,
}

impl Pipeline {
    /// A node of a kind, its parameters at their defaults.
    fn add(&mut self, kind_id: &str, id: &str, title: &str, at: [f32; 2], group: Option<&str>) {
        let Some(k) = kind(kind_id) else { return };
        let ty = |t: &str| PortType::new(t);
        let mut n = Node::new(id, kind_id, title, at);
        for &(pid, label, t) in k.inputs {
            let mut p = Port::new(pid, label, ty(t));
            if kind_id == "output" {
                p = p.multiple();
            }
            n = n.input(p);
        }
        for &(pid, label, t) in k.outputs {
            n = n.output(Port::new(pid, label, ty(t)));
        }
        if let Some(g) = group {
            n = n.group(g);
        }
        for p in k.params {
            self.values.insert((id.to_string(), p.0.to_string()), p.2);
        }
        self.nodes.push(n);
    }

    fn wire(&mut self, from: (&str, &str), to: (&str, &str)) {
        self.edges.push(Edge::new(
            PortRef::new(from.0, from.1),
            PortRef::new(to.0, to.1),
        ));
    }

    /// Fifty nodes: six sources, each through six stages, three masks
    /// and blends, a last blend and the output.
    pub fn synthetic() -> Pipeline {
        let mut p = Pipeline::default();
        let stages = [
            ("exposure", Some("tone")),
            ("curve", Some("tone")),
            ("saturation", None),
            ("balance", None),
            ("blur", Some("detail")),
            ("sharpen", Some("detail")),
        ];
        let col = 240.0;
        let row = 130.0;
        for r in 0..6 {
            let y = r as f32 * row;
            let load = format!("load-{}", r + 1);
            p.add(
                "load",
                &load,
                &format!("Load {}", r + 1),
                [0.0, y],
                Some("sources"),
            );
            let mut prev = load;
            for (c, (k, group)) in stages.iter().enumerate() {
                let id = format!("{k}-{}", r + 1);
                let title = format!("{} {}", kind(k).map_or("", |d| d.title), r + 1);
                p.add(k, &id, &title, [(c + 1) as f32 * col, y], *group);
                p.wire((&prev, "image"), (&id, "image"));
                prev = id;
            }
        }
        for m in 0..3 {
            let y = (2 * m) as f32 * row + row / 2.0;
            let (a, b) = (
                format!("sharpen-{}", 2 * m + 1),
                format!("sharpen-{}", 2 * m + 2),
            );
            let mask = format!("mask-{}", m + 1);
            let mix = format!("mix-{}", m + 1);
            p.add(
                "mask",
                &mask,
                &format!("Luminance mask {}", m + 1),
                [7.0 * col, y - row * 0.45],
                Some("blend"),
            );
            p.add(
                "mix",
                &mix,
                &format!("Blend {}", m + 1),
                [8.0 * col, y],
                Some("blend"),
            );
            p.wire((&a, "image"), (&mask, "image"));
            p.wire((&a, "image"), (&mix, "base"));
            p.wire((&b, "image"), (&mix, "top"));
            p.wire((&mask, "mask"), (&mix, "mask"));
        }
        p.add(
            "mix",
            "mix-final",
            "Final blend",
            [9.0 * col, 2.0 * row],
            None,
        );
        p.wire(("mix-1", "image"), ("mix-final", "base"));
        p.wire(("mix-2", "image"), ("mix-final", "top"));
        p.add("output", "output", "Output", [10.0 * col, 2.0 * row], None);
        p.wire(("mix-final", "image"), ("output", "image"));
        p.next = p.nodes.len();
        p
    }

    /// Applies an edit the editor handed back.
    pub fn apply(&mut self, e: &Edit) {
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
                self.values.retain(|(n, _), _| !nodes.contains(n));
            }
            Edit::Add { nodes } => {
                for new in nodes {
                    self.next += 1;
                    let Some(k) = kind(&new.kind) else { continue };
                    let id = format!("{}-{}", k.id, self.next);
                    let title = format!("{} {}", k.title, self.next);
                    let group = new
                        .copy_of
                        .as_ref()
                        .and_then(|c| self.nodes.iter().find(|n| n.id == *c))
                        .and_then(|n| n.group.clone());
                    self.add(k.id, &id, &title, new.at, group.as_deref());
                    if let Some(c) = &new.copy_of {
                        for p in k.params {
                            if let Some(v) = self.values.get(&(c.clone(), p.0.to_string())) {
                                self.values.insert((id.clone(), p.0.to_string()), *v);
                            }
                        }
                    }
                }
            }
            Edit::Param {
                node, param, to, ..
            } => {
                self.values.insert((node.clone(), param.clone()), *to);
            }
        }
    }
}

impl GraphModel for Pipeline {
    fn nodes(&self) -> Vec<Node> {
        self.nodes.clone()
    }

    fn edges(&self) -> Vec<Edge> {
        self.edges.clone()
    }

    fn groups(&self) -> Vec<Group> {
        vec![
            Group::new("sources", "Sources"),
            Group::new("tone", "Tone"),
            Group::new("detail", "Detail"),
            Group::new("blend", "Masks and blends"),
        ]
    }

    fn params(&self, node: &str) -> Vec<Param> {
        let Some(n) = self.nodes.iter().find(|n| n.id == node) else {
            return Vec::new();
        };
        let Some(k) = kind(&n.kind) else {
            return Vec::new();
        };
        k.params
            .iter()
            .map(|(id, label, default, min, max, step, unit)| {
                let v = self
                    .values
                    .get(&(node.to_string(), id.to_string()))
                    .copied()
                    .unwrap_or(*default);
                let mut p = Param::new(*id, *label, v, *min..=*max)
                    .step(*step)
                    .default(*default);
                if !unit.is_empty() {
                    p = p.unit(*unit);
                }
                p
            })
            .collect()
    }

    fn kinds(&self) -> Vec<Kind> {
        KINDS.iter().map(|k| Kind::new(k.id, k.title)).collect()
    }
}

/// The page's state.
#[derive(Debug)]
pub struct State {
    pub graph: Pipeline,
    pub view: GraphState,
    pub snap: bool,
    level: Option<Level>,
}

impl Default for State {
    fn default() -> Self {
        State {
            graph: Pipeline::synthetic(),
            view: GraphState::new(),
            snap: true,
            level: None,
        }
    }
}

/// The page.
#[derive(Debug, Default)]
pub struct NodeGraphPage;

impl Plugin for NodeGraphPage {
    fn id(&self) -> PluginId {
        NODE_GRAPH
    }

    fn title(&self) -> String {
        "Node graph editor".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL, imaging::IMAGING_PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        // A before-hook: the output stays.
        ctx.before(STEP_GRAPH_DELETE, |_, args| {
            let output = args["nodes"]
                .as_array()
                .is_some_and(|n| n.iter().any(|n| n == "output"));
            if args["graph"] == GRAPH && output {
                Before::Veto("the output stays".into())
            } else {
                Before::Go
            }
        });
        let state = Rc::new(RefCell::new(State::default()));
        let play = state.clone();
        ctx.story(
            Story::new(StoryId::from_static("node-graph"), "Node graph editor", move |ui, app| {
                show(ui, app, &mut state.borrow_mut())
            })
            .description("A node graph over fifty nodes: typed ports and wires, groups folded into stacks, an overview and a detail view with inline sliders, a minimap; each edit a graph.* step.")
            .group("Components", 78)
            .playground(move |ui, _| playground(ui, &mut play.borrow_mut())),
        )?;
        Ok(())
    }
}

/// The page.
pub fn show(ui: &mut Ui, app: &AppHandle, s: &mut State) {
    ui.label("Wheel to pan, Ctrl and the wheel to zoom: zoomed out the overview folds groups into stacks, zoomed in the detail shows each node's parameters. Drag from a port to wire (types are checked), drag nodes, drag the canvas for a rubber band; Delete, Ctrl+D, Ctrl+A. The page's plugin refuses to delete the output.");
    let mini = vec2(200.0, 110.0);
    let width = ui.available_width().max(320.0);
    let height = (ui.available_height() - mini.y - 12.0).max(260.0);
    let success = Tokens::of(ui.ctx()).status.success;
    let editor = GraphEditor::new(&s.graph, &mut s.view)
        .label("Processing graph")
        .size(vec2(width, height))
        .snap(s.snap.then_some(10.0))
        .header(move |ui, _node| {
            // The application's slot: a status light.
            let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
            ui.painter().circle_filled(r.center(), 3.5, success);
        });
    let out = imaging::show_graph(app, GRAPH, editor, ui);
    for e in &out.edits {
        s.graph.apply(e);
    }
    s.level = Some(out.level);
    ui.add_space(4.0);
    ui.horizontal_top(|ui| {
        GraphMinimap::new(&mut s.view).size(mini).show(ui);
        ui.vertical(|ui| {
            ui.label(format!(
                "{} nodes, {} wires, {} view",
                s.graph.nodes.len(),
                s.graph.edges.len(),
                out.level.name()
            ));
            let selected: Vec<&str> = s.view.selection.iter().map(String::as_str).collect();
            if !selected.is_empty() {
                ui.label(format!("Selected: {}", selected.join(", ")));
            }
        });
    });
}

/// The playground: snapping and the views.
fn playground(ui: &mut Ui, s: &mut State) {
    widgets::checkbox(ui, &mut s.snap, "Snap to a 10-unit grid");
    ui.horizontal_wrapped(|ui| {
        if ui.button("Fit").clicked() {
            s.view.request(Request::Fit);
        }
        for (level, label) in [
            (Level::Overview, "Overview"),
            (Level::Ports, "Ports"),
            (Level::Detail, "Detail"),
        ] {
            if ui
                .add(egui::Button::new(label).selected(s.level == Some(level)))
                .clicked()
            {
                s.view.request(Request::Level(level));
            }
        }
    });
    if ui.button("Reset the graph").clicked() {
        *s = State::default();
    }
}
