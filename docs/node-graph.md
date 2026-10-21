# Node graph editor

`egui_pop::viewport::graph` (the facade's `viewport` feature) is a node
graph editor over a graph the **application keeps**: it describes its
nodes, typed ports, wires and groups through a trait, the editor draws
them on egui's `Scene` (pan and zoom) and hands back what the user did
as edits, which the application applies, or not. The editor never owns
the graph.

![the node graph page](../crates/egui-pop-gallery/tests/snapshots/node-graph-pop-dark.png)

## The model

```rust
use egui_pop::viewport::graph::{Edge, GraphModel, Group, Kind, Node, Param, Port, PortType};

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

    fn groups(&self) -> Vec<Group> {
        vec![Group::new("detail", "Detail")]
    }

    // Shown in the detail view, edited inline with egui-pop's sliders.
    fn params(&self, node: &str) -> Vec<Param> {
        match node {
            "blur" => vec![Param::new("radius", "Radius", self.radius, 0.0..=20.0).step(0.1).unit("px")],
            _ => Vec::new(),
        }
    }

    // What the canvas's menu (a secondary click) offers to add.
    fn kinds(&self) -> Vec<Kind> {
        vec![Kind::new("blur", "Blur")]
    }
}

let image = || PortType::new("image");
let _graph = Pipeline {
    nodes: vec![
        Node::new("load", "load", "Load", [0.0, 0.0]).output(Port::new("image", "Image", image())),
        Node::new("blur", "blur", "Blur", [240.0, 0.0])
            .input(Port::new("image", "Image", image()))
            .output(Port::new("image", "Image", image()))
            .group("detail"),
    ],
    edges: Vec::new(),
    radius: 2.0,
};
```

- **Ids** are the application's strings: a node's `id`, a port's `id`
  within its node, a group's, a kind's. Steps, commands and the debug
  bridge show them.
- **Positions** are the application's too (`Node::pos`, the top-left
  corner in graph units, points at 100 %): a drag hands back a move.
- **Types**: a `PortType` has a name and a colour token. By default the
  colour is one of `graph.type-1` to `graph.type-6`, picked from the
  name (the same name, the same colour); `.color("my.token")` takes any
  token of the theme. An output feeds any number of inputs; an input
  takes one wire (a new one replaces it) unless it is `.multiple()`.
- **Checks**: `GraphModel::can_connect` says whether an output may feed
  an input; the default (`graph::check`) wants the same type and two
  different nodes. Its `Refusal` is shown and announced.

## The editor

```rust
use egui_pop::egui;
use egui_pop::viewport::change::{Cause, Verdict};
use egui_pop::viewport::graph::{Edge, Edit, GraphEditor, GraphMinimap, GraphModel, GraphState, Node};

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

fn graph_panel(ui: &mut egui::Ui, graph: &mut Pipeline, state: &mut GraphState) {
    let out = GraphEditor::new(&*graph, state)
        .snap(Some(10.0))
        // A slot in each title bar, the application's.
        .header(|ui, _node| {
            ui.small("on");
        })
        // Sees every edit first: allow, veto, or redirect.
        .on_edit(|edit, _cause: Cause| match edit {
            Edit::Delete { nodes, .. } if nodes.iter().any(|n| n == "output") => Verdict::Veto,
            _ => Verdict::Allow,
        })
        .show(ui);
    for edit in &out.edits {
        match edit {
            Edit::Connect { from, to, replaces } => {
                graph.edges.retain(|e| !replaces.contains(e));
                graph.edges.push(Edge::new(from.clone(), to.clone()));
            }
            Edit::Disconnect { edges } => graph.edges.retain(|e| !edges.contains(e)),
            Edit::Move { moves } => {
                for m in moves {
                    if let Some(n) = graph.nodes.iter_mut().find(|n| n.id == m.node) {
                        n.pos = m.to;
                    }
                }
            }
            Edit::Delete { nodes, edges } => {
                graph.nodes.retain(|n| !nodes.contains(&n.id));
                graph.edges.retain(|e| !edges.contains(e));
            }
            Edit::Add { .. } | Edit::Param { .. } => {}
        }
    }
    // The minimap shares the editor's state, like the image navigator.
    GraphMinimap::new(state).show(ui);
}
```

