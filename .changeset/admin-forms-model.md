---
egui-pop-model: minor
---

`egui_pop_model::admin`: the administration app's declarative form tree
(`docs/design/admin-forms.md`). A `Manifest` (versioned, `FORMAT` 1)
holds scopes, sections, groups and their items: fields of the ten kinds
(bound to `settings/<id>` or a task's draft), actions (a command and an
argument template, `${field}`), statuses, callouts and registered widget
kinds. Rust builders write it, it round-trips through JSON, a newer
format is refused, and `Manifest::check` leaves out what does not resolve
with a diagnostic. `Tree` merges manifests and resolves deep links
(`scope/section/key`, or a setting id alone). `admin::hooks` registers
the hooks a manifest names (validate, choices, preview, transform,
confirm, status), which compute and never draw; `admin::layers` keeps
the value scopes (app, library, workspace, and the run's overrides, read
only) with the origin of each value; `admin::generate` builds the pages
of the declared settings.
