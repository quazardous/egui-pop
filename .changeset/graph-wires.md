---
egui-pop-viewport: major
egui-pop-imaging: major
egui-pop-test: major
egui-pop-i18n: minor
egui-pop: major
---

The node graph editor's wires are targets of their own. Hovered (within
6 points, 12 on a touch screen), a wire is drawn thicker with its ports
ringed and a tooltip naming its ends; a click selects it, Ctrl or Shift
adds it; Delete removes the wires selected as one `Edit::Disconnect`;
its menu disconnects it or selects the node at either end; a drag on it
moves its nearer end, one `Edit::Connect` whose `replaces` holds the old
wire (dropped nowhere: a disconnect). From the keyboard, Tab past a
connected input reaches each of its wires (Delete, Enter and Shift+Enter
pick its input or output end up, Escape), each an AccessKit button
described as "wire". `GraphState::selected_edges` holds the wires
selected; `graph.select` takes `edges`, `graph.read` gives
`selected_edges`, and `graph.edit`'s `connect` honours `replaces`.

To move: `Request` gains `SelectWires` and `GraphEvent` gains
`WireSelection` (`graph.wire_selection_changed` on the journal); a
`match` on either needs the new arm. `graph.select` with `nodes` alone
now also clears the wires selected.