`GraphState` is the view: the part of the graph in view (`view`), the
selection, the collapsed nodes and the folded groups, all for the
application to keep; `request` asks for something at the next frame
(`Request::Fit`, `Level`, `Select`, `Focus`, or an `Edit` as if the user
made it). `GraphOutput` gives the edits after the hook, the `events`
(`Selection`, `Level`, `Refused`), the `level` and the `zoom`.

### Edits

| Edit | When | Fields |
|---|---|---|
| `Connect` | a wire dropped on a port that accepts it, two ports clicked, Enter on two ports | `from` (an output), `to` (an input), `replaces`: the wires it takes the place of |
| `Disconnect` | a wire dragged off an input and dropped nowhere, Delete on a port, a port's menu | `edges` |
| `Move` | the end of a drag of nodes, a stack or a group's title; Shift and the arrows | `moves`: each node's `from` and `to` (snapped with `snap`) |
| `Delete` | Delete or Backspace, a node's menu | `nodes`, and the `edges` touching them |
| `Add` | the canvas's menu (at the pointer), Ctrl+D (copies, `copy_of`) | `nodes`: `kind`, `at`, `copy_of` |
| `Param` | a parameter's slider in the detail view | `node`, `param`, `from`, `to`, `done` (a drag's frames are not done) |

A drag never writes: the nodes follow the pointer in the editor, and
one `Move` comes at the end (Escape during the drag drops it).

### Gestures

Pan and zoom follow the pointer profile
([pointer-gestures.md](pointer-gestures.md)); the defaults (Document):

| Gesture | What it does |
|---|---|
| Wheel, two fingers | Pans (Shift and the wheel: sideways) |
| Ctrl (Cmd) and the wheel, a pinch | Zooms around the pointer |
| Drag with the middle button, or with Space held | Pans |
| Drag on the canvas | A rubber band: selects the nodes it touches (Shift or Ctrl adds) |
| Click on the canvas | Clears the selection, drops a wire begun with a click |
| Double click on the canvas | Fits the graph (`gesture.graph.fit`) |
| Click on a node | Selects it (Shift or Ctrl adds or removes it) |
| Drag a node | Moves the selection |
| Double click on a title, or its chevron | Collapses the node to its title |
| Drag from a port | Draws a wire, checked as it goes: ports that accept it are ringed, others dimmed, the wire turns to the refusal colour over a port that refuses it |
| Drag from a connected input | Takes its wire along: dropped nowhere, it disconnects |
| Click a port, then another | Connects them |
| A group's title | Folds the group into a stack; a click on the stack unfolds it; dragging either moves the group |
| Secondary click | The canvas's menu (add a node), a node's (duplicate, delete), a port's (disconnect) |

### Wires

`wires(style)` picks how the graph's wires are drawn (`WireStyle`):

| Style | What it draws |
|---|---|
| `Curves` (the default) | A curve out of each port; it may cross nodes |
| `Orthogonal` | Horizontal and vertical segments around the nodes (and stacks), keeping a margin from them, with few bends; wires through one channel are spread apart |
| `Straight` | A straight segment from port to port |

Orthogonal routes come from [ortho-router](../crates/ortho-router/README.md),
a crate of this workspace without egui, published on its own for any
project that draws connectors: A* with a bend cost over the grid of
the nodes' edges grown by the margin. The routes are kept in the
`GraphState` and computed again only when the layout changes (a node
moved, collapsed or folded, a wire added or removed, another level), so
a frame that changes nothing routes nothing; while nodes are dragged
they follow at each frame. When a port has no way out (boxed in by
other nodes), its wire falls back to one elbow halfway. The wire being
drawn is that elbow too, routed once it is connected.
`GraphState::wire_style` says the style drawn last, and the graph's
snapshot (`describe`, `graph.read`) carries `wires` (its name) and,
when orthogonal, `routes`: each wire's `edge` and `points`.

