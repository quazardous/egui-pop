# Debugging

Every registry describes itself to the **debug bridge**: collectors for
commands, key resolution traces, focus, overlays, zones, plugins, steps,
the workbench, the store (each path's owner, writers and last writer,
and the writes refused to a scope that is not a writer), the journal,
diagnostics and the mount
contract (`mounts`: every component and host declared, what is mounted
where, `where_can` and `check`, see [hosts.md](hosts.md)), the fonts
(`fonts`: faces, fallback chains, sizes, and `check` for the chars a
family misses, see [fonts.md](fonts.md)), the settings window
(`settings_window`: open or not, the page shown, the search, the tree and
the custom pages), the pointer (`pointer`: the profile in force, the
hint, and the widgets that claim the secondary button, see
[pointer-gestures.md](pointer-gestures.md)), each with a JSON snapshot and
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

## State in egui's memory

egui-pop keeps its per-context state (tokens, fonts, the frame's
layout, focus, overlays, a slider's drag) in egui's temporary memory,
through typed slots (`egui_pop::core::state::Slot`). Every slot written
is listed by the `state` collector: its name, type, whether it holds one
value per context or one per instance (a widget, a window), how many
entries it holds now and a one-line summary of the first ones. A count
of instance entries that keeps growing while the interface does not is
a leak; the `slot` action lists every entry of one slot.

```rust
use egui_pop::core::debug::DebugBridge;
use egui_pop::core::state::Slot;
use egui_pop::egui;

// A component's own state, declared once.
const OPEN: Slot<bool> = Slot::new("my_plugin.open");

let ctx = egui::Context::default();
OPEN.keyed("first").set(&ctx, true);
let bridge = DebugBridge::with_core_collectors();
let state = bridge.snapshot_of(&ctx, "state").unwrap();
let open = state["slots"]
    .as_array()
    .unwrap()
    .iter()
    .find(|s| s["name"] == "my_plugin.open")
    .unwrap();
assert_eq!((open["scope"].as_str(), open["entries"].as_u64()), (Some("instance"), Some(1)));
```

egui locks its context while its memory is read or written: a slot
takes the lock itself, so build keys (`ctx.viewport_id()`, a widget's
id) and read other state before calling it. Only `update`'s closure
runs under the lock, and it must not touch the context; a slot touched
there panics with both names instead of deadlocking.

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
