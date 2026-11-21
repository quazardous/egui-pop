---
egui-pop-api: minor
egui-pop: minor
---

Panel instances get their kind's keys and undo scope. The context key
`focusedPanelKind` (`CONTEXT_FOCUSED_PANEL_KIND`) is the focused panel's
kind, set beside `focusedPanel`: a binding with `when:
"focusedPanelKind == 'app.gallery'"` fires in the panel declared and in
each instance `app.gallery#<n>`. A history scope whose focus target names
a panel kind (`ScopeInfo::focus`) is active while one of its instances is
drawn, and Ctrl+Z routes to it while one has the focus; a target naming
one panel or one instance exactly still names that one only.
