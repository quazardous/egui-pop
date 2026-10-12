# The tray icon

`egui-pop-platform`'s tray puts an icon in the panel, the notification
area or the menu bar, with a menu that is an ordinary egui-pop menu:
plugins fill it as they fill any other, its items take their titles,
keys, ticks and greying from the commands, and a pick runs the command
through the host. The facade re-exports it with the `platform` feature
(or `platform-tray` alone):

```toml
egui-pop = { version = "0.1", features = ["platform-tray"] }
```

It wraps [tray-icon](https://crates.io/crates/tray-icon): the
StatusNotifierItem protocol over D-Bus on Linux and BSD (through ksni,
with no GTK), the notification area on Windows, the menu bar on macOS.

## A declared menu

The menu is `MENU_TRAY` (`app.tray`). A command placed in it, a
submenu, a list filled when it opens: all as in the window's menus. The
plugin keeps the system's menu current at each pass, so a `checked`
clause ticks the item as the context changes:

```rust
use egui_pop::egui;
use egui_pop::platform::tray::{Entry, MENU_TRAY, Simulated, TrayPlugin, Icon, STEP_ACTIVATE};
use egui_pop::prelude::*;

const PAUSE: CommandId = CommandId::from_static("player.pause");

struct Player;

impl Plugin for Player {
    fn id(&self) -> PluginId {
        PluginId::from_static("player")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.command(
            CommandSpec::new(PAUSE, "Pause", "Player")
                .checked("playerPaused")
                .menu(MENU_TRAY, "1_play", 0),
            |call| {
                let paused = call.app.context().get("playerPaused") == "true";
                call.app.set_context("playerPaused", if paused { "false" } else { "true" });
                Ok(serde_json::Value::Null)
            },
        )?;
        // A left click toggles instead of bringing the window up.
        ctx.replace(STEP_ACTIVATE, |app, _| {
            app.run(PAUSE, serde_json::Value::Null);
            Ok(serde_json::json!({ "toggled": true }))
        });
        Ok(())
    }
}

// `TrayPlugin::new("Player")` shows the system's icon; the simulated
// tray records what it would show.
let tray = Simulated::default();
let ctx = egui::Context::default();
let mut app = App::builder()
    .plugin(Player)
    .plugin(
        TrayPlugin::new("Player")
            .app_id("org.example.Player")
            .tooltip("Player: idle")
            .icon(Icon::disc(32, [0xe6, 0x61, 0x00]))
            .backend(tray.clone()),
    )
    .build(&ctx)
    .unwrap();
app.logic(&ctx);
assert!(matches!(&tray.menu()[0], Entry::Item { checked: Some(false), .. }));
tray.click("left");
app.logic(&ctx);
app.logic(&ctx);
assert!(matches!(&tray.menu()[0], Entry::Item { checked: Some(true), .. }));
```

## The step

| Step | Arguments | Default |
|---|---|---|
| `tray.activate` | `button`: `left`, `middle`, `double` | the main window comes to the front (shown, restored, focused) |

The right button opens the menu on every platform. A pick from the menu
is a command run, in the host's journal like any other.

## Hiding to the tray

eframe keeps running the application's logic while the window is
hidden, when something asks for a repaint: the tray's thread does on
each click, and the plugin takes it at the next pass (`On::Pass`). An
application that hides instead of closing vetoes `frame.close` (or
handles eframe's close request) and sends
`ViewportCommand::Visible(false)`; the default `tray.activate` shows
the window again.

## Platforms

| | Icon | Menu | Clicks |
|---|---|---|---|
| KDE Plasma, Xfce, Cinnamon, LXQt, sway's bars | yes (StatusNotifierItem) | yes | left, middle |
| GNOME | only with the AppIndicator extension (installed by default on Ubuntu and Fedora's workstation, not on vanilla GNOME) | yes | left, middle |
| Windows | yes | yes | left, middle, double |
| macOS | yes, in the menu bar | yes | left, middle |
| Web | no | | |

Where no tray host runs, the icon has nowhere to go and nothing says so
(the protocol has no answer for it); the `TRAY` service reports what
the backend could start. The icon is RGBA pixels (`Icon`); a themed icon
name is not used. Inside a Flatpak sandbox the plugin goes without a
well-known D-Bus name, as the StatusNotifierItem hosts accept; see
[Packaging](packaging.md#tray).
