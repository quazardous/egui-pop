---
egui-pop-api: minor
egui-pop-i18n: patch
---

The Settings app (`egui_pop_api::admin`, `AdminPlugin`): every declared
setting as full pages generated from the declarations, with no
hand-written page. The scopes (General, Interface, Input, Developer, and
those manifests add) are tabs on top, their sections a left panel, and a
search at the end of the tab row finds settings in every scope, editable
in place under their breadcrumb. It opens as an overlay of the whole main
window or, with "Pop out", in a window of its own; the mode used last is
the `admin.window` setting. Each row has a modified bar, an origin tag
(`library`, `workspace`, `command line`), its reset of the layer shown and
a "Set for" menu when the application opens a library or a workspace
layer (`Admin::offer`, `Admin::load_layer`, `Admin::export_layer`); values
apply live and are undone with the settings' history, and the user's
settings file keeps the user's own value under a layer's. A command line
override (`Admin::set_run`) is shown, never edited. Manifests
(`Admin::manifest`) add scopes and sections rendered from data (fields,
actions running their command with the argument template, statuses from
hooks, callouts), hooks are registered by name (`Admin::hook`) and widget
kinds have a stub registry (`Admin::widget_kind`). Commands: `admin.open`
(`{"path": "scope/section/key"}`, `{"key": "<setting id>"}`, `{"mode":
"overlay" | "window"}`), `admin.close`, `admin.popOut`, `admin.set` and
`admin.reset` (a value in a scope); the `admin` collector shows the
resolved tree. Every row is a named node for assistive technology. The
settings window stays, with `Ctrl+,`, until the migration replaces it.
Its strings are in English and French.
