# A single instance and deep links

An application that should run once per user (a photo library, a
player, a chat) makes a second launch hand its arguments to the running
one, which comes to the front and opens what it was given: files,
`myapp://` links. `egui-pop-platform` does it with a local socket named
after the application; the facade re-exports it as
`egui_pop::platform` with the `platform` feature (or
`platform-instance` alone):

```toml
egui-pop = { version = "0.1", features = ["platform-instance"] }
```

## At the top of `main`

`instance::single` runs before any window. The first launch listens and
gets `Single::First`; a later one connects, hands over its arguments,
its working directory and its activation token, gets `Single::Handed`
and exits:

```rust,no_run
use egui_pop::platform::instance::{self, InstancePlugin, Single};
use egui_pop::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let first = match instance::single("org.example.Viewer")? {
        Single::Handed => return Ok(()),
        Single::First(first) => first,
    };
    App::builder()
        .plugins(DefaultPlugins)
        .plugin(
            InstancePlugin::new(first)
                .scheme("viewer")
                .links(CommandId::from_static("viewer.open_link")),
        )
        .run_native("Viewer", Default::default())?;
    Ok(())
}
```

The socket is an abstract Unix socket on Linux (a connection from
another user is refused), a file in `$XDG_RUNTIME_DIR/app/<id>` inside a
Flatpak sandbox (the directory the application's sandboxes share, since
each may have a network namespace of its own), a named pipe on Windows,
and a socket in `/tmp` on the other Unix systems; its name holds the
application's id and the user's name. Nothing else is written.

## The steps

| Step | Arguments | Default |
|---|---|---|
| `instance.second_launch` | `args`, `cwd`, `links` (the arguments in one of the plugin's schemes) | the main window comes to the front, then each link runs `deeplink.open` |
| `deeplink.open` | `url` | the plugin's `links` command runs with `{ "url" }`; without one the step fails and says so |

A click on a link in one of the plugin's schemes inside the
application (the [links service](links.md)) opens it in-process, with
no round trip through the system: the plugin adds its schemes to the
links policy at start, and its hidden command `deeplink.open` (`url`)
runs the same step.

`deeplink.open` also runs at start for the links the first launch was
given (`viewer://album/3` on its command line). A plugin vetoes
`instance.second_launch` to stay in the background, replaces it to open
the files a launch names, or follows `deeplink.open` to log links:

```rust
use egui_pop::egui;
use egui_pop::platform::instance::{
    Instance, InstancePlugin, LAUNCHES, Launch, STEP_SECOND_LAUNCH,
};
use egui_pop::prelude::*;

struct Viewer;

impl Plugin for Viewer {
    fn id(&self) -> PluginId {
        PluginId::from_static("viewer")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.command(
            CommandSpec::new(CommandId::from_static("viewer.open_link"), "Open a link", "Viewer"),
            |call| Ok(call.args["url"].clone()),
        )?;
        // Files named by a second launch open here; links still go to
        // `deeplink.open`.
        ctx.after(STEP_SECOND_LAUNCH, |app, args, _| {
            for file in args["args"].as_array().into_iter().flatten() {
                app.publish("viewer", None, "viewer.file", file.clone());
            }
        });
        Ok(())
    }
}

let ctx = egui::Context::default();
let mut app = App::builder()
    .plugin(Viewer)
    .plugin(
        InstancePlugin::new(Instance::detached(Launch::default()))
            .scheme("viewer")
            .links(CommandId::from_static("viewer.open_link")),
    )
    .build(&ctx)
    .unwrap();
// A launch handed over in-process, as the socket would.
app.handle().service(&LAUNCHES).unwrap().deliver(Launch {
    args: vec!["viewer://album/3".into()],
    ..Launch::default()
});
app.logic(&ctx);
app.logic(&ctx);
let ran: Vec<_> = app.handle().history().into_iter().map(|r| r.id.to_string()).collect();
assert_eq!(ran, ["viewer.open_link"]);
```

`Instance::detached` is an instance listening to nothing, for tests and
for applications that are single some other way; `Launches::deliver`
hands a launch over in-process.

## Platforms

| | Single instance | Coming to the front |
|---|---|---|
| Linux, X11 | yes | yes |
| Linux, Wayland | yes | the window asks for attention: the compositor gives the focus only with an activation token, which egui does not pass on to the window |
| Windows | yes (named pipe) | yes, within the focus-stealing rules of Windows (the taskbar button flashes otherwise) |
| macOS | yes | yes; macOS itself usually keeps a bundle single and sends links as Apple events, which winit does not hand to egui yet: links reach a bundled application only on its command line |
| Web | no (each tab is its own) | no |

Registering the `viewer:` scheme with the system (a desktop file's
`MimeType=x-scheme-handler/viewer`, a registry key, the bundle's
`CFBundleURLTypes`) is the package's part: see
[Packaging](packaging.md#single-instance-and-deep-links).