### Overview and detail

The zoom picks the level (`Level`), with `levels(overview_below,
detail_from)` (0.55 and 0.9 by default):

- **Overview**: compact nodes with their titles at a size readable at
  that zoom, groups folded into stacks (`+12`), wires between boxes;
  nodes still select and move.
- **Ports**: every node with its ports and their labels, the wires
  between ports.
- **Detail**: also each node's parameters, egui-pop sliders edited
  inline (the arrows, a typed value, a double click to the default).

A group folded at the ports or detail level stays folded
(`GraphState::folded`); in the overview, groups are folded unless
unfolded there (`GraphState::unfolded`).

### Keyboard

| Key | Where | What it does |
|---|---|---|
| Tab, Shift+Tab | anywhere | Through the canvas, each node, its chevron, its ports and its sliders |
| Arrows | the canvas | To the node nearest the centre of the view |
| Arrows | a node | To the nearest node that way |
| Shift + arrows | a node | Moves the selection by a grid step (`Move`) |
| Enter, Space | a node | Selects it |
| Arrows | a port | To the node's other ports |
| Enter, Space | a port | Begins a wire there; on a second port, connects (or says why not) |
| Delete, Backspace | a port | Disconnects it |
| Escape | a port | Drops the wire begun, else back to the node |
| Delete, Backspace | a node, the canvas | Deletes the selection (the focused node when it is not selected) |
| Ctrl+D | a node, the canvas | Duplicates the selection |
| Ctrl+A | a node, the canvas | Selects every node |
| Home | a node, the canvas | Fits the graph |

The keys are data (`graph::GraphKeys`, `graph::set_keys`). The view
follows the focus: a node or port brought into focus out of view is
brought into it.

## Steps

`egui_pop::imaging::show_graph` shows an editor as part of the
application (ADR 0011): each edit runs its step, the events go on the
journal, agents' requests reach it.

| Step | Arguments | Default |
|---|---|---|
| `graph.connect` | `graph`, `from`, `to` (`{node, port}`), `replaces`, `cause` | hands the edit to the application |
| `graph.disconnect` | `graph`, `edges` (`[{from, to}]`), `cause` | hands the edit to the application |
| `graph.move` | `graph`, `moves` (`[{node, from, to}]`), `cause` | hands the edit to the application |
| `graph.delete` | `graph`, `nodes`, `edges`, `cause` | hands the edit to the application |
| `graph.add` | `graph`, `nodes` (`[{kind, at, copy_of}]`), `cause` | hands the edit to the application |

A before-hook vetoes the edit (it never reaches the application) or
redirects it (another place, another kind); a run-hook's result is the
edit handed back; an after-hook follows it. A parameter's slider is not
a step. `cause` is `drag`, `click`, `key`, `command`...

Events on the journal: `graph.selection_changed` (`graph`, `nodes`),
`graph.level_changed` (`graph`, `level`), `graph.refused` (`graph`,
`from`, `to`, `reason`).

```rust
use egui_pop::egui;
use egui_pop::api::steps::Before;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::imaging::{self, graph::STEP_GRAPH_DELETE};
use egui_pop::model::ids::PluginId;
use egui_pop::viewport::graph::{GraphEditor, GraphModel, GraphState};

/// The output node stays.
struct KeepOutput;

impl Plugin for KeepOutput {
    fn id(&self) -> PluginId {
        PluginId::from_static("keep_output")
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![imaging::IMAGING_PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.before(STEP_GRAPH_DELETE, |_, args| {
            if args["nodes"].as_array().is_some_and(|n| n.iter().any(|n| n == "output")) {
                Before::Veto("the output stays".into())
            } else {
                Before::Go
            }
        });
        Ok(())
    }
}

fn panel(ui: &mut egui::Ui, app: &AppHandle, graph: &dyn GraphModel, state: &mut GraphState) {
    let out = imaging::show_graph(app, "main", GraphEditor::new(graph, state), ui);
    for _edit in out.edits {
        // Apply it to the graph.
    }
}

let _ = (KeepOutput, panel);
```

