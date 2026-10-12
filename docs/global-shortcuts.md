# Global shortcuts

A global shortcut runs its command while the application is not
focused: play or pause from any window, a quick capture, a push-to-talk.
In egui-pop it is an ordinary key binding in the **system** scope: it is
declared with its command, listed and rebound in the shortcuts editor,
saved in keymap files, and inside the window it fires as a global
binding does. `egui-pop-platform` registers these bindings with the
desktop; the facade re-exports it with the `platform` feature (or
`platform-keys` alone):

```toml
egui-pop = { version = "0.1", features = ["platform-keys"] }
```

## Declaring one

```rust
use egui_pop::egui;
use egui_pop::api::system_keys::{KeyStatus, SYSTEM_KEYS};
use egui_pop::model::keymap::BindOptions;
use egui_pop::platform::keys::{Simulated, SystemKeysPlugin, STEP_PRESSED};
use egui_pop::prelude::*;

const TOGGLE: CommandId = CommandId::from_static("player.toggle");

struct Player;

impl Plugin for Player {
    fn id(&self) -> PluginId {
        PluginId::from_static("player")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.command(
            CommandSpec::new(TOGGLE, "Play or pause", "Player")
                .key_with("Ctrl+Alt+P", BindOptions::system()),
            |_| Ok(serde_json::Value::Null),
        )?;
        // Count the presses that come from outside the window.
        ctx.after(STEP_PRESSED, |app, args, _| {
            app.publish("player", None, "player.global_key", args.clone());
        });
        Ok(())
    }
}

// `SystemKeysPlugin::new("org.example.Player")` registers them with the
// desktop; the simulated desktop records them.
let desktop = Simulated::default();
let ctx = egui::Context::default();
let mut app = App::builder()
    .plugins(DefaultPlugins)
    .plugin(Player)
    .plugin(SystemKeysPlugin::new("org.example.Player").backend(desktop.clone()))
    .build(&ctx)
    .unwrap();
app.logic(&ctx);
let keys = app.handle().service(&SYSTEM_KEYS).unwrap();
assert_eq!(keys.status("Ctrl+Alt+P"), Some(KeyStatus::Registered));

// The user picks other keys in the shortcuts editor: they stay global.
app.handle().rebind(TOGGLE, Some("Ctrl+Alt+F9".into()));
app.logic(&ctx);
app.logic(&ctx);
assert_eq!(desktop.registered()[0].keys, "Ctrl+Alt+F9");
desktop.press("Ctrl+Alt+F9");
app.logic(&ctx);
assert_eq!(app.handle().history()[0].id, TOGGLE);
```

In a keymap file the scope is written `"scope": "system"`.

## What the shortcuts editor says

The plugin provides the `SYSTEM_KEYS` service
(`egui_pop::api::system_keys`): whether the desktop takes global
shortcuts at all, and each binding's state. The shortcuts editor shows
it next to the command:

- *Ctrl+Alt+P also works outside the window*: registered;
- *Ctrl+Alt+P outside the window: waiting for the desktop*: asked, the
  desktop has not answered (a portal asking the user);
- *Ctrl+Alt+P does not work outside the window: taken by another
  application* (or the desktop's reason, or *a global shortcut is a
  single chord* for `Ctrl+K Ctrl+P`);
- *Ctrl+Alt+P works only in the window: (reason)*: the desktop takes no
  global shortcuts (a Wayland compositor without the portal, the web);
- *... no plugin registers global shortcuts*: the application did not
  add the plugin.

Nothing fails silently: a binding that cannot work outside the window
says so where the user changes it.

## The step

| Step | Arguments | Default |
|---|---|---|
| `keys.system` | `command`, `keys`, `backend` | the command runs through the host |

## Platforms

| | Through | Notes |
|---|---|---|
| Windows | global-hotkey (`RegisterHotKey`) | keys another application holds are refused, and said so |
| macOS | global-hotkey (Carbon hot keys) | `Ctrl` in egui-pop's keys is Command; no accessibility permission is needed |
| Linux and BSD, X11 | global-hotkey (an X grab on the root window) | checked end to end under Xvfb with XTest; refused keys are said so |
| Linux, Wayland | the XDG GlobalShortcuts portal | GNOME 48 and later, KDE Plasma 5.27 and later; the desktop asks the user to accept the shortcuts and may let them choose other keys; outside a sandbox the portal needs the application's desktop file, `<app id>.desktop`, installed, else it refuses ("an app id is required") and the editor says so |
| Wayland without the portal (sway, older GNOME) | nothing | the editor says the shortcuts work only in the window |
| Web | nothing | as above |

On Wayland the backend is the portal even when XWayland runs: an X grab
there only sees keys going to X11 windows. `detect` picks the backend
for the session; `HotKeys`, `Portal` and `Simulated` can be given to
`SystemKeysPlugin::backend` directly. See
[Packaging](packaging.md#global-shortcuts) for the desktop file.
