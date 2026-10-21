//! The node graph editor in an application: each edit runs as a step,
//! what the editor reports goes on the journal, and agents read and
//! edit a graph through commands.
//!
//! | Step | Arguments | Default |
//! |---|---|---|
//! | `graph.connect` | `graph`, `from`, `to` (`{node, port}`), `replaces` (`[{from, to}]`), `cause` | hands the edit to the application |
//! | `graph.disconnect` | `graph`, `edges` (`[{from, to}]`), `cause` | hands the edit to the application |
//! | `graph.move` | `graph`, `moves` (`[{node, from: [x, y], to: [x, y]}]`), `cause` | hands the edit to the application |
//! | `graph.delete` | `graph`, `nodes`, `edges`, `cause` | hands the edit to the application |
//! | `graph.add` | `graph`, `nodes` (`[{kind, at: [x, y], copy_of}]`), `cause` | hands the edit to the application |
//!
//! A before-hook vetoes the edit (the editor drops it) or redirects it
//! (other arguments: another place, another kind); a run-hook's result
//! is the edit handed back. A parameter's slider is not a step: its
//! changes go through as they are.
//!
//! Events on the journal: `graph.selection_changed` (`graph`, `nodes`),
//! `graph.level_changed` (`graph`, `level`), `graph.refused` (`graph`,
//! `from`, `to`, `reason`).
//!
//! Commands, for agents (`graph` names an editor shown with
//! [`show_graph`]; without it, the only one): `graph.read` (the graph
//! as drawn), `graph.edit` (an edit, done at the editor's next frame as
//! the user's: type-checked, then its step), `graph.select`,
//! `graph.view` (fit, a level, a node to focus) and `graph.export` (the
//! graph as an SVG image, from the copy the editor kept at its last
//! frame).

use std::collections::BTreeMap;
use std::sync::Mutex;

use egui_pop_api::services::ServiceToken;
use egui_pop_api::steps::{Outcome, StepSpec};
use egui_pop_api::{AppHandle, Call, Ctx, CtxError};
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::ids::{CommandId, StepId};
use egui_pop_viewport::change::{Cause, Verdict};
use egui_pop_viewport::graph::{
    Edit, ExportOptions, GraphEditor, GraphEvent, GraphExport, GraphOutput, Level, Request,
    WireStyle,
};
use serde_json::{Value as Json, json};

pub const STEP_GRAPH_CONNECT: StepId = StepId::from_static("graph.connect");
pub const STEP_GRAPH_DISCONNECT: StepId = StepId::from_static("graph.disconnect");
pub const STEP_GRAPH_MOVE: StepId = StepId::from_static("graph.move");
pub const STEP_GRAPH_DELETE: StepId = StepId::from_static("graph.delete");
pub const STEP_GRAPH_ADD: StepId = StepId::from_static("graph.add");

/// Events published on the journal (not steps: nothing to veto).
pub const EVENT_GRAPH_SELECTION: &str = "graph.selection_changed";
pub const EVENT_GRAPH_LEVEL: &str = "graph.level_changed";
pub const EVENT_GRAPH_REFUSED: &str = "graph.refused";

pub const CMD_GRAPH_READ: CommandId = CommandId::from_static("graph.read");
pub const CMD_GRAPH_EDIT: CommandId = CommandId::from_static("graph.edit");
pub const CMD_GRAPH_SELECT: CommandId = CommandId::from_static("graph.select");
pub const CMD_GRAPH_VIEW: CommandId = CommandId::from_static("graph.view");
pub const CMD_GRAPH_EXPORT: CommandId = CommandId::from_static("graph.export");

/// The node editors shown with an application, by name: their last
/// snapshot and the requests waiting for their next frame.
#[derive(Debug, Default)]
pub struct Graphs {
    inner: Mutex<BTreeMap<String, Entry>>,
}

#[derive(Debug, Default)]
struct Entry {
    snapshot: Json,
    queue: Vec<Request>,
    /// The graph as drawn last, to export.
    export: Option<GraphExport>,
}

/// The registry, a service of [`ImagingPlugin`](crate::ImagingPlugin).
pub const GRAPHS: ServiceToken<Graphs> = ServiceToken::new("egui_pop.graphs");

