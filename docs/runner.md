# The native runner, reads and waiting

Three parts of the family "Application API"
([ADR 0027](adr/0027-component-families.md)) that the guides
([Plugins, commands and steps](plugins.md), [Threading](threading.md))
do not cover: how the application runs natively, how a page reads from
the host without blocking, and how a test or an agent waits on the
application.

| Module | What it does |
|---|---|
| `egui_pop_api::runner` | The native runner: eframe on a winit event loop egui-pop owns (`create-native` feature) |
| `egui_pop_api::reads` | The read kernel: keyed reads from the host, never waiting, stale while revalidating |
| `egui_pop_api::wait` | Waiting until a journal event matches, or until the application is settled |

**Owns**: the event loop handler around eframe, the read cache and its
policies, the wait conditions shared by the test kit and the bridge.

**Refuses**: an async runtime (reads run on a thread of the kernel's,
waits are checked per frame); an application's kernel (the host gives
the transport); a windowing system of its own (eframe and winit paint
and create every window).

## The native runner

With the `create-native` feature (on by default),
`AppBuilder::run_native` and `run_native_with` build the winit event
loop, hand eframe to it through `eframe::create_native`, and run it with
a handler that sees every winit callback before eframe. eframe still
creates and paints every window; owning the loop adds:

- **The screens before the main window**
  (`egui_pop_api::runner::screens`): the place restored from the last run
  is repaired against the real monitors, not only bounded.
- **Every window's raw handles**
  (`egui_pop_api::runner::window_handle`): eframe hands out the main
  window's only. The handler pairs each painted window with its egui
  viewport and records its handles where the file chooser's parent and
  the frame's hit testing read them. They are used only once winit's
  ids were checked against eframe's own handles for the main window
  (`egui_pop_api::runner::ids_verified`).

- **Each viewport's window, by its id** (`egui_pop_api::runner::window`):
  a `Window` implementing `HasWindowHandle` and `HasDisplayHandle`,
  what a file chooser's parent, a Wayland subsurface or a Win32 subclass
  takes. The main window's is eframe's own window, known as soon as
  eframe creates it, on every system (macOS included); another deferred
  viewport's comes from its winit id, once painted and verified as
  above. `egui_pop_api::runner::windows` lists the viewports whose
  window is known, the main one first; a closed viewport leaves the list.

- **Paced paints**: a window paints at most once per interval, a little
  under the fastest screen's refresh period (60 Hz when no screen tells
  its rate); a redraw that comes sooner is held and painted at its time.
  Vsync and Wayland's frame callbacks pace a window already; this holds
  where the present does not wait (an X server without DRI3, Mesa's
  software rendering), where a window that animates would otherwise draw
  hundreds of frames a second. The wbox scenario
  `tools/wbox/multi_window.py` checks it on X11 and Wayland: over a few
  idle seconds the idle windows draw at most a few frames a second, the
  animating one at most its screen's refresh.

```rust
use egui_pop_api::runner;

fn diagnostics() -> String {
    format!("screens: {:?}, ids verified: {:?}", runner::screens(), runner::ids_verified())
}

fn windows_known() -> Vec<(egui::ViewportId, Option<runner::raw_window_handle::RawWindowHandle>)> {
    runner::windows()
        .into_iter()
        .map(|v| (v, runner::window(v).and_then(|w| w.raw_window())))
        .collect()
}
```

The handles name windows winit keeps: read them on the thread running
the application's frames (the runner's state is kept there; elsewhere
the functions answer nothing), and keep them no longer than the viewport
stays open.

The windows the runner knows are one side of a check
[ADR 0029](adr/0029-not-on-the-bus.md) decides: the window registry (the
one list of what egui-pop believes is shown, fed by `window.open` and
`window.close` on the bus) must equal the windows that exist, every
pass. Until its slices land, each module shows and closes its own
viewports.

