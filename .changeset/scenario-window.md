---
egui-pop-test: major
egui-pop-agent: major
egui-pop: major
---

A scenario names the application window its steps look at:
`Window("import")` makes the `Ui` steps and `Expect(Tree)` that follow
read and act in that window (`main` for the main window), until the
next `Window`. Live, they go to that window's own tree and input; in a
kittest harness, where the windows are embedded in the main one, to
its embedded window. `Step` gains the `Window` variant, a break for
code matching every step. The agent socket's `ui` method (and the MCP
`ui` tool) takes `window` as `call` and `snapshot` do: a window of its
own has its own tree and input, its root node named by the window's
title, and where egui embeds the windows, a window is its embedded
window and what is in it, and `main` the rest.