impl Graphs {
    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, Entry>> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The editors' names.
    pub fn names(&self) -> Vec<String> {
        self.lock().keys().cloned().collect()
    }

    /// An editor's graph as it drew it last.
    pub fn snapshot(&self, name: &str) -> Option<Json> {
        self.lock().get(name).map(|e| e.snapshot.clone())
    }

    /// Asks an editor for something at its next frame.
    pub fn request(&self, name: &str, request: Request) {
        self.lock()
            .entry(name.to_string())
            .or_default()
            .queue
            .push(request);
    }

    fn take(&self, name: &str) -> Vec<Request> {
        self.lock()
            .get_mut(name)
            .map(|e| std::mem::take(&mut e.queue))
            .unwrap_or_default()
    }

    /// An editor's graph as it drew it last, ready to export.
    pub fn export(&self, name: &str) -> Option<GraphExport> {
        self.lock().get(name).and_then(|e| e.export.clone())
    }

    fn publish(&self, name: &str, snapshot: Json, export: Option<GraphExport>) {
        let mut all = self.lock();
        let e = all.entry(name.to_string()).or_default();
        e.snapshot = snapshot;
        if export.is_some() {
            e.export = export;
        }
    }
}

/// The step of an edit (`None`: a parameter, not a step).
pub fn step_of(edit: &Edit) -> Option<StepId> {
    match edit {
        Edit::Connect { .. } => Some(STEP_GRAPH_CONNECT),
        Edit::Disconnect { .. } => Some(STEP_GRAPH_DISCONNECT),
        Edit::Move { .. } => Some(STEP_GRAPH_MOVE),
        Edit::Delete { .. } => Some(STEP_GRAPH_DELETE),
        Edit::Add { .. } => Some(STEP_GRAPH_ADD),
        Edit::Param { .. } => None,
    }
}

/// A node editor hook ([`GraphEditor::on_edit`]) running each edit as
/// its `graph.*` step, the editor named `name`.
pub fn graph<'a>(
    app: &'a AppHandle,
    name: &'a str,
) -> impl FnMut(&Edit, Cause) -> Verdict<Edit> + 'a {
    move |edit: &Edit, cause: Cause| {
        let Some(step) = step_of(edit) else {
            return Verdict::Allow;
        };
        let mut args = edit.to_json();
        args["graph"] = json!(name);
        args["cause"] = json!(cause.name());
        match app.step(&step, args, |_, args| Ok(args.clone())) {
            Outcome::Done(Ok(v)) => match Edit::from_json(edit.kind(), &v) {
                Some(e) if e == *edit => Verdict::Allow,
                Some(e) => Verdict::Redirect(e),
                None => Verdict::Veto,
            },
            Outcome::Done(Err(_)) | Outcome::Vetoed { .. } => Verdict::Veto,
        }
    }
}

/// Shows a node editor as part of the application: its edits run their
/// steps, its events go on the journal, agents' requests reach it, and
/// `graph.read` and `graph.export` see what it drew. `name` names it to
/// steps and commands.
pub fn show_graph(
    app: &AppHandle,
    name: &str,
    editor: GraphEditor<'_>,
    ui: &mut egui::Ui,
) -> GraphOutput {
    let graphs = app.service(&GRAPHS);
    let queued = graphs.as_ref().map(|g| g.take(name)).unwrap_or_default();
    let out = editor
        .requests(queued)
        .on_edit(graph(app, name))
        .describe(graphs.is_some())
        .keep_export(graphs.is_some())
        .show(ui);
    for e in &out.events {
        let (event, data) = match e {
            GraphEvent::Selection(nodes) => (
                EVENT_GRAPH_SELECTION,
                json!({ "graph": name, "nodes": nodes }),
            ),
            GraphEvent::Level(level) => (
                EVENT_GRAPH_LEVEL,
                json!({ "graph": name, "level": level.name() }),
            ),
            GraphEvent::Refused { from, to, reason } => (
                EVENT_GRAPH_REFUSED,
                json!({ "graph": name, "from": from.to_json(), "to": to.to_json(), "reason": reason }),
            ),
        };
        app.publish("graph", None, event, data);
    }
    if let (Some(g), Some(s)) = (graphs, out.snapshot.as_ref()) {
        g.publish(name, s.clone(), out.export.clone());
    }
    out
}

