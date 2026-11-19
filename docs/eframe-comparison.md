# eframe and egui-pop

egui-pop runs its applications through [eframe](https://github.com/emilk/egui/tree/main/crates/eframe),
egui's official application framework, behind the `eframe` feature. This
page compares the two: what eframe gives, what egui-pop adds on top, where
egui-pop works around eframe or duplicates it, and how egui-pop should
grow alongside it.

**The direction**: egui-pop does not replace eframe; it extends it. It
builds on eframe's extension points, stops duplicating what eframe does,
and treats what only eframe (or egui-winit, egui-wgpu, winit) can fill as
upstream proposals, with a workaround in egui-pop meanwhile that a
future upstream fix can replace without breaking egui-pop's API.

Versions read: eframe, egui, egui-winit and egui-wgpu 0.37.0 (the
versions in `Cargo.lock`), winit 0.30. Paths to eframe's sources are
given inside its crate (`eframe-0.37.0/src/...`), as published on
crates.io; paths to egui-pop's are from this repository's root. Line
numbers are those of the versions read.

## What eframe provides

| Area | What eframe 0.37 gives | Where |
|---|---|---|
| Native runner | `run_native` (winit event loop, one window per viewport), `run_ui_native` (a closure, no state), `create_native` returning an `EframeWinitApplication` to run on an event loop the application owns, with other windows it manages itself, or pumped (`pump_eframe_app`) | `lib.rs:288`, `lib.rs:478`, `lib.rs:376`, `native/run.rs:540`, `native/run.rs:610` |
| Renderers | wgpu (default) or glow (OpenGL), chosen by `NativeOptions::renderer`; the wgpu render state is handed to the application for custom painting | `epi.rs:331`, `epi.rs:943` |
| Web | `WebRunner` on a canvas, `WebOptions` (renderer, wgpu options, dithering), a text agent for IME and mobile keyboards, web storage | `web/web_runner.rs`, `epi.rs:514`, `web/text_agent.rs`, `web/storage.rs` |
| Lifecycle (`App`) | `logic` (also ticks while the window is hidden), `ui`, `save` and `auto_save_interval`, `on_exit`, `clear_color`, `persist_egui_memory`, `raw_input_hook` | `epi.rs:152` to `epi.rs:282` |
| Creation | `CreationContext`: the egui context, storage, the GL context or wgpu render state, the root window (`winit_window`) and its raw handles | `epi.rs:53`, `epi.rs:101`, `epi.rs:144` |
| Persistence | the `Storage` trait (strings), a file store in the platform's data directory (`storage_dir`), egui's memory, the root window's place restored and clamped to the monitors seen by the event loop (`persist_window`) | `epi.rs:1086`, `native/file_storage.rs:17`, `epi.rs:382`, `native/epi_integration.rs:32` |
| Viewports | egui's immediate and deferred viewports become winit windows; viewport commands (title, size, decorations, focus, attention, fullscreen, drag, resize) applied by egui-winit | `native/wgpu_integration.rs:640`, `native/wgpu_integration.rs:1203` |
| Window options | `NativeOptions::viewport` (an `egui::ViewportBuilder`: title, icon, app id, decorations, transparency, min/max size, position), `window_builder` (a last edit of the root's builder), `event_loop_builder` (platform tweaks to the event loop), `centered` | `epi.rs:299` to `epi.rs:370`, `native/epi_integration.rs:83` |
| Icon, title, IME, cursor | icon and title through the viewport builder (macOS app icon in `native/app_icon.rs`); IME and cursors through egui-winit's platform output | `native/app_icon.rs` |
| Accessibility | AccessKit per window through `accesskit_winit` on native; a screen reader path on the web | `native/wgpu_integration.rs:319`, `web/screen_reader.rs` |
| Clipboard and links | the system clipboard (`clipboard_shortcuts` decides whether the chords become `Copy`/`Cut`/`Paste` events or plain keys); `OpenUrl` opened in the browser (`links` feature) | `epi.rs:423` |
| Frame pacing | repaint on demand (`request_repaint`, deferred viewports repainting on their own), the wgpu present mode and frame latency (`WgpuConfiguration`, `Frame::set_wgpu_surface_config`), a hidden or minimized window sleeping | `epi.rs:378`, `epi.rs:965`, `native/wgpu_integration.rs:715` |
| Activation | an activation token requested from the windowing system, to hand to another process (Wayland's xdg-activation) | `epi.rs:881` |
| macOS chrome | traffic lights placed in a custom title bar | `epi.rs:839` |

### Its limits that matter to egui-pop

- **One window handle.** `CreationContext::winit_window` and
  `Frame::winit_window` give the root window. A deferred viewport's
  callback gets only an `egui::Ui`, never a `Frame`
  (`native/epi_integration.rs:347` to `:361`), so no viewport but the root
  hands out its winit window or raw handles.
- **No owner or parent window.** `ViewportBuilder` has no owner, and
  egui-winit's `create_winit_window_attributes` (egui-winit
  `lib.rs:2238`) sets none; winit 0.30 has one on Windows only. A tool
  window floats above every application; a native dialog cannot be
  attached to a secondary window.
- **The window builder hook reaches the root only**, once
  (`WindowBuilderHook` is a `FnOnce` over the root's `ViewportBuilder`,
  `epi.rs:42`, `native/epi_integration.rs:83`), and sees no monitors.
- **No monitors for the application.** eframe clamps its own saved
  window place to the monitors (`native/epi_integration.rs:42`), but an
  application restoring a place of its own gets no list of screens.
- **No surface format or colour space.** egui-wgpu picks the format
  (`preferred_framebuffer_format`, egui-wgpu `lib.rs:421`) and wgpu's
  default colour space (`configure_surface`, egui-wgpu `winit.rs:110`);
  `SurfaceConfig` carries the present mode and frame latency only
  (egui-wgpu `lib.rs:69`).
- **Nothing native beyond the window**: no menu bar, no tray, no native
  dialogs, no notifications, no single instance. eframe leaves them to
  other crates (muda, tray-icon, rfd, notify-rust).
- **Multi-window**: deferred viewports must be `Send + Sync` though
  they run on the main thread; one UI thread presents every window, so
  a FIFO present waiting on one compositor surface can stall the others
  (seen on a headless mutter, ADR 0025); winit cannot hide a window on
  Wayland; deferred viewports' rects stay empty on Wayland.
- **Testing**: eframe has no harness of its own; egui_kittest drives an
  `eframe::App` or a bare context, without real windows (no native scale
  change, no second OS window).

## What egui-pop adds on top

egui-pop's runtime is a layer over an `egui::Context`, not over eframe:
`App::logic` and `App::show_wrapped` run in any pass, and the eframe
glue is one `impl eframe::App` (`crates/egui-pop-api/src/app.rs:6891`)
plus `AppBuilder::run_native` and `run_native_with`
(`crates/egui-pop-api/src/app.rs:3984` to `:4057`). The same
application runs under eframe on native and on the web
(`crates/egui-pop-gallery/src/main.rs:77`), headless in kittest, and in
a test's bare context. What it adds:

- **The application model**: plugins that own what they register,
  commands, keymaps and `when` clauses, menus, panels and docking,
  zones and blocks, steps and hooks (ADR 0011), dialogs, the palette,
  toasts, undo, value binding and settings, admin forms.
- **Application windows** (ADR 0025, `crates/egui-pop-api/src/windows.rs`):
  deferred viewports with an `Rc` shim (a thread-local registry keyed by
  viewport, lines 12 to 17), titles from the content, the main window
  taking the content of the last window worked in when it is closed,
  geometry kept per window id (lines 32 to 36), a shell per window group,
  per-window keys and modals.
- **Its own window frame** (`crates/egui-pop-api/src/frame.rs`): a
  title bar drawn by the kit, with Windows' hit test answered through
  `egui-pop-win32` (Snap Layouts, lines 64 to 77) and window corners.
- **Platform services** (`crates/egui-pop-platform`): a single instance
  and deep links, notifications, a tray (tray-icon over ksni on Linux),
  global shortcuts, logs and crash reports, the system's text scale,
  focus mode and wheel lines, raising the main window.
- **Files**: an in-app file picker, and the system's chooser behind a
  feature, attached to the window it was asked from when its handles
  are known (`crates/egui-pop-api/src/files.rs:739`, `:782`).
- **Wide gamut**: the image viewport's own Wayland subsurface tagged
  through `wp_color_management_v1` (`crates/egui-pop-wayland`,
  `crates/egui-pop-viewport/src/image_surface.rs:417`).
- **Persistence of its own** (`egui_pop_core::persist`): versioned,
  migrated JSON documents per concern (settings, layouts, windows).
- **Agents and tests**: the debug bridge over a socket
  (`egui-pop-agent`), the test kit (`egui-pop-test`).

## Where egui-pop works around eframe

Each is an entry of [upstream.md](upstream.md), with an `// upstream:`
comment at the code site.

| Limit | egui-pop's workaround | Entry |
|---|---|---|
| No window handle for a viewport other than the root | the runner reads each window's handles from its winit id and records them per viewport (`files::set_parent_window`, `frame::set_native_window`); the root's still come from the creation context (`frame::record_native_window`). Their consumers still serve the main window only: the Windows hit test and the Wayland image surface | [upstream.md:499](upstream.md#eframe-no-window-handle-for-a-viewport-other-than-the-main-window) |
| No owner window (winit and egui) | palettes stay inside the main window; detached panels are ordinary top-level windows (U3 in [hosts.md](hosts.md)) | [upstream.md:478](upstream.md#winit-and-egui-no-owner-window-for-a-viewport) |
| No screens before the window is placed | the runner reads the monitors before eframe creates the main window, and the root's `window_builder` hook repairs the restored place against them (`WindowState::repair`); without the runner, the size is bounded and a far position dropped (`crates/egui-pop-api/src/window.rs:147`) | [upstream.md:612](upstream.md#eframe-no-list-of-screens-when-a-window-is-placed) |
| No surface format or colour space | the viewport reports sRGB truthfully; on Wayland the image gets its own surface | [upstream.md:660](upstream.md#egui-wgpu-no-surface-format-or-colour-space-to-choose), [upstream.md:729](upstream.md#wayland-no-colour-management-through-winit) |
| No hit-test hook on Windows (winit) | `egui-pop-win32` subclasses the root window | [upstream.md:628](upstream.md#winit-no-hit-test-hook-for-a-custom-title-bar-on-windows) |
| Custom cursors dropped on the web and in immediate viewports | a system cursor fallback per bitmap cursor | [upstream.md:430](upstream.md#eframe-no-custom-cursor-on-the-web-or-in-immediate-viewports) |
| Clipboard chords arrive as events | with a reading backend (`SystemClipboardPlugin`), `run_native` turns `clipboard_shortcuts` off and the keymap runs the chords, text fields included; elsewhere (the web, the default backend) the clipboard plugin maps the events to commands | [upstream.md:691](upstream.md#egui-winit-copy-cut-and-paste-arrive-as-events-their-keys-swallowed) |
| No release after a compositor move on Wayland (winit) | `frame::compositor_grab` (`crates/egui-pop-core/src/frame.rs:395`) | [upstream.md:458](upstream.md#winit-no-release-after-a-compositor-move-on-wayland) |
| A FIFO present stalls every window on Wayland | `windows::present_mode` picks `AutoNoVsync` there (`crates/egui-pop-api/src/windows.rs:450`) | ADR 0025 |
| Deferred viewports must be `Send + Sync` | the `Rc` shim and its thread-local registry | ADR 0025 |
| Raising a window on Wayland needs an activation token | `platform::window::raise` asks for attention instead (`crates/egui-pop-platform/src/window.rs:11`) | - |

## Overlaps and conflicts

1. **The root window's place is restored twice.** egui-pop restores it
   from its settings into `NativeOptions::viewport`
   (`crates/egui-pop-api/src/window.rs:163`). With eframe's
   `persistence` feature on (the gallery enables it, `persist_window` is
   on by default), eframe also restores its own saved place and applies
   it over the builder (`native/epi_integration.rs:32` to `:45`), so
   eframe's wins. eframe's copy is better on one point: it clamps the
   position to the real monitors, which egui-pop's cannot (it sees none).
   *Resolved (2026-11-18):* `window::restore` turns `persist_window` off,
   so a run egui-pop starts has one owner, egui-pop, and the runner's
   screens give it the clamping eframe had. A host on eframe's own runner
   chooses: it calls `window::restore` (egui-pop's place), or leaves the
   root to eframe (the gallery, which keeps its documents in eframe's
   storage, does that). A detached window's closed place has one owner
   too: egui-pop's (`closed_windows`, naming its screen); the copy
   egui_stack keeps when it closes a float is dropped at each pass.
2. **Two persistence systems.** egui-pop never implements
   `eframe::App::save` and never reads `Storage`; it keeps its documents
   in its own directory. eframe's store, when the feature is on, keeps
   egui's memory and the root window beside it. Neither is wrong, but an
   application sees two directories and two sources of truth.
   *Decided (2026-11-18): egui-pop keeps its own writer.* eframe calls
   `App::save` only with its `persistence` feature and a storage, every
   `auto_save_interval` (30 s) and at exit (`native/epi_integration.rs:458`
   to `:502`). egui-pop's writer saves a second after a change, with no
   feature, so a crash loses at most that second; driving it from
   `App::save` would lose up to thirty and force eframe's feature on.
   eframe's storage stays the application's choice, for egui's memory.
3. **The present mode is chosen before start-up, for every window.**
   `windows::present_mode` rewrites `wgpu_options` globally; eframe 0.37
   lets the application change it per frame through
   `Frame::set_wgpu_surface_config` (`epi.rs:965`).
   *Decided (2026-11-18): it stays chosen at start-up.* The per-frame
   setter reconfigures every surface at once, so it cannot give one
   window a mode of its own, and going back to a FIFO present when only
   one window is open would bring the Wayland stall back for a hidden or
   minimised main window, which the main window's wake
   (`main_wake::paint_safe`) relies on not having. The per-window mode is
   asked upstream instead (upstream.md, drafted requests).
4. **Window handles taken once, from the creation context**, then kept
   in egui-pop's statics (`frame::set_native_window`,
   `files::set_parent_window`): correct for the root, and the only option
   for the others, which eframe does not expose.
5. **The `Rc` shim** works around the `Send + Sync` bound of deferred
   viewports rather than eframe's model; it holds as long as eframe runs
   deferred callbacks on the main thread, which egui-pop asserts.
6. **Things eframe has that egui-pop replaces on purpose**: keyboard
   zoom (egui-pop owns the interface zoom, upstream.md:583), clipboard
   events (`clipboard_shortcuts` off where a backend reads), the window frame
   (drawn by the kit). These are design choices, not conflicts.

## Options

### A. Keep eframe as the runner, extend it (chosen)

Cost: low; the glue is already thin and runner-agnostic. Risk: the
limits above stay until upstream moves, so their workarounds stay.

A variant inside A, **adopted (2026-11-15)**: `create_native` runs
eframe on an event loop egui-pop owns (`lib.rs:376`, `native/run.rs:540`).
eframe still creates, paints and runs every window, on the desktop and
on the web, so there stays one runner for both targets; owning the loop
reaches several native hooks eframe does not expose. See
[The runner](#the-runner-eframe-through-create_native) below.

### B. An own runner on winit and egui-wgpu (rejected)

It would give every window's handle, owners where winit has them, the
surface format, and the event loop. It would cost the web runner, the
AccessKit, IME, clipboard and frame pacing integration eframe maintains,
a fork to keep in step at every egui release, and the users who run
egui-pop parts in a plain eframe application ([standalone.md](standalone.md)).
Several of the gains are blocked in winit anyway (owners, colour
management). Rejected: it fights the ecosystem rather than extending it.

### C. Contribute the missing pieces upstream

egui's CONTRIBUTING, "Agents & LLM policy": a human is responsible for
the quality of code, issues and pull request descriptions, a human
reviews the code before a pull request leaves draft, descriptions stay
short, and pull requests that put an undue burden on the maintainers
("massive PRs, LLM slop") may be closed. So: focused issues with a
minimal reproduction, small pull requests a maintainer asked for, filed
by the owner. winit's CONTRIBUTING has no AI policy and asks for larger
API proposals as an issue first. Nothing is opened from here; drafts go
in `docs/upstream-drafts/`.

## Recommendation

Keep eframe (A), and organise egui-pop's work around it in three parts.

### 1. Build on eframe's extension points

- **The `App` trait**: keep egui-pop's application an `eframe::App`
  (`logic` for hidden-window ticks, `ui`, `on_exit`). `save` is not
  used: eframe calls it only with its `persistence` feature, every
  30 s, and egui-pop's writer saves a second after a change (overlap 2).
- **`NativeOptions` and `WebOptions`**: keep taking the application's
  options and adding to them (`window::restore`, `present_mode`), never
  building them behind the application's back; document which fields
  egui-pop sets.
- **Viewports**: application windows and detached panels stay egui
  viewports with `ViewportBuilder` and `ViewportCommand`, so they keep
  working on the web and in kittest (embedded as `egui::Window`).
- **Raw window handles**: take them where eframe gives them (the creation
  context, `Frame`), through `raw-window-handle` types, as `files` and
  `frame` already do; for the windows eframe does not expose, from the
  event loop egui-pop owns (the runner), behind the same per-viewport
  functions.
- **Frame pacing**: the present mode stays chosen at start-up
  (`windows::present_mode`); `Frame::set_wgpu_surface_config` changes
  every surface at once and would bring back the Wayland stall (overlap
  3).
- **Platform services** stay separate crates (tray-icon, notifications,
  a single instance) waking egui through `request_repaint`, which is how
  eframe expects them.

### 2. Stop fighting or duplicating eframe

- **The root window's place**: one owner, egui-pop: `window::restore`
  turns `persist_window` off, and the runner's screens clamp the place
  (done, overlap 1).
- **Storage**: document that egui-pop's persistence is the application's
  store and eframe's `persistence` feature is not needed; when an
  application turns it on, egui-pop says so in its `about` information
  rather than silently keeping two.
- **Clipboard**: turn `clipboard_shortcuts` off and bind the chords in
  the keymap, as the upstream.md entry already plans, but only where a
  system backend reads the clipboard (`egui-pop-platform`'s arboard
  backend): with the shortcuts off, `TextEdit` no longer copies or
  pastes on its own, and egui-pop must push the events and read the text
  itself, which the default backend cannot do natively. The event
  mapping stays for the web and the default backend (done:
  `clipboard::set_keymap_shortcuts`, [clipboard.md](clipboard.md)).

### 3. Gaps only eframe (or below) can fill

Each with a workaround in egui-pop shaped like the future fix, so the fix
replaces the inside of an egui-pop function and not its API.

| Gap | Proposal | Layer | Workaround meanwhile, compatible with the fix |
|---|---|---|---|
| A viewport's window handle | expose each viewport's raw handles (on `ViewportInfo`, or a `Context` query on native) | eframe | the runner reads them from winit's window ids and records them through `files::set_parent_window` and `frame::set_native_window`; when eframe gives handles, they replace the ids behind the same functions |
| An owner window | `ViewportBuilder::with_parent(ViewportId)`, mapped to winit's owner where it exists | winit first (issue 4256), then egui-winit | palettes inside the main window; the U3 cell in hosts.md flips when it lands |
| Screens before placement | the monitors (rects and scales) given to the `window_builder` hook or in `ViewportInfo` | eframe | reached by the runner (`runner::screens`), which hands them to `WindowState::repair`; the ask stays for applications on eframe's own runner |
| Surface format and colour space | `WgpuConfiguration` choosing them, the renderer's output transfer from the colour space | egui-wgpu | the display stage reports sRGB with its reason; `gpu::install` reads the format from the render state, so it follows automatically |
| Custom cursors in immediate viewports and on the web | pass the bitmap through | egui-winit, eframe web | a system cursor fallback per bitmap |
| A Windows hit test | a hit-test callback | winit (PR 3207) | the subclass layer in `egui-pop-win32`, removed by a step |
| Activation token for raising | pass a token to `ViewportCommand::Focus` | egui, egui-winit | asking for attention on Wayland |

Drafts already exist for several of them in
`docs/upstream-drafts/`; the others are listed as "not
filed yet" in [upstream.md](upstream.md), which stays the register: a
workaround and its entry go when the fix ships in a release egui-pop
moves to.

## Fallback triggers

egui-pop stays on eframe (option A, through `create_native`) until one of
these holds; then it runs its own winit loop with eframe's pieces
(`create_native` already owns the loop) or, last, option B:

- **A hook only eframe's painter holds becomes a must**: the main
  interface's surface in a wide-gamut or HDR colour space, or a present
  mode per window, with no answer to the drafted egui-wgpu requests
  (upstream.md) within two egui releases of filing them.
- **An owner window egui-pop must set at creation**, on a system where
  setting it after creation does not work, with winit's cross-platform
  owner (rust-windowing/winit#4256) still not released two winit
  releases after it is filed for.
- **eframe stops painting inside `RedrawRequested`**, or winit's
  `WindowId` stops being the native window, and the release that changes
  it gives no per-viewport handle instead: the runner's guards
  (`runner::ids_verified`, `egui-pop-winit`'s `window_id` test) say so
  first.
- **eframe drops `create_native`**, or a hook egui-pop relies on, with no
  replacement.

Each trigger is checked at an egui release bump. Until one holds, the
cost (about 2,900 lines natively and 4,500 on the web, re-synced at each
release) is not worth paying.

## The runner: eframe through `create_native`

Decided 2026-11-15, after a time-boxed spike (one file of about 270
lines, run in wbox on Wayland and X11): egui-pop keeps eframe as its one
runner for native and web, and on the desktop runs it on an event loop of
its own. `AppBuilder::run_native` and `run_native_with` build the winit
event loop, hand eframe to it with `eframe::create_native`, and run it
with a handler (`crates/egui-pop-api/src/runner/native.rs`) that sees
every winit callback before eframe; behind the `create-native` feature,
on by default. Without the feature, on the web, on iOS and Android,
eframe runs as before (`eframe::run_native`, `WebRunner`). kittest never
goes through `run_native`.

What owning the loop reaches:

| Hook | Reached? | How | Workaround it removes |
|---|---|---|---|
| Each viewport's raw window handle | Yes on Windows, X11, Wayland; not on macOS (winit's id is the `NSWindow`, the handle wants its `NSView`) | eframe paints a window synchronously in its `RedrawRequested` (`native/run.rs:420`); the handler notes which, a `Context::on_begin_pass` hook pairs it with `ctx.viewport_id()`. winit 0.30's `WindowId` is the native window (the `HWND`, the X11 id, the `wl_surface` proxy): `egui-pop-winit` turns it into raw handles, the only crate with the `unsafe` | the per-viewport consumers move to it next: the chooser's parent from a secondary window, the Windows hit test, the Wayland image surface on any window |
| Monitors before placement | Yes | the handler reads `available_monitors` in `resumed`, before eframe's own creates the main window; the root's `window_builder` hook, called inside it, repairs the restored place | `WindowState::repair(&[])` gets real screens |
| Owner / parent window | Not at creation (eframe builds the windows). After creation, from the handle: Windows (`GWLP_HWNDPARENT`), X11 (`WM_TRANSIENT_FOR`); not Wayland | not coded | palettes inside the main window, on Windows and X11 only |
| Events before eframe | Yes (filter or add `WindowEvent`s) | not coded | candidate: `frame::compositor_grab` |
| Surface format / colour space | No: egui-wgpu's painter is private to eframe | - | none |
| Present mode per window | No, and unrelated to the loop: `Frame::set_wgpu_surface_config` is per frame but for every surface | - | none beyond realignment step 3 |
| Activation token | Already in eframe 0.37 for the root; the handler sees `ActivationTokenDone` for every window | - | none |

Guards on the assumptions it rests on:

- **`WindowId` is the native window**, an implementation detail of
  winit 0.30, not its API. `egui-pop-winit`'s `window_id` test creates a
  hidden window on each system and compares; and at run time the runner
  compares the main window's id, read as a handle, with the handle
  eframe gives for it: other windows' handles are used only when both
  agree (`runner::ids_verified`).
- **eframe paints inside `RedrawRequested`**, true in 0.37: the first
  pass that begins there is that window's viewport. An immediate viewport
  (a detached panel's window) is painted in a pass nested in its
  parent's, and its own window's `RedrawRequested` runs no pass (eframe
  repaints the parent instead): the two are paired in the order seen, a
  guess a later pass of the window's own corrects. A host running the
  loop itself gets the same through `runner::host` (feature `host-loop`).
- **eframe's own result**: `create_native` keeps the run's error inside
  its handler, so the runner keeps the application creator's error
  itself (`run_native_with`'s `Error::AppCreation`), and reports a run
  that stopped before the application was made as an error too.
- **One event loop per process** (winit): it is kept for a second run, as
  eframe does.

### An own runner for native and web (sketched, not built)

Built on egui-winit, egui-wgpu and egui's web pieces, it would
re-implement what eframe 0.37 keeps in about 2,900 lines natively for
wgpu (`native/run.rs`, `winit_integration.rs`, `wgpu_integration.rs`,
`epi_integration.rs`, without glow) and 4,500 lines on the web (`web/`:
the runner, events, text agent, screen reader, painter, storage),
re-synced at each egui release. On top of what `create_native` reaches,
it would gain the surface format and colour space per window, the
present mode per window, owners set at creation (winit has them on
Windows only), and lose the users who mix egui-pop parts into a plain
eframe application. It stays the fallback if wide gamut in the main egui
surface, or another hook only the painter holds, becomes a must.
