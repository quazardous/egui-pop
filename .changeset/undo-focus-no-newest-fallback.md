---
egui-pop-api: minor
egui-pop-i18n: minor
egui-pop: minor
---

Under `Routing::Focus`, Ctrl+Z never falls back to the newest step of
another scope. With no view focused, it goes to the scope of the view
focused last in that window, shown or not, while the owner still has
it; with none focused yet (at start-up), it does nothing, a toast says
"Nothing to undo here: click a view first" (new strings
`egui-pop-history-nothing-focused` and `-redo`, with their French), and
`history.refused` is published with the kind `nothing_focused`. The
`pop.history.target` step names this case with the rule `none`
(`history::NOTHING_FOCUSED`), so a before-hook may pick a scope.
`Routing::Newest` is unchanged.
