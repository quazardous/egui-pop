---
egui-pop-agent: minor
egui-pop-test: minor
---

Agents read and drive the interface of a running application through
its agent socket:

- the socket's `ui` method: `tree` lists the accessibility tree of the
  frame just drawn, `click`, `type`, `key` and `scroll` act on a node
  found by label and role, as raw egui events queued into the next
  frames (headless or on a desktop); acting needs the bridge on;
- the MCP server offers it as the `ui` tool;
- live scenarios run `Ui` steps and `Expect(Tree)` over it.
- a scenario's node may be picked among several with `nth`, in
  reading order (`Ui(Click(label: "Import", nth: 0))`).
