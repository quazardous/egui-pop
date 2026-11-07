---
egui-pop-tokens: minor
egui-pop-core: major
egui-pop-api: major
egui-pop: major
egui-pop-editors: major
egui-pop-i18n: major
egui-pop-imaging: major
egui-pop-platform: major
egui-pop-test: major
egui-pop-viewport: major
egui-pop-views: major
---

egui-pop moves to egui_stack 0.25, and the kit draws the lines between panels: a kit can leave at most one line where two panels meet.

- **`dock-body`.** The border of a panel's body in a dock is a kit component, painted through egui_stack's new `DockPainter::tab_body`: its `border-width`, `border-color` and `radius`, and a glow (`ring-color`, `ring-width`, `ring-offset`), at `rest`, under the pointer (`hover`) and for the dock's focused panel (`focus`). The default kit outlines every body in `border.default`, one point wide, with the widgets' radius, as before; a frameless area still draws none.
- **`dock-separator`.** The line between two panels of a dock is a kit component of its own, no longer the `splitter` of `widgets::Splitter`: its `width`, and its `fill` at `rest`, `hover` and `active` (pressed or dragged), its ring at `focus`. The default kit draws it as the splitter, so the look is unchanged.
- **The Rerun-like kit** (`examples/kits/rerun-like`) outlines no body, hides the dock's separator at rest (it shows under the pointer and while dragged) and shows the scroll bars only while the pointer is over their area. Where two areas meet, the side area's separator is the one line left; between two panels of one area, none at rest.

Migration: egui_stack types re-exported by egui-pop, and the `DockPainter` the kit implements, are those of 0.25: see its changelog (no body border nor separator line at rest in `Style::from_egui`; `DockPainter::tab_body`). A kit that restyled the dock's separators through `splitter` sets the same terms on `dock-separator`.
