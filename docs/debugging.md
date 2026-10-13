# Debugging

Every registry describes itself to the **debug bridge**: collectors for
commands, key resolution traces, focus, overlays, zones, plugins, steps,
the workbench, the store, the journal and diagnostics, each with a JSON snapshot and
actions flagged `mutates`; every call returns a feedback block. A person
reads it in the gallery, an agent or a test reads the same data as JSON.

```rust
use egui_pop::egui;
use egui_pop::prelude::*;

let ctx = egui::Context::default();
let mut app = App::builder().plugins(DefaultPlugins).build(&ctx).unwrap();
let snapshot = app.handle().debug_snapshot();
assert!(snapshot["commands"]["commands"].is_array());
let feedback = app.debug_call("commands", "run", &serde_json::json!({ "id": "app.palette" }));
assert!(feedback.ok && feedback.mutates);
```

The gallery's Debug page shows them, with the last key resolutions:

![the debug page](screenshots/debug-pop-dark.png)

Run headless, the gallery prints the same snapshot on stdout before it
quits (`--dump-debug`, see [tools/wbox](../tools/wbox)). What cannot
apply (a hook on a step nobody declares, two plugins replacing the same
step) is reported as a diagnostic, never a silent no-op
([ADR 0003](adr/0003-no-silent-no-op.md)).

## Devtools

The same collectors inside the application: `DevtoolsPlugin` (opt-in)
adds a bottom panel, opened with `F12` once the `developer.devtools`
setting is on (off by default, so end users never meet it). Its tabs
show the last key resolutions with each binding's verdict, the journal,
the steps with their hooks and last runs, the focus chain, overlays and
context keys, each window's scale (the `screen` collector), and every
collector as JSON with its actions. An action run from the panel goes
through the `devtools.call` step, which a plugin may veto (a kiosk
build, an action too dangerous to offer).

```rust
use egui_pop::api::devtools::{DevtoolsPlugin, PANEL_DEVTOOLS};
use egui_pop::egui;
use egui_pop::prelude::*;

let app = App::builder()
    .plugins(DefaultPlugins)
    .plugin(DevtoolsPlugin::default())
    .build(&egui::Context::default())
    .unwrap();
// Closed until asked for, and only once the setting allows it.
assert!(!app.handle().panel_visible(&PANEL_DEVTOOLS));
assert!(app.handle().debug_snapshot()["screen"]["windows"].is_array());
```
