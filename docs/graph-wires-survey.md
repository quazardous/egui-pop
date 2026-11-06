# Graph wires survey: editing connections in node editors

How do node editors let people edit the wires themselves: hover, select,
delete, cut, detach and reconnect from either end, move all of a port's
wires, reroute, insert a node on a wire, mute, and reach all of it from
the keyboard and a screen reader? This study surveys Blender, Houdini,
Nuke, DaVinci Fusion, TouchDesigner, Unreal Blueprints, Unity Shader
Graph, ComfyUI (LiteGraph), Max, Pure Data, React Flow, Rete.js and vkdt,
compares them with egui-pop's node graph editor, and ends with a ranked
plan. It answers issue
[#227](https://github.com/quazardous/egui-pop/issues/227), "diagram
connectors cannot be edited" (select, delete, disconnect, reconnect).

Status: items 1 and 2 of the plan below are built (2026-11-06; see
[node-graph.md](node-graph.md)); items 3 to 7 are not started.

Read on 2026-11-06, against egui-pop's `main`. Nothing was built or run:
every point about another editor comes from its documentation (linked),
its source, or, where marked *(community)*, a forum or third-party page.
"Not documented" means the sources read do not say, not that the feature
is missing.

## Summary

- **egui-pop has no wire as an object.** Wires are edited only through
  their ports: drag off a connected input, Delete on a focused port, a
  port's menu. A wire cannot be hovered, clicked, selected, deleted on
  its own, or picked up from its output end when the output has several.
- **Almost every editor surveyed makes the wire a first-class target**:
  it highlights under the pointer (Max, TouchDesigner, LiteGraph, React
  Flow), a click selects it and Delete removes it (Houdini, Max, Pure
  Data, Shader Graph, React Flow), a right click opens a wire menu
  (Houdini, TouchDesigner, Max, Shader Graph, LiteGraph). Blender is the
  exception: links are never selected, only cut with a stroke.
- **Reconnecting from either end** is common: Fusion splits a wire into an
  output half and an input half, Max shows two handles on a selected
  cord, Nuke drags head or tail, React Flow lets each end be reconnectable.
  Dropped nowhere, the wire is removed (Fusion, Nuke, React Flow's
  example, egui-pop today).
- **Moving all of a port's wires** is a modifier on the port drag: Ctrl in
  Unreal and Blender (outputs), Shift in ComfyUI.
- **Inserting a node by dropping it on a wire** is near universal in
  compositing tools (Blender, Nuke, Houdini, vkdt) and matters most for an
  image pipeline.
- **Cut strokes** (Blender Ctrl+right drag, Houdini Y+drag) are fast but
  invisible; they belong in a profile, not the default.
- **Keyboard and screen reader access to wires** is rare. React Flow makes
  edges focusable with a spoken hint; Pure Data cycles through selected
  cords with Tab. egui-pop already announces wires on ports and can do
  better than most.

Recommended first pull request (M): wire hit-testing, hover highlight
and tooltip, click and Ctrl/Shift selection, Delete as one `Disconnect`,
a wire menu, end handles on a selected wire to reconnect either end, and
the wire in the keyboard path, the accessibility tree and agent
commands. It closes #227. Insert-on-wire and moving all wires follow.

## egui-pop today

From `crates/egui-pop-viewport/src/graph/mod.rs` and
[node-graph.md](node-graph.md):

| Action | Today |
|---|---|
| Draw a wire | Drag from a port (type-checked while drawn); or click a port then another; or Enter on two focused ports |
| Replace an input's wire | Drop a new wire on it: `Connect { replaces }` |
| Detach from the input end | Drag off a connected input: the wire follows; dropped on a port it reconnects, nowhere it disconnects |
| Detach from the output end | Not possible: a drag from an output always starts a new wire (fan-out) |
| Hover a wire | No feedback |
| Select a wire | Not possible (`GraphState::selection` holds nodes only) |
| Delete a wire | Delete on a focused port (all its wires), a port's context menu, or deleting a node |
| Cut, reroute, insert on wire, mute | None |
| Keyboard | Tab to a port, Delete disconnects it; Enter/Space picks it as a wire's end |
| Accessibility | A port is a button whose value lists its type and wires; wires begun, made or refused are announced |
| Agents | `graph.edit` with a `disconnect` or `connect` edit; `graph.read` lists edges |
| Undo | Each `Edit` is one finished gesture; the application makes it one step (Helios: one `network.*` command, one undo step) |

The model already fits wire editing: `Edit::Disconnect { edges }` takes
several wires, `Edit::Connect { replaces }` expresses a reconnect as one
edit, and every edit passes the `on_edit` hook and the `graph.*` steps.
What is missing is on the drawing side: hit-testing a wire, a wire
selection in `GraphState`, and the gestures on it. Wires are drawn as
curves, straight segments or orthogonal routes (`wires.rs`), the routes
kept in `Drawn::routes`, so a hit test has the polyline at hand for
every style.

## Per-system notes

### Blender (node editors)

- Links are never selected. Cut: Ctrl+right drag a stroke across links
  deletes them; Ctrl+Alt+right drag mutes or unmutes them (a muted link
  is drawn red and passes nothing). Detach: Alt+left drag on nodes cuts
  all their links. Drag a link off an input and release: disconnected.
- Ctrl while dragging from an output moves its outgoing links; Alt while
  moving a link swaps links of a similar type.
- Drop a node on a link: the link highlights, the node is inserted, and
  auto-offset pushes the downstream nodes aside (T flips the side, Alt
  disables the attachment).
- Ctrl+Delete deletes a node and reconnects its inputs to its outputs.
- Shift+right drag across a link inserts a reroute node (one input, many
  outputs).
- Releasing a link over empty space opens a search of compatible nodes
  (link drag search).
- Keyboard: J / Shift+J make links between selected nodes. No screen
  reader support.
- Sources: [Editing nodes](https://docs.blender.org/manual/en/latest/interface/controls/nodes/editing.html),
  [Arranging nodes](https://docs.blender.org/manual/en/latest/interface/controls/nodes/arranging.html),
  [Reroute node](https://docs.blender.org/manual/en/latest/interface/controls/nodes/reroute.html),
  [Link drag search commit](https://projects.blender.org/archive/blender-archive/commit/11be151d58ec0ca9).

### Houdini (network editor)

- A wire can be selected and Del deletes it; right click opens a wire
  menu (Disconnect among others).
- Y+drag across wires cuts them; shaking a node disconnects it.
- Alt+click a wire adds a dot (a reroute point), Alt+click a dot pins it;
  Alt+drag a dot gathers the wires it crosses.
- Drop a node on a wire to insert it (hold 8 to prevent); select a wire
  and press Tab to insert a node there; right click a connector to insert
  before, middle click to branch.
- Sources: [Wires](https://www.sidefx.com/docs/houdini/network/wire.html),
  [Network editor shortcuts](https://www.sidefx.com/docs/houdini/network/shortcuts.html).

### Nuke (node graph)

- Drag the head or the tail of a pipe: dropped on a node it reconnects,
  in empty space it disconnects. Ctrl/Cmd+D disconnects the selected
  node's inputs.
- Shift+drag a pipe end duplicates the connection (the original stays).
- Ctrl/Cmd+click the yellow dot at a pipe's middle inserts a Dot node.
- Drag a node between two connected nodes: the pipe lights up, release
  inserts it.
- Source: [Connecting nodes](https://learn.foundry.com/nuke/content/getting_started/using_interface/connecting_nodes.html);
  shake to disconnect: [Foundry](https://www.foundry.com/insights/nuke-progress-shake-to-disconnect).

### DaVinci Resolve Fusion (node editor)

- A connection has two halves: click the input half once to break it;
  drag either half onto another node's socket to move that end (the old
  link goes, one gesture); drag it to empty space to disconnect.
- Routers (reroute points) reshape and branch connections; pipes can be
  direct or orthogonal.
- Source: Resolve 18.6 manual, Fusion "Connecting and disconnecting
  nodes" (Blackmagic Design); summary in
  [Mixing Light](https://mixinglight.com/color-grading-tutorials/resolve-fusion-back-to-basics-part-2/) *(community)*.

### TouchDesigner (network editor)

- A wire turns yellow under the pointer. Right click a highlighted wire:
  insert an operator, select the source node, select the destination
  node, disconnect. Middle click shows the source's info.
- Wires are drawn by drag or by two clicks (as egui-pop does).
- Source: [Wire](https://docs.derivative.ca/Wire).

### Unreal Engine Blueprints

- Hovering a pin emphasises its wires. Alt+click a pin breaks all its
  connections; Ctrl+drag from a pin moves all its connections to another
  pin; right click a pin for a menu (break a single link by name).
- Double click a wire adds a reroute node *(community)*; dropping a wire
  in empty space opens the action menu filtered by the pin's type.
- Sources: [Blueprint editor cheat sheet](https://dev.epicgames.com/documentation/en-us/unreal-engine/blueprint-editor-cheat-sheet-in-unreal-engine);
  double click and Alt+click on the wire: [Epic forum](https://forums.unrealengine.com/t/how-do-i-disconnect-cut-remove-blueprint-wires-lines/370820) *(community)*.

### Unity Shader Graph

- Edges are selected and deleted with Delete (Cmd+Backspace on macOS) or
  the context menu; dropping a dragged edge in empty space opens the
  Create Node menu.
- A redirect (elbow) node: double click a wire, right click it and "Add
  Redirect Node", or select it and Ctrl/Cmd+R.
- Sources: [Edge](https://docs.unity3d.com/Packages/com.unity.shadergraph@14.0/manual/Edge.html),
  [Redirect node](https://docs.unity3d.com/Packages/com.unity.shadergraph@17.0/manual/Redirect-Node.html).

### ComfyUI and LiteGraph

- LiteGraph highlights the links of the selected nodes and draws a marker
  at each link's middle; a click on it opens a menu (Add Node, which
  inserts the node on the link, and Delete). Dragging a connected input
  takes its link along (`allow_reconnect_links`).
- ComfyUI: a setting picks what a link released on empty space does
  (context menu, search box, nothing; Shift gives the other); Shift
  drags several links at once (frontend 1.2.7).
- Sources: [LiteGraph source](https://github.com/jagenjo/litegraph.js/blob/master/src/litegraph.js)
  (`showLinkMenu`, `allow_reconnect_links`),
  [ComfyUI LiteGraph settings](https://docs.comfy.org/interface/settings/lite-graph),
  [ComfyUI shortcuts](https://docs.comfy.org/interface/shortcuts).

### Max (Cycling '74)

- Cords highlight under the pointer when the patch is unlocked. A click
  selects a cord; Alt-drag selects cords with a rectangle. A selected
  cord shows a handle at each end: drag one to change the source or the
  destination. Right click: colour, Align (segments).
- Source: [Patch cords](https://docs.cycling74.com/max8/vignettes/patch_cords).

### Pure Data (vanilla)

- Click a connection to select it, Backspace or Delete removes it; cords
  cannot be rubber-banded. Tab and Shift+Tab cycle through the selected
  cords.
- Shift+click another cord swaps the two; Ctrl+K inserts the selected box
  into the selected connection (and bypasses a middle box); Ctrl+T on a
  selected connection inserts a pass-through object; Ctrl+D duplicates a
  connection.
- Source: [Pd manual, chapter 2](https://msp.ucsd.edu/Pd_documentation/2.theory.of.operation.htm)
  (2.2.6, 2.3).

### React Flow (web library)

- Edges are selectable by click (`elementsSelectable`), with Ctrl/Meta
  for several (`multiSelectionKeyCode`) and by the selection box
  (`selectionKeyCode`); `deleteKeyCode` (Backspace) deletes selected nodes
  and edges; per edge `selectable`, `deletable`, `focusable`.
- An invisible wider path makes thin edges easy to hit
  (`interactionWidth`), which also serves touch.
- Reconnect: `edgesReconnectable` and `onReconnect`, per edge
  `reconnectable: true | "source" | "target"`; an example deletes an edge
  dropped on the pane (`onReconnectStart` / `onReconnectEnd`).
- Keyboard: edges are Tab stops (`edgesFocusable`), Enter or Space selects,
  Delete removes, Escape clears; a spoken description says so
  (`ariaLabelConfig`, `edge.a11yDescription.default`), and each edge has
  an `ariaLabel`.
- Sources: [ReactFlow props](https://reactflow.dev/api-reference/react-flow),
  [Edge type](https://reactflow.dev/api-reference/types/edge),
  [Reconnect edge](https://reactflow.dev/examples/edges/reconnect-edge),
  [Delete edge on drop](https://reactflow.dev/examples/edges/delete-edge-on-drop),
  [Accessibility](https://reactflow.dev/learn/advanced-use/accessibility).

### Rete.js

- The connection plugin's presets pick up an input's connection on press
  (a pseudo-connection follows the pointer, as egui-pop does); the
  `connectionpick` and `connectiondrop` events can be vetoed. Selectable
  connections and reroute pins are separate plugins
  (`rete-connection-reroute-plugin`).
- Source: [Connections guide](https://retejs.org/docs/guides/connections).

### vkdt and darktable

- vkdt's graph editor: drag an output pin to an input to connect; right
  click an input's ball to remove its connection; drag a disconnected
  node onto a link to insert it, when the node has a plain `input` to
  `output` chain. No wire selection.
- darktable has no graph: its pipeline is an ordered stack; modules are
  moved in the stack, not rewired.
- Source: [vkdt GUI basics](https://github.com/hanatos/vkdt/blob/master/doc/howto/gui-basics/readme.md).

## Comparison

Y: documented; –: not offered; ?: not documented in the sources read.

| | Hover | Select wire | Delete wire | Cut stroke | Move either end | All of a port | Reroute | Insert on wire | Mute | Wire menu | Keyboard on wires |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Blender | ? | – | cut / drag off | Ctrl+RMB | input end | Ctrl (output) | Shift+RMB | drop node | Ctrl+Alt+RMB | – | – |
| Houdini | ? | Y | Del, menu | Y+drag | via dots | ? | Alt+click dot | drop, Tab | – | Y | Tab inserts |
| Nuke | ? | ? | drag off | – | head and tail | ? | Ctrl+click dot | drop node | – | ? | – |
| Fusion | ? | ? | click input half | – | both halves | ? | routers | ? | – | ? | – |
| TouchDesigner | Y | ? | menu | – | ? | ? | ? | menu | – | Y | – |
| Unreal | pin hover | ? | Alt+click | – | Ctrl+drag | Ctrl+drag | double click | ? | – | pin menu | – |
| Shader Graph | ? | Y | Delete | – | ? | ? | double click, Ctrl+R | ? | – | Y | – |
| ComfyUI | selected nodes' | – | midpoint menu | – | input end | Shift | Y | midpoint menu | – | Y | – |
| Max | Y | click, Alt+box | Y | – | handles | ? | Align | ? | – | Y | – |
| Pure Data | ? | click | Delete | – | ? | Shift (fan) | – | Ctrl+K, Ctrl+T | – | – | Tab cycles |
| React Flow | style | click, box | Backspace | – | each end | – | (custom) | (custom) | – | (custom) | Tab, Enter, Delete |
| vkdt | ? | – | right click input | – | – | – | – | drop node | – | – | – |
| **egui-pop** | – | – | port only | – | input end | – | – | – | – | port menu | port only |

Undo granularity: where documented, a gesture is one undo step (Blender's
operators, Pd's edits); in React Flow and Rete the application decides.
Touch: only React Flow addresses it (the wide hit path); the desktop tools
assume a mouse.

## Recommended for egui-pop

Ranked by value for size. S: a day or less; M: a few days; L: more.

1. **The wire as a target (M, closes #227).**
   - *Hit test*: distance from the pointer to the drawn polyline (curves
     flattened, orthogonal routes from `Drawn::routes`), within 6 screen
     points, ports and nodes winning over wires. On touch, 12.
   - *Hover*: the wire drawn thicker in `graph.wire-hover` (a new token),
     its two ports ringed, a tooltip "Load · Image → Blur · Image", and the
     hint line says what a click and a drag do.
   - *Select*: a click selects the wire (clearing nodes), Ctrl/Shift
     toggles it into the selection; selected wires drawn in
     `graph.node-selected`. `GraphState` gains `selected_edges:
     BTreeSet<Edge>`; `GraphEvent::Selection` reports nodes and edges.
     The rubber band keeps selecting nodes only (Pd and Max make box
     selection of wires a separate gesture; it adds little).
   - *Delete*: Delete/Backspace with wires selected emits one
     `Edit::Disconnect` with all of them (one undo step); with nodes and
     wires selected, one `Delete` whose `edges` carry both.
   - *Wire menu* (secondary click): Disconnect, Select source node,
     Select destination node, Insert node… (once item 3 lands).
   - *Reconnect either end*: a selected or hovered wire shows a handle near
     each end (Max, Fusion's halves, React Flow); dragging a handle
     detaches that end, dropped on a port it emits `Connect { replaces:
     [old] }` (one edit), dropped nowhere it emits `Disconnect`, as a drag
     off an input does today. This is also how one wire of a fanned-out
     output is picked up. Escape puts it back.
2. **Keyboard, screen reader and agents for wires (S, in the same PR if it
   fits).** See the next section.
3. **Insert a node on a wire (M).** Dragging a node with a free input and a
   free output over a wire highlights it; release inserts (Blender, Nuke,
   Houdini, vkdt). Alt held suspends it (Blender). Needs one compound edit
   for one undo step: `Edit::Insert { node, edge, input, output }`,
   type-checked like a connect, which the application may veto or turn
   into its own command. The wire menu's "Insert node…" opens the add menu
   and inserts the new node the same way (LiteGraph, TouchDesigner).
4. **Move all of a port's wires (S–M).** Ctrl (Cmd) + drag from a connected
   port takes all its wires to another port of the same side (Unreal; Blender
   for outputs). One edit: `Edit::Reconnect { moves: Vec<(Edge, Edge)> }`,
   or `Connect` generalised to several pairs. Dropped nowhere: all
   disconnected.
5. **Delete and heal (S).** A node deleted with Ctrl+Delete reconnects its
   main input's source to its main output's consumers (Blender
   Ctrl+Delete, Houdini "reconnect around", Pd Ctrl+K). An option on
   `Edit::Delete` (`heal: true`) so the application applies it in the same
   step.
6. **Release on empty canvas opens the add menu (S).** A wire dropped
   nowhere from an output opens the add menu filtered by type, and the new
   node is connected (Blender link drag search, Unreal, Shader Graph,
   ComfyUI). Today it is a silent cancel for a new wire, so nothing is lost.
7. **Cut stroke (S), profile-bound.** Ctrl + secondary drag draws a line;
   wires crossed are cut as one `Disconnect`. Bound in the "Blender / node
   editors" pointer profile only (its secondary drag already pans the graph;
   Ctrl keeps them apart), unbound in the others.

**Skipped, and why:**

- *Reroute points / dots* (L): the model has no point on a wire, and the
  orthogonal router already keeps wires off nodes. An application that wants
  them can offer a "dot" node kind. Revisit if users ask.
- *Muting a wire*: Helios bypasses nodes, not wires, and a muted wire is an
  engine concept the editor cannot own. An application could draw a muted
  wire through a style on `Edge` later.
- *Shake to disconnect*: undiscoverable and easy to trigger by accident;
  Ctrl+Delete heal covers the same need.
- *Swapping wires* (Pd, Blender Alt): rare; two reconnects do it.
- *Duplicating a wire* (Nuke Shift+drag): a new drag from the output already
  fans out.
- *Box selection of wires*: deleting nodes already takes their wires; a
  wire is selected where it is seen.

## Accessibility and agents

- **Keyboard.** A wire belongs to its input port (an input has at most one
  wire unless `multiple`), so no new chord is needed: inside a node's
  parts (Enter on the node with `TabStops::One`), Tab goes through the
  ports and, after each connected input, its wires. A focused wire: Delete disconnects it, Enter picks its input end for reconnecting
  (then a port is chosen as today, Enter on it reconnects, Escape cancels),
  Shift+Enter picks its output end, the menu key opens the wire menu,
  Escape goes back to the port. Pure Data's Tab through selected cords and
  React Flow's focusable edges are the precedents.
- **Screen reader.** Each wire is an AccessKit node under its input port:
  role button, role description "wire", name "Load, output Image to Blur,
  input Image", selected state. The spoken hint, as React Flow's: "Delete to
  disconnect, Enter to move its input end". Selecting, disconnecting and
  reconnecting a wire are announced in the live region, as connecting is
  today.
- **Focus visible.** The focused wire is drawn with the focus ring's colour
  along its path, not only the selection's.
- **Agents.** Nothing new is needed to edit: `graph.edit` already takes
  `disconnect` and `connect` with `replaces`. To add: `Request::Select`
  extended with edges (`graph.select` with `edges: [{from, to}]`),
  `graph.read` reporting the selected wires and, for each wire, its route
  points (already there for orthogonal routes), and the new edits of items 3
  to 5 as `graph.insert`, `graph.reconnect` steps with the same veto and
  redirect as the others.
- **Undo.** One gesture, one edit, one step: a multi-wire delete, a
  reconnect, an insert and a heal are each one edit, never a disconnect
  followed by a connect.

## What Helios needs

Helios shows the workspace's image-processing network (sources, darktable
and vkdt modules, composites, mask networks) through egui-pop's editor, and
turns each edit into a `network.*` command, one undo step that agents also
see (`helios-gui/src/graph.rs`).

- **Disconnect and reconnect (item 1)** map straight onto `network.disconnect`
  and `network.connect` with `replaces`: no Helios change beyond what it
  does today.
- **Insert on a wire (item 3)** is the gesture image pipelines lean on most
  (a module added between two others). Helios's design asks for explicit
  actions ("insert before", "insert after") rather than precise gestures; the
  wire menu's "Insert node…" and the drop-on-wire gesture both serve it, and
  `Edit::Insert` would become one Helios command (a new `network.insert`, or
  `network.add` followed by connects in one step).
- **Delete and heal (item 5)** matches Helios's rule that taking a member out
  rewires the network; `network.remove` today drops the wires, so heal would
  be a Helios change too.
- **Fan-out**: a module output feeds several consumers (masks, blends);
  picking up one of them by its handle (item 1) is the missing piece.
- **Mute**: Helios bypasses nodes (`node.bypass`, one undo step), so wire
  muting is not needed.
- **Reroutes**: not needed; Helios's layout is layered and the orthogonal
  wires avoid nodes.
