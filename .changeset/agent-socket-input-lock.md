---
egui-pop-agent: minor
egui-pop-api: minor
egui-pop: minor
---

While an agent's confirmation is open on the agent socket (a destructive call's or a pairing request's), agents' `ui` actions are refused (`-32001`, "input is locked while the user answers an agent's request") and journaled as `debug.call.refused` with collector `ui` and `by: "pop.agent.confirm"`, so an agent cannot click its own confirmation; reading the tree still works and the user's input is untouched. A confirmation or pairing request that ends unanswered (its deadline passed, the socket stopped) now cancels its dialog, and an answer given late runs nothing. New `Dialogs::cancel(id, origin)` cancels a dialog by its id, shown or waiting, through the answer step.