### Commands for agents

`ImagingPlugin` declares them (hidden from the palette); `graph` names
an editor shown with `show_graph` (without it, the only one). Edits
asked by a command are done at the editor's next frame, as the user's:
a connection is type-checked, then the edit runs its step.

| Command | Arguments | Result |
|---|---|---|
| `graph.read` | `graph` | the graph as drawn: `nodes` (place, ports, group, collapsed, selected, parameters), `edges`, `groups`, `kinds`, `selection`, `level`, `zoom`, `visible` |
| `graph.edit` | `graph`, `edit` (`connect`, `disconnect`, `move`, `delete`, `add`) and the edit's fields | `queued` |
| `graph.select` | `graph`, `nodes` | `queued` |
| `graph.view` | `graph`, and `fit: true`, `level` or `focus` (a node) | `queued` |
| `graph.export` | `graph`, `selection`, `level`, `wires`, `background`, `title` | `svg`: the graph as an image (see [Export](#export)) |

## Export

The graph exports as an SVG image, drawn from the model and its layout
rather than from the screen: it is crisp at any size, and the same
graph with the same options gives the same text, so an export can be
compared in a test or reviewed in a diff.

```rust
use egui_pop::egui;
use egui_pop::viewport::graph::{ExportOptions, GraphEditor, GraphExport, GraphModel, GraphState, Level, WireStyle};

fn save(ctx: &egui::Context, graph: &dyn GraphModel, state: &mut GraphState) -> (String, String) {
    // What this editor would draw: its wire style and node width.
    let whole = GraphEditor::new(graph, state)
        .wires(WireStyle::Orthogonal)
        .export_svg(ctx, &ExportOptions::default());
    // From a model and the state alone (the style drawn last), the
    // selection with each node's parameters.
    let selection = GraphExport::capture(ctx, graph, state)
        .svg(&ExportOptions::default().selection(true).level(Level::Detail));
    (whole, selection)
}

let _ = save;
```

| Option | Default | What |
|---|---|---|
| `selection` | `false` | Only the selected nodes, the wires between them and the frames of their groups (an empty selection, an empty image) |
| `level` | `Level::Ports` | The overview (compact nodes, groups folded into stacks unless unfolded there), the ports, or the detail (each parameter's label, value and unit, and how far along its range it is) |
| `background` | `true` | The graph's background behind the drawing; off, transparent |
| `margin` | 24 | Around the drawing, in graph units |
| `title` | "Node graph" | The image's `<title>`, for screen readers |

What is drawn: group frames with their labels, the wires in the graph's
`WireStyle` (curves, straight segments, or orthogonal routes computed
for the exported layout) in their port type's colour, each node with
its title bar, its type band, its ports and their labels, and stacks
with their count. The state's collapsed nodes and folded groups are
kept. What only an interactive canvas shows is left out: the grid, the
selection's outline, focus rings, chevrons, the application's slot in
title bars. Coordinates are graph units (a point at 100 %); the
`viewBox` frames what is drawn and the margin.

The colours and sizes come from the theme (`ExportTheme::of`: the
`graph.*` tokens, each port type's colour token, the `font.family.ui`
family then `sans-serif`, the body and small sizes); an application
builds its own `ExportTheme` for a light export of a dark interface,
and passes it to `GraphExport::with_theme`. Elements carry the graph's
ids for tools and tests: `<g data-node>`, `<path data-edge="a.out->b.in">`,
`<g data-group>`, `<g data-stack>`, `<circle data-port data-side>`,
`<g data-param>`.

PNG and PDF are not built in: resvg, already a dependency of the
`svg` feature, renders SVG to pixels, but only with its text support,
which brings its own font loading and shaping; an application that
wants a raster renders the SVG with the tool it already has.

`show_graph` keeps a copy of the graph at each frame
(`GraphEditor::keep_export`, `GraphOutput::export`), which the
`graph.export` command exports: `level` by name, `wires` (`curves`,
`orthogonal`, `straight`) to draw another style than the editor's.

## A graph file

An application without a graph model of its own can keep its graph in
a YAML file: `egui_pop::viewport::graph::yaml::YamlGraph` (the facade's
`graph-yaml` feature) reads one, is the editor's `GraphModel`, and
writes the edits back into the file's text. The file is the source of
truth: it can be written by hand or by an agent, reviewed in a diff and
versioned.

```rust
use egui_pop::egui;
use egui_pop::api::AppHandle;
use egui_pop::imaging;
use egui_pop::viewport::graph::yaml::YamlGraph;
use egui_pop::viewport::graph::{GraphEditor, GraphModel, GraphState};

fn panel(ui: &mut egui::Ui, app: &AppHandle, graph: &mut YamlGraph, state: &mut GraphState) {
    let out = imaging::show_graph(app, "main", GraphEditor::new(&*graph, state), ui);
    for edit in &out.edits {
        // The one mutation point: the edit lands in the text.
        graph.apply(edit);
    }
    // `graph.text()` is the file to save.
}

let graph = YamlGraph::parse("nodes:\n  - id: a # the first\n    pos: [0, 0]\n").expect("a graph file");
assert_eq!(graph.nodes().len(), 1);
let _ = panel;
```

### The format

```yaml
# The format's version, as token files carry theirs.
$extensions:
  org.egui-pop.graph: {version: 1}

groups:
  detail: Detail            # an id and its label, or {label: Detail}

kinds:
  load:
    title: Load
    outputs:
      image: image          # a port's id and its type
  blur:
    title: Blur
    inputs: {image: image}
    outputs: {image: image}
    params:
      radius: {label: Radius, value: 2, min: 0, max: 20, step: 0.1, unit: px}
  mix:
    title: Blend
    inputs:
      base: image
      mask: {type: mask, label: Mask, multiple: true, color: graph.type-3}
    outputs: {image: image}

nodes:
  # The source.
  - id: load
    kind: load
    pos: [0, 0]
  - id: blur
    kind: blur
    title: Soft blur        # instead of the kind's
    pos: [240, 0]
    group: detail
    params:
      radius: 3.5           # the value; the rest is the kind's

edges:
  - load.image -> blur.image
  - {from: blur.image, to: mix.base, note: kept}
```

| Key | What |
|---|---|
| `$extensions` | `org.egui-pop.graph: {version: N}` (`FORMAT_EXTENSION`, `FORMAT_VERSION`); a file without it is read as the current version, a newer one is read with a warning (`YamlGraph::warnings`). Other extensions are the application's. |
| `groups` | Each group's id and label (`detail: Detail` or `detail: {label: Detail}`). |
| `kinds` | What the canvas's menu adds, and what a node of the kind has: `title`, `inputs` and `outputs` (each port `id: type`, or `{type, label, multiple, color}`), `params` (each `{label, value, min, max, step, unit, default}`; `min` and `max` are required). |
| `nodes` | A list of nodes: `id` (required, unique), `kind`, `title` (else the kind's, else the id), `pos` (`[x, y]`, the top-left corner, else `[0, 0]`), `group`, and `inputs`, `outputs`, `params` of their own: a port of the same id replaces the kind's, a parameter's fields go over the kind's (a number alone sets the value). |
| `edges` | A list of wires: `node.port -> node.port` (an output, then an input; the port's id is after the last dot), or `{from: node.port, to: node.port}`. |

Labels left out are the id, capitalised. A parameter's value is its
`value`, else its `default`, else `min`; a double click goes back to
its `default`, else to the kind's `value`. A kind or a group a node
names must be declared. A file the format does not allow is refused
with the line and column of what is wrong (`YamlGraphError`): YAML
that does not parse, a top level that is not a block mapping, two
nodes with one id, a kind, group, node or port that is not there, a
parameter without its range.

### What round-trips

`YamlGraph::text` is the file as read until an edit, byte for byte. An
edit changes the lines it lands on and nothing else:

| Edit | In the text |
|---|---|
| `Move` | the node's `pos` value (the comment after it stays), or a `pos` line added |
| `Param` | the parameter's value under the node's `params`, or a line added there |
| `Delete` | the node's item and the comment right above it, the items of its wires |
| `Connect`, `Disconnect` | the wires' items removed, a new item after the last wire |
| `Add` | a new item after the last node (`kind-1`, `kind-2`...; a duplicate takes the group and parameters of the node it copies) |

Comments, blank lines, key order, quoting, line endings and the keys
egui-pop does not know (in a node, a wire, anywhere else) are kept.
Two cases are written again: a node written in flow style
(`- {id: a, pos: [0, 0]}`) that an edit reaches is written again in
block style, its unknown keys as flow values; a `nodes` or `edges` list
in flow style (`nodes: [...]`) is written again whole in block style
at its first edit, comments inside it lost. Each splice is read back
and compared with the graph the edit makes; a splice that would not
read back falls back to writing the two lists again.

An edit is one gesture (a drag's `Move` comes at its end, a slider's
frames with `done: false`): an application that keeps an undo history
records `text()` before an edit and reads it back to undo.

## Accessibility

| Part | Role | Name | Value |
|---|---|---|---|
| The canvas | group | its label ("Node graph") | nodes, wires and the level |
| A node | group, described as "node", selected or not | its title | its inputs, outputs and wires |
| A port | button, described as "input port" or "output port" | "Blur, input Image" | its type and wires |
| The chevron | button | "Collapse Blur", "Expand Blur" | |
| A group's title | button | "Fold Detail" | |
| A stack | button, described as "stack" | "Unfold Detail (12 nodes)" | |
| A parameter | slider | its label | its value |
| The minimap | scroll view | "Graph overview" | the zoom |

A wire begun, made or refused is announced (a live region). The
editor's widgets sit in the `Scene`'s transformed layer, whose
AccessKit bounds egui reports untransformed: the editor sets them
itself (see [upstream](upstream.md)).

## Tokens

`graph.*`: `background`, `grid`, `node`, `node-header`, `node-border`,
`node-selected`, `wire` (a type with no colour), `wire-preview`,
`wire-refused`, `group`, `group-border`, `band`, and `type-1` to
`type-6` (the port types' colours, aliases of the accent and the
status colours). The minimap draws with `navigator.*`. The selected
outline is contrast-checked against the node.

## Why its own (2026-10-13)

| Crate | Licence | egui | What it does | Why not |
|---|---|---|---|---|
| egui's `Scene` | MIT OR Apache-2.0 | 0.36 | A pan and zoom container (a transformed layer) | **Adopted**: the editor is drawn in it |
| egui-snarl 0.12 | MIT OR Apache-2.0 | 0.36 | A node graph widget with a viewer trait | It owns the graph (`Snarl<T>` keeps nodes, positions and wires; the viewer's callbacks change it in place), so edits cannot come back as requests to veto; no AccessKit nodes for nodes and ports, no keyboard path, no levels of detail, styled outside the tokens |
| egui_node_graph 0.4 | MIT | 0.2x | The first node graph editor for egui | Unmaintained since 2022 |
| egui_node_graph2 0.7, egui-graph-edit 0.9 | MIT | 0.29, 0.35 | Its forks | Own the editor's graph state (a slotmap); not on egui 0.36 |
| egui_graphs 0.32 | MIT | 0.36 | Shows a petgraph graph, with force layouts | A viewer of the application's petgraph, not an editor of typed ports |

Decision: **own**, built on egui's `Scene`. The application's graph,
requests run as steps, levels of detail and the keyboard and screen
reader paths are what egui-pop needs and none of the editors offers;
no code is taken from them.
