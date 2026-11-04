---
egui-pop-core: minor
egui-pop-viewport: minor
---

A large node graph is one Tab stop: from twelve nodes (`graph::ONE_STOP_FROM`),
Tab enters the graph on the node or the canvas focused there last and leaves it
from anywhere in it, the arrows go between nodes, Enter goes into a node's parts
(chevron, ports, sliders, header) where Tab goes round, and Escape comes back to
the node. `GraphEditor::tab_stops` chooses (`TabStops::Auto`, `One`, `Each`).
In egui-pop-core, `tab::parts` makes a widget's parts entered by Enter and left
by Escape, `tab::Axis::Own` a roving group whose widgets move the focus with the
arrows themselves, and a roving group takes in what it draws on other layers
(a scene's) below menus and popovers, which keep their own.
