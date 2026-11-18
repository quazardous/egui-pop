---
egui-pop: minor
egui-pop-api: minor
egui-pop-core: minor
---

egui_stack 0.26: a dock's separators are named after the tabs on each side for screen readers ("Resize between Files and Outline", was "Resize"), a tab starts dragging 6 points from the press, and a drop over a dock leaves its tab bar out, a band under the bar stacking the tab there. Code that builds egui_stack's `OverlayFeel` itself now finishes the literal with `..Default::default()`.