Immediate viewports (painted inside their parent's pass) are not
paired. On the web, and without the feature, eframe runs as before and
these functions answer nothing (`Vec::new()`, `None`).

## Reads

A page reads what the host knows (a status's value, a list's rows)
through a `ReadKernel` over a `ReadTransport` the host gives:

```rust
use std::sync::Arc;
use std::time::Duration;
use egui_pop_api::reads::{ReadKernel, ReadPolicy, ReadState, ReadTransport};
use serde_json::{Value as Json, json};

struct Host;
impl ReadTransport for Host {
    fn serves(&self, command: &str) -> bool {
        command == "storage.free"
    }
    fn read(&self, _command: &str, _args: &Json) -> Result<Json, String> {
        Ok(json!("120 GB"))
    }
}

let kernel = ReadKernel::new(Arc::new(Host));
kernel.policy(
    "storage.free",
    ReadPolicy::new().fresh_for(Duration::from_secs(30)).invalidated_by(&["volume.*"]),
);
let first = kernel.read("storage.free", &json!({}));
assert!(matches!(first.state, ReadState::Pending | ReadState::Fresh));
```

- **Keyed**: a read is a command and its arguments; two pages asking the
  same share one entry and one request in flight.
- **At once**: `ReadKernel::read` never waits; it answers a `Read` in a
  `ReadState`: `Pending`, `Fresh`, `Refreshing` (the last value while a
  new one is read) or `Failed` (the error, and the value before it if
  any). `ReadKernel::peek` answers without starting anything.
- **Stale while revalidating**: a value past its freshness stays shown,
  `Fresh`, while it is read again; one an event made out of date
  (`ReadKernel::event`, `invalidate`, `invalidate_all`) is `Refreshing`.
- The kernel repaints the context set with `ReadKernel::set_context`
  when an answer comes. On the web, or when no thread starts, the
  transport runs at once.

The Settings app reads through it the commands its transport serves
(`egui_pop_api::admin::Admin::attach_reads`); late values draw a read's
states ([Late values](late-values.md)).

## Waiting on the application

`AppHandle::wait_met` says whether a `WaitFor` holds now, for the
journal's events after a sequence number: an event by name (and
optionally its cause), `Settled` (no command queued or running, every
job idle, no repaint asked), or `Any` of several. A job orphaned by its
disconnected plugin ([Progress](progress.md)) is not waited on: it may
never end; `AppHandle::orphaned_jobs` counts it, and `bridge.wait`'s
answer carries the count (`orphaned`) beside `met` and `last`. It never
blocks: who waits checks again after each frame. The test kit's
`egui_pop_test::wait::wait_until` steps frames until it holds or a deadline
passes; the bridge's `bridge.wait` holds the call in the transport.

```rust
use egui_pop_api::wait::WaitFor;
use egui_pop_api::{App, DefaultPlugins};

let app = App::builder().plugins(DefaultPlugins).build(&egui::Context::default()).unwrap();
let since = app.handle().last_event();
let _ = app.handle().wait_met(&WaitFor::event("command.done"), since);
```

## Failure behaviour

- Runner: handles not verified (`ids_verified` false) are never
  recorded, so what reads them (the file chooser's parent, the frame's
  hit testing, `runner::window`) gets nothing for windows other than the
  main one; the main window's stays known, from eframe.
- Reads: a failed read keeps the value before it, so a page shows the
  value and the error beside it; a command no transport serves is not
  the kernel's (`ReadKernel::serves`) and keeps going through the
  command bus.
- Waits: an unknown argument shape is refused by `WaitFor::from_args`
  with the reason; past the deadline the test kit's error names the
  condition, the frames stepped and the journal's last events.

## Accessibility

None of its own: what a read feeds is drawn by its family (a late value
says "Loading" and is `busy` while pending).

## Tier

From [API stability](stability.md): **Experimental** ("Application
windows (`egui_pop_api::windows`) and the native runner
(`egui_pop_api::runner`, the `create-native` feature, not yet released):
the run loop they share may still move"; "The read kernel
(`egui_pop_api::reads`) and waiting on the application
(`egui_pop_api::wait`): their states are kept, their API may move").

## Tests

- Runner: unit tests in `crates/egui-pop-api/src/runner.rs` (a place
  repaired against screens, no window before the loop runs); the window
  ids are checked per system by `egui-pop-winit`'s `window_id` test. The
  run loop itself has no headless test (it needs a display): the
  gallery's `multi_window` example, run under wbox on the box, reports
  each viewport's window and display handles (`runner` and `opened`
  lines).
- Reads: unit tests in `crates/egui-pop-api/src/reads.rs`; through the
  Settings app in `crates/egui-pop-api/tests/admin.rs`.
- Waits: `crates/egui-pop-test/tests/wait.rs` and the module's doc test.
