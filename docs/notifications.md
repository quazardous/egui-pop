# Notifications

`egui-pop-platform`'s notifications show on the desktop (the XDG
notification server on Linux and BSD, the notification centre on macOS,
toasts on Windows, all through notify-rust) with actions that run
commands, and fall back to egui-pop's in-app toasts where the desktop
shows none. The facade re-exports them with the `platform` feature (or
`platform-notify` alone):

```toml
egui-pop = { version = "0.3", features = ["platform-notify"] }
```

## Showing one

A `Notification` has a title, a body, a level, actions (an id, a label,
a command and its arguments) and a command for a click on its body.
`notify::show` shows one from the interface thread; `Notifier::post`
(the `NOTIFIER` service) queues one from any thread, shown at the next
pass:

```rust
use egui_pop::egui;
use egui_pop::platform::notify::{self, Level, Notification, NotifyPlugin, Simulated, NOTIFIER};
use egui_pop::prelude::*;

let ctx = egui::Context::default();
// `NotifyPlugin::new("Viewer")` shows them on the desktop; the
// simulated desktop records them instead.
let desktop = Simulated::default();
let mut app = App::builder()
    .plugins(DefaultPlugins)
    .plugin(NotifyPlugin::new("Viewer").backend(desktop.clone()))
    .build(&ctx)
    .unwrap();

let export = Notification::new("Export done", "3 images in Pictures")
    .level(Level::Success)
    .action("show", "Show", CommandId::from_static("files.reveal"), serde_json::json!({ "path": "Pictures" }))
    .on_click(CommandId::from_static("export.open"), serde_json::Value::Null);
let out = notify::show(app.handle(), export);
assert_eq!(out.result().unwrap().as_ref().unwrap()["via"], "desktop");

// From a worker thread:
let notifier = app.handle().service(&NOTIFIER).unwrap();
let n = (*notifier).clone();
std::thread::spawn(move || n.post(Notification::new("Import done", "120 images")))
    .join()
    .unwrap();
app.logic(&ctx);
assert_eq!(notifier.history().len(), 2);
```

## Where it shows

The `notifications.desktop` setting decides: `always` (the default) on
the desktop, `unfocused` on the desktop only while the window does not
have the focus, `never` in the window. A notification goes to a toast
when the setting says so, when the desktop has no notification server,
or when showing it failed; a failure arrives later (desktop
notifications are shown from a thread of their own, since the D-Bus
call can block) and publishes `notify.failed` with the error. A toast
carries the notification's first action.

## Toasts

The in-app toasts are the `ToastsPlugin`'s (in `DefaultPlugins`;
`egui_pop::api::toasts`): a level, a text, one action, a close button,
and a job's progress when given one. Information and success go after
4 s, warnings after 8 s, errors stay until closed; a thin bar in the
level's colour empties as the time runs. The pointer over a toast stops
its time (and its bar); it goes on from where it stopped once the
pointer leaves.

They show in the corner the `appearance.toast_corner` setting chooses,
under Appearance: `top-left`, `top-center`, `top-right`, `bottom-left`,
`bottom-center` or `bottom-right` (the default). An application gives
its own default with `AppBuilder::toast_corner`:

```rust
use egui_pop::api::toasts::Corner;
use egui_pop::prelude::*;

let app = App::builder()
    .plugins(DefaultPlugins)
    .toast_corner(Corner::TopRight)
    .build(&egui_pop::egui::Context::default())
    .unwrap();
assert_eq!(egui_pop::api::toasts::corner(app.handle()), Corner::TopRight);
```

The newest is always the nearest to the edge: at the bottom under a
bottom corner, on top under a top one. A toast slides in from its
corner's side and out again when it goes, in `motion.toast.slide`
(200 ms) with a cubic ease-out; in the middle of an edge it comes from
the edge and fades. Under reduced motion it appears and goes at once,
and its bar steps once a second.
Toasts cast the shadow of what lies over the interface: `toast.shadow-*`,
aliases of `shadow.popup.*`, as menus and popovers have it.

Their state is data: `Toasts::shown` (each with its `phase`, `appear`,
`idle`, its time `left` as the bar shows it, and whether the pointer
`held` it), `Toasts::leaving` (gone, still sliding out: painted only,
with nothing to press and nothing for a screen reader) and
`Toasts::history`, all in the debug bridge's `toasts` collector. Their
time runs on the motion clock (`core::motion`), so a test freezes it
and steps it.

The slide with its appear, idle and disappear states, the time-left bar
that stops under the pointer, the corner as a choice, the newest nearest
the edge and the shadow are ideas taken from
[egui-notify](https://github.com/ItsEthra/egui-notify) (MIT; see
`NOTICE`).

## The steps

| Step | Arguments | Default |
|---|---|---|
| `notify.show` | the notification's fields and `id` | to the desktop or a toast, as above; the result is `{ "id", "via" }` and, for a toast, `reason` |
| `notify.action` | `id`, `action` (an action's id, `default` for a click on the body), `command`, `args` | the command runs through the host; `default` also brings the window to the front |

A toast's button and a desktop button both go through `notify.action`
(the toast runs the hidden `notify.act` command). A plugin vetoes
`notify.show` for a do-not-disturb mode, replaces it to send
notifications elsewhere, and follows `notify.action` to count what users
pick:

```rust
use egui_pop::egui;
use egui_pop::platform::notify::{self, Notification, NotifyPlugin, Simulated, STEP_SHOW};
use egui_pop::prelude::*;

struct DoNotDisturb;

impl Plugin for DoNotDisturb {
    fn id(&self) -> PluginId {
        PluginId::from_static("dnd")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.before(STEP_SHOW, |_, args| {
            if args["level"] == "error" { Before::Go } else { Before::Veto("busy".into()) }
        });
        Ok(())
    }
}

let app = App::builder()
    .plugins(DefaultPlugins)
    .plugin(NotifyPlugin::new("Viewer").backend(Simulated::default()))
    .plugin(DoNotDisturb)
    .build(&egui::Context::default())
    .unwrap();
let out = notify::show(app.handle(), Notification::new("Synced", ""));
assert_eq!(out.vetoed(), Some("busy"));
```

## Platforms

| | Shows | Actions | Click on the body |
|---|---|---|---|
| Linux, BSD | through the XDG notification server (GNOME Shell, KDE Plasma, dunst, mako...) | yes, when the server offers actions | yes |
| Windows | as Windows toasts | yes (buttons) | not reported by notify-rust: nothing runs |
| macOS | through the notification centre | the first action, as the notification's button | yes |
| Web | no: toasts | in the toast | no |
| No server (a bare X session, a container) | toasts | the first, in the toast | no |

Gaps: a toast shows one action (the first). Each desktop notification
waiting for an answer holds a thread until it is closed. Inside a
Flatpak sandbox the notification server is reached through
`--talk-name=org.freedesktop.Notifications` (the portal's notification
interface is not used yet).

The desktop knows the application by its id, given with
`NotifyPlugin::new("Viewer").backend(Desktop::new("Viewer").app_id("org.example.Viewer"))`:
its desktop file's name on Linux (the `desktop-entry` hint, so the
server shows its icon and its settings apply), its AppUserModelID on
Windows (without one, notifications show as PowerShell's), its bundle
identifier on macOS. See [Packaging](packaging.md#notifications).
