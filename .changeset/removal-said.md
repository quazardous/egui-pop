---
egui-pop-model: minor
egui-pop-api: patch
---

A plugin's going is said (ADR 0028). Its disconnection leaves a `plugin.disconnected` event on the journal, with why (`removed`, `disabled`, `requirement gone`, `failed connect`) and what went by kind (commands, panels, undo steps and those mixing its values with others'), and an info line in the log. A run of one of its commands asked after it went (queued, a menu, a toast's or a job's action, an accepted dialog, a key) is a `command.refused` event naming its plugin, and so is a run of a command never declared; a key still bound to a gone command logs a warning, not an info line. The value store's new `Store::remove_owner_report` says which undo steps went with an owner, per history, and logs them.
