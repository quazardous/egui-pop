---
egui-pop-api: minor
egui-pop-model: patch
egui-pop: minor
---

Ctrl+Z can follow the focus instead of the newest step.
`history::set_routing(app, Routing::Focus)` sends Ctrl+Z and
Ctrl+Shift+Z to the history of the innermost focused surface that one of
the owner's scopes names among its focus targets (`ScopeInfo::focus`),
even when another history has a newer step; a surface can also declare
the history it works on (`Ctx::history_target`), without making that
history active or not. With no such surface focused
(the menu bar, nothing clicked yet), they go to the history focused last
while it is still shown, then to the newest. Redo goes to the same
history as undo. `Routing::Newest` stays the default. The history picked
goes through the new `pop.history.target` step, so a before-hook can
name another history or none. `history::focused_scope` and
`history::last_focused` say where the focus routes, and the `history`
collector shows the routing.