/// The steps, as [`ImagingPlugin`](crate::ImagingPlugin) declares them.
pub fn specs() -> Vec<StepSpec> {
    let p = |n: &str, q: &str| json!({ "node": n, "port": q });
    vec![
        StepSpec::new(
            STEP_GRAPH_CONNECT,
            "A node editor connects an output to an input (`graph`, `from`, `to`: {node, port}, `replaces`: the wires it takes the place of, `cause`); a veto drops it, a redirect or a run-hook gives another connection.",
        )
        .args(json!({
            "graph": "main",
            "from": p("load", "image"),
            "to": p("blur", "image"),
            "replaces": [],
            "cause": "drag",
        })),
        StepSpec::new(
            STEP_GRAPH_DISCONNECT,
            "A node editor removes wires (`graph`, `edges`: [{from, to}], `cause`); a veto keeps them.",
        )
        .args(json!({
            "graph": "main",
            "edges": [{ "from": p("load", "image"), "to": p("blur", "image") }],
            "cause": "drag",
        })),
        StepSpec::new(
            STEP_GRAPH_MOVE,
            "A node editor moves nodes at the end of a drag or a key (`graph`, `moves`: [{node, from, to}], `cause`); a veto leaves them, a redirect gives other places.",
        )
        .args(json!({
            "graph": "main",
            "moves": [{ "node": "blur", "from": [200.0, 40.0], "to": [240.0, 60.0] }],
            "cause": "drag",
        })),
        StepSpec::new(
            STEP_GRAPH_DELETE,
            "A node editor deletes nodes and the wires touching them (`graph`, `nodes`, `edges`, `cause`); a veto keeps them.",
        )
        .args(json!({
            "graph": "main",
            "nodes": ["blur"],
            "edges": [{ "from": p("load", "image"), "to": p("blur", "image") }],
            "cause": "key",
        })),
        StepSpec::new(
            STEP_GRAPH_ADD,
            "A node editor adds nodes, from its menu, a command or a duplicate (`graph`, `nodes`: [{kind, at, copy_of}], `cause`); a veto adds nothing, a redirect gives other nodes.",
        )
        .args(json!({
            "graph": "main",
            "nodes": [{ "kind": "blur", "at": [400.0, 80.0], "copy_of": null }],
            "cause": "command",
        })),
    ]
}

/// The editor a command names, or the only one.
fn target(call: &Call<'_>) -> Result<(std::sync::Arc<Graphs>, String), String> {
    let graphs = call
        .app
        .service(&GRAPHS)
        .ok_or("the imaging plugin is not connected")?;
    let name = match call.args.get("graph").and_then(Json::as_str) {
        Some(n) => n.to_string(),
        None => {
            let names = graphs.names();
            match names.as_slice() {
                [one] => one.clone(),
                [] => return Err("no node editor has been shown".into()),
                _ => return Err(format!("name one with `graph`: {}", names.join(", "))),
            }
        }
    };
    if graphs.snapshot(&name).is_none() {
        return Err(format!("no node editor `{name}`"));
    }
    Ok((graphs, name))
}

/// Declares the service and the agents' commands.
pub(crate) fn connect(ctx: &mut Ctx<'_>) -> Result<(), CtxError> {
    const CAT: &str = "Node graph";
    ctx.service(&GRAPHS, Graphs::default())?;
    ctx.command(
        CommandSpec::new(CMD_GRAPH_READ, "Read the graph", CAT)
            .description("The graph a node editor drew last: nodes (place, ports, group, parameters), wires, groups, the kinds it can add, the selection, the level and the part in view.")
            .hidden()
            .args(json!({ "graph": "main" })),
        |call| {
            let (graphs, name) = target(call)?;
            let mut out = graphs.snapshot(&name).unwrap_or_default();
            out["graph"] = json!(name);
            Ok(out)
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_GRAPH_EDIT, "Edit the graph", CAT)
            .description("Asks a node editor for an edit, done at its next frame as the user's (a connection type-checked, then its graph.* step): `edit` is `connect` (`from`, `to`), `disconnect` (`edges`), `move` (`moves`), `delete` (`nodes`, `edges`) or `add` (`nodes`).")
            .hidden()
            .args(json!({
                "graph": "main",
                "edit": "connect",
                "from": { "node": "load", "port": "image" },
                "to": { "node": "blur", "port": "image" },
            })),
        |call| {
            let (graphs, name) = target(call)?;
            let kind = call
                .args
                .get("edit")
                .and_then(Json::as_str)
                .ok_or("`edit` is missing")?;
            if kind == "param" {
                return Err("parameters are the application's to set".into());
            }
            let edit = Edit::from_json(kind, call.args)
                .ok_or_else(|| format!("not a `{kind}` edit: {}", call.args))?;
            graphs.request(&name, Request::Edit(edit));
            Ok(json!({ "graph": name, "queued": kind }))
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_GRAPH_SELECT, "Select nodes", CAT)
            .description("Selects nodes of a node editor (`nodes`, their ids) at its next frame.")
            .hidden()
            .args(json!({ "graph": "main", "nodes": ["blur"] })),
        |call| {
            let (graphs, name) = target(call)?;
            let nodes = call
                .args
                .get("nodes")
                .and_then(Json::as_array)
                .ok_or("`nodes` is missing")?
                .iter()
                .filter_map(|n| n.as_str().map(str::to_string))
                .collect();
            graphs.request(&name, Request::Select(nodes));
            Ok(json!({ "graph": name, "queued": "select" }))
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_GRAPH_VIEW, "Change the graph's view", CAT)
            .description("Fits a node editor's graph (`fit`: true), zooms to a level (`level`: overview, ports or detail) or focuses a node (`focus`), at its next frame.")
            .hidden()
            .args(json!({ "graph": "main", "level": "detail" })),
        |call| {
            let (graphs, name) = target(call)?;
            let a = call.args;
            let request = if a.get("fit").and_then(Json::as_bool) == Some(true) {
                Request::Fit
            } else if let Some(l) = a.get("level").and_then(Json::as_str) {
                Request::Level(level_of(l)?)
            } else if let Some(n) = a.get("focus").and_then(Json::as_str) {
                Request::Focus(n.to_string())
            } else {
                return Err("give `fit`, `level` or `focus`".into());
            };
            graphs.request(&name, request);
            Ok(json!({ "graph": name, "queued": "view" }))
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_GRAPH_EXPORT, "Export the graph", CAT)
            .description("A node editor's graph as an SVG image, drawn from the graph and its layout as the editor drew it last: `selection` (true: only the selected nodes), `level` (overview, ports or detail; ports by default), `wires` (curves, orthogonal or straight; the editor's by default), `background` (false: transparent), `title`. Gives `svg`.")
            .hidden()
            .args(json!({ "graph": "main", "selection": false, "level": "ports" })),
        |call| {
            let (graphs, name) = target(call)?;
            let export = graphs
                .export(&name)
                .ok_or_else(|| format!("no graph to export from `{name}` yet"))?;
            let (export, options) = export_args(export, call.args)?;
            let svg = export.svg(&options);
            Ok(json!({ "graph": name, "format": "svg", "svg": svg }))
        },
    )?;
    Ok(())
}

/// A level by its name.
fn level_of(name: &str) -> Result<Level, String> {
    match name {
        "overview" => Ok(Level::Overview),
        "ports" => Ok(Level::Ports),
        "detail" => Ok(Level::Detail),
        _ => Err(format!("no level `{name}`")),
    }
}

/// `graph.export`'s arguments: the options, and the wire style.
fn export_args(export: GraphExport, a: &Json) -> Result<(GraphExport, ExportOptions), String> {
    let mut options = ExportOptions::default();
    if let Some(on) = a.get("selection").and_then(Json::as_bool) {
        options = options.selection(on);
    }
    if let Some(l) = a.get("level").and_then(Json::as_str) {
        options = options.level(level_of(l)?);
    }
    if let Some(on) = a.get("background").and_then(Json::as_bool) {
        options = options.background(on);
    }
    if let Some(t) = a.get("title").and_then(Json::as_str) {
        options = options.title(t);
    }
    let export = match a.get("wires").and_then(Json::as_str) {
        Some(w) => {
            export.wires(WireStyle::from_name(w).ok_or_else(|| format!("no wire style `{w}`"))?)
        }
        None => export,
    };
    Ok((export, options))
}
