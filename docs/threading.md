# Threading

What runs on which thread in an egui-pop application, what egui allows,
what egui-pop could move off the interface thread, and at what cost.
It began as an analysis that changed no behaviour; the decisions it
asks for are listed at the end, and the one taken since (the viewport's
tile intake) is recorded in section 5.3.

**In short.**

- egui builds each frame on **one thread**: the thread running the
  window's event loop, which is the process's main thread (on macOS it
  must be; on Linux and Windows it may be another one, opted into).
  That thread builds the frame, tessellates it and renders it with
  wgpu, in that order, every frame. No egui API builds parts of one
  frame on several threads.
- egui's `Context` itself is `Send + Sync`: any thread may ask for a
  repaint, upload or replace a texture, or read the context. So work
  can run anywhere; only **drawing it into the frame** happens on the
  interface thread.
- **The application's kernel can already live on another thread**: the
  host contract (ADR 0010) was made for it (results later, from any
  thread; a journal; a waker). What cannot move is egui-pop's own
  interface state (commands, keymap, steps and hooks, zones, the
  workbench): it is built with `Rc` and `RefCell` and is used while the
  frame is drawn. Moving it would cost a frame of latency on every
  interaction and make hooks asynchronous; it is not recommended.
- **Technical displays can be delegated**: a worker renders into a
  texture (on the CPU, or on the GPU with its own wgpu work on the
  shared device), the interface thread only draws the texture. egui-pop
  could offer this as a component ("a delegated surface").
- The measurements below show where the interface thread actually
  loses time under background load today, and the fixes are cheap:
  budgets on what a frame takes in, coalesced wakeups, and no parsing
  under a lock the frame needs.

## 1. egui's constraints

All references are to the crates.io sources of egui, eframe, egui-wgpu,
epaint and egui-winit 0.36.2, winit 0.30.13 and wgpu 30.0.1 (the version
egui-wgpu 0.36.2 depends on).

### 1.1 Which thread builds the frame

eframe 0.36's `App` has `logic(&mut self, ctx, frame)` (a default body)
and `ui(&mut self, ui, frame)` (`eframe/src/epi.rs`). eframe calls both
from winit's `RedrawRequested` handler: `run_ui_and_paint`
(`eframe/src/native/run.rs`) calls `integration.update`
(`native/wgpu_integration.rs`, with the comment "make sure we hold no
locks here!"), which runs `egui_ctx.run_ui(raw_input, |ui| { app.logic(..);
app.ui(ui, ..) })` (`native/epi_integration.rs`). The glow backend
does the same. All of it runs inside `event_loop.run_app`: **on the
thread that created the winit event loop.** eframe's own state there is
`Rc<RefCell<..>>`: single-threaded by design.

Which thread may create the event loop is winit's rule
(`winit/src/event_loop.rs`): "the `EventLoop` must be created on the
main thread, and only once per application". On Linux and Windows,
creating it elsewhere panics unless `with_any_thread(true)` is set
(`platform/x11.rs`, `platform/wayland.rs`, `platform/windows.rs`); on
**macOS there is no way around it** (`platform_impl/macos/event_loop.rs`:
"on macOS, `EventLoop` must be created on the main thread!").

eframe has no option for it, but `NativeOptions::event_loop_builder`
(a hook given the `EventLoopBuilder`) lets an application call
`with_any_thread(true)`: the interface then runs on a thread of the
application's choosing on X11, Wayland and Windows. That moves the
interface thread; it does not give the interface two threads.

**In plain words:** one thread draws the window, and on a Mac it is
always the process's first thread. "The interface thread" below is
that thread.

### 1.2 Is `Context` shareable, and what locks it takes

`pub struct Context(Arc<RwLock<ContextImpl>>)` (`egui/src/context.rs`),
a `parking_lot` lock behind epaint's wrapper; a test asserts
`Context: Send + Sync`. The lock is **not held for the whole frame**:
`run_ui` takes it briefly to begin the pass, runs the application's
code unlocked, and takes it again to end the pass. Every widget call
takes it for a moment. Another thread can therefore use the context
while a frame is being built, between those moments.

What a worker may do with a cloned `Context`:

- `request_repaint()` / `request_repaint_after(d)`: documented for
  this: "If called from outside the UI thread, the UI thread will wake
  up and run" (eframe installs the callback). It targets the viewport
  on top of the context's viewport stack; a worker should prefer
  `request_repaint_of(ViewportId::ROOT)` when several windows exist;
- `load_texture`, `TextureHandle::set`, `tex_manager()`: the texture
  manager has its own lock (`Arc<RwLock<TextureManager>>`); the change
  is picked up at the next end of pass (`take_delta`). It does not wake
  the interface: call `request_repaint` too;
- read input or memory: possible, but it reads whatever state the
  frame in progress left, and `input()` and `memory_mut()` take the
  **write** lock.

What a worker must not do: call the context from inside a closure
given to the context (`ctx.input(|i| ctx.request_repaint())`): the
lock is not reentrant ("you may NOT recursively lock the same Context,
as that can lead to a deadlock"). Note also that `tessellate` holds the
write lock for the whole tessellation, so a worker calling the context
then waits that long.

### 1.3 Viewports (several windows)

- **Immediate** viewports (`show_viewport_immediate`): the callback is
  called at once, "This may only be called on the main thread"; the
  renderer for them is a thread-local eframe installs.
- **Deferred** viewports (`show_viewport_deferred`): the callback must
  be `Fn(&mut Ui, ViewportClass) + Send + Sync + 'static` and is
  "called by the integration at a later time, perhaps multiple times".
  In eframe it is called **on the same event-loop thread**, when that
  window needs a repaint. `Send + Sync` is required of the callback's
  type; no other thread runs it.

Neither kind draws in parallel. A second window costs the interface
thread its own frame build, tessellation and render.

### 1.4 Textures from other threads

eframe gathers the frame's `TexturesDelta` and hands it to egui-wgpu's
`Painter::paint_and_update_textures`, which takes
`renderer.write()` and calls `Renderer::update_texture` for each
change (`egui-wgpu/src/winit.rs`): **the GPU upload happens on the
interface thread, during the render**, for every texture a worker set.

To avoid that copy, a texture can be made outside egui:
`RenderState { device, queue, renderer: Arc<RwLock<Renderer>>, .. }` is
`Clone`, reachable from `CreationContext::wgpu_render_state` or
`Frame::wgpu_render_state()`. wgpu's `Device` and `Queue` are
`Send + Sync` on native (asserted in `wgpu/src/api/device.rs` and
`queue.rs`), so **a worker can create textures, write them and submit
its own command buffers**. To show one in egui, register its view once
with `Renderer::register_native_texture` (an `Rgba8Unorm` view) or swap
it with `update_egui_texture_from_wgpu_texture`; both need
`renderer.write()`, the lock the render holds, so a worker does it
rarely (registering a few textures once and then writing into them is
the pattern).

### 1.5 Drawing off the interface thread

- **Paint callbacks** (`egui_wgpu::CallbackTrait: Send + Sync`):
  `prepare` and `finish_prepare` run inside `Renderer::update_buffers`
  under `renderer.write()`, and `paint` inside `Renderer::render` under
  `renderer.read()`, both **on the interface thread during the
  render**; their command buffers go in egui's submit. A callback is
  GPU work recorded on the interface thread, not work done elsewhere.
  Shared resources live in `callback_resources`, a concurrent type map.
- **Rendering to a texture on a worker** is allowed: a worker with the
  shared `Device` and `Queue` records and submits its own passes into a
  texture it owns (two textures, or a ring, so the one being shown is
  never the one being written), then tells the interface which one is
  ready; the interface draws it as an image or samples it in a paint
  callback. wgpu orders submissions on one queue, so the texture is
  complete when the interface's frame samples it, if the worker
  submitted before the frame's submit. This is how a technical
  viewport can be "delegated".
- **On the CPU**, a worker fills pixels and calls
  `TextureHandle::set`; the upload still happens on the interface
  thread at the next render (1.4).

### 1.6 Where egui-wgpu renders

Synchronously, on the interface thread, after the frame is built:
`run_ui_and_paint` builds the frame, then tessellates it
(`Context::tessellate`), then calls `paint_and_update_textures`, which
uploads textures, prepares callbacks and buffers, renders, submits and
presents. With the default surface configuration (`AutoVsync`, at most
one frame in flight) the thread can **block on the display** in
`get_current_texture` and `present` (egui-wgpu times them as "vsync").
Tessellation runs in parallel only with epaint's `rayon` feature (off
in egui's and eframe's defaults) and only splits large shapes.

### 1.7 Repaint plumbing

eframe sets `set_request_repaint_callback` to send a `RequestRepaint`
user event through winit's `EventLoopProxy` (behind a `Mutex`); the
loop drops requests made obsolete by a frame that already ran, and
throttles invisible windows to 100 ms. A zero-delay request asks for
two passes. The callback is called **while the context's write lock
is held**. `Context::repaint_causes()` says why the last pass asked for
another ("helpful in debugging why egui is constantly repainting");
each `request_repaint` records its caller's file and line.

egui_kittest runs `run_ui` on the test's own thread, with no event
loop: everything egui-pop tests headless runs on one thread.

### 1.8 What this means

| Wish | With egui |
|---|---|
| Build the frame on two threads | No. One thread builds, tessellates and renders. |
| Move the interface off the main thread | Linux and Windows only (`event_loop_builder` + `with_any_thread`); never on macOS. |
| Compute anything on a worker | Yes: the context and wgpu's device and queue are shareable. |
| Upload pixels from a worker | Set on a worker, copied to the GPU on the interface thread; or written by the worker into its own wgpu texture, no copy on the interface thread. |
| Render a technical view on a worker | Yes, into a texture (CPU or wgpu), presented by the interface thread. |
| A second window drawn in parallel | No: deferred viewports run on the same thread. |

## 2. egui-pop's threads today

### 2.1 On the interface thread

Each frame, in this order:

1. `App::logic`: attach the context, install the theme if asked,
   **settle** (poll every command result in flight), feed the strips'
   indicators from the journal, the pass lifecycle, process the
   requests (commands queued by blocks and handlers, enabling plugins,
   rebinding keys, workbench changes). eframe calls it through
   `eframe::App::logic`, then `App::show` calls it **a second time**.
2. `App::show`: key dispatch (commands run synchronously through the
   host), the shell (title, menu bar, status bar, the workbench and
   its panels), auxiliary windows, overlays, popups, live regions,
   the requests again.
3. Inside the panels, the components: the viewport takes in the tiles
   that arrived, already converted by the source's workers (the default
   since section 5.3), and hands them to egui within an upload budget;
   the terminal locks its model and paints the
   grid; the editor locks its model, highlights up to 500 lines a pass
   and lays out the visible lines; code assist calls the provider; the
   tree applies the edits its source sent; the platform plugins drain
   their queues.
4. eframe tessellates and renders (1.6). On the GPU path, the
   viewport's paint callback creates a texture for each new tile and
   copies its half-float texels into it, in `prepare`, on this thread
   (section 5.2 measures it).

Steps and hooks (ADR 0011), command handlers of `LocalHost`, store
listeners and writes, persistence of documents, and the default code
assist provider (`WordCompletion`, which scans up to 5,000 lines around
the cursor) all run here, synchronously.

### 2.2 Threads egui-pop starts

| Thread | Crate | Lives | Returns results through |
|---|---|---|---|
| `egui-pop-file-chooser` | api | one per system file dialog | an `Arc<Mutex<Option<Answer>>>` polled each frame, then a repaint |
| `egui-pop terminal input` | editors (`pty`) | a PTY session | an unbounded channel from the interface |
| `egui-pop terminal output` | editors (`pty`) | a PTY session | **feeds the emulator itself**, under the terminal's model lock, then a repaint per read (16 KiB at most) |
| `egui-pop terminal wait` | editors (`pty`) | until the child exits | the model (`pending_exit`) |
| `egui-pop-viewport-demo-N` | viewport (demo source) | the source | the tile sink (a channel), a repaint per tile |
| `egui-pop instance` | platform | the process | an `Arc<Mutex>` inbox, a repaint per launch |
| `egui-pop global shortcuts` | platform (portal) | the plugin | a channel, a repaint per press |
| `egui-pop notifications` | platform | the plugin | a channel, a repaint per event |
| `egui-pop notification` | platform | one per shown actionable notification (no limit) | the same channel |
| `egui-pop focus mode` | platform | one read at a time | an `Arc<Mutex>`, a repaint |

Libraries add their own (global-hotkey's handler thread, ksni's D-Bus,
rfd's portal). Applications add theirs behind the traits below: a tile
source's workers, a tree source's loader, a terminal source, an assist
provider's thread, the host's kernel.

All of them post the same way: a queue (a channel or a vector behind a
mutex) and `request_repaint()`. The interface drains the queue at its
next pass. None is drained with a budget, except the editor's
highlighting (500 lines a pass).

A component its host stops drawing (a tab behind another, a palette
hidden or rolled up) keeps receiving. Each of egui-pop's keeps bounded
memory meanwhile, by draining off-draw or by a cap whose drops are
counted where agents see them:

| Queue | While not drawn |
|---|---|
| Terminal output (a PTY, a `Console`) | Off-draw: the delivering thread feeds the emulator, whose scrollback is bounded. |
| A `Console` no terminal started yet | Capped: the latest `CONSOLE_PENDING` bytes (1 MiB) from a line's start; `Console::dropped`, and `dropped` in the start's result. |
| Jobs (`job.*` events) | Off-draw: folded at each pass from the bounded journal; each job's log keeps its last lines. |
| Image viewport (`TileSink`) | Tiles and failures answer requests, made only when drawn. Changes and readouts are capped at `source::NOTICES` (256): a change past it becomes one change of the whole image, a readout is dropped; `coalesced` in the viewport's stats. |
| Tree (`TreeHandle`, a source's answers) | Capped at `tree::EDITS_WAITING` (4096): past it, the edits waiting are dropped and every branch loads again at the next pass; `TreeState::dropped`, and a `tree.overflowed` event. |
| Library commands | Capped at `library::plugin::INBOX` (64), the oldest dropped; `dropped` in the `libraries` collector. |
| Code assist | One answer waiting per kind; a new request cancels the old. |
| Platform plugins (notifications, shortcuts, tray, instance, focus mode) | Off-draw: drained at each pass. |

Thumbnails and the library's images are the application's: egui-pop
keeps no queue of them. A component an application declares with
workers drained when drawn (`Mountable::workers`) gets condition c11
in `docs/hosts.md`; `Mountable::bounded_workers` says it keeps bounded
memory hidden.

### 2.3 What is shared

- The **value store**, `SharedStore = Arc<Mutex<Store>>`: bound widgets
  lock it every frame to read, each change locks it again; listeners
  run while it is locked, on whatever thread wrote.
- Command results: `Pending`/`Resolver`, an `Arc<Mutex<Option<..>>>`
  slot, polled by `settle`; the `Waker` (`Arc<dyn Fn() + Send + Sync>`)
  calls `request_repaint`.
- Services (`Arc<dyn Any + Send + Sync>`) and the plugins' states
  (toasts, dialogs, files, devtools: `Arc<Mutex<State>>`).
- The terminal's model (`Arc<Mutex<Model>>`): the source's thread
  parses into it, the interface paints from it.
- The editor's model (`Arc<Mutex<Model>>`): the interface highlights
  and paints under it; an assist provider's thread reading the text
  through `Request::state` locks it too.
- The viewport's tile channel, its GPU upload queue (`Arc<Mutex>`), its
  cancel tokens (atomics).

Everything else of the application layer is **not shareable**:
`AppHandle` is `Rc<Shared>` with about thirty `RefCell` fields;
command handlers, step hooks, list providers and reducers are `Rc<dyn
Fn>`; blocks are `Box<dyn FnMut>` without `Send`; the `Host` trait has
no `Send` bound and lives in a `RefCell`.

### 2.4 The `Send` and `Sync` contract

| What the application implements | Bound | Called on |
|---|---|---|
| `Host` | none | the interface thread (`dispatch` must return at once) |
| command handler (`Ctx::command`) | none (`Rc`) | the interface thread |
| step hooks (`before`, `replace`, `after`) | none (`Rc`) | the interface thread |
| store listener | `FnMut + Send` | the writer's thread, store locked |
| `TileSource` | none | the interface thread (`request` must return at once) |
| `DisplayTransform` | `Send + Sync` | the interface thread (CPU path) |
| `TreeSource` | none | the interface thread (`load` must return at once) |
| `TerminalSource` | `Send` | the interface thread (`start`, `write`); output from any thread |
| `AssistProvider` | `Send` | the interface thread (every request must return at once) |
| `Highlighter` | `Send` | the interface thread, editor locked |
| notification `Backend` | `Send + Sync` | its own thread (may block) |
| shortcuts and tray `Backend` | none | the interface thread |

What the application gets to answer from any thread: `Resolver`,
`Waker`, `TileSink`, the tree's `Sink`, the terminal's `Sink` and
`Console`, the assist `Reply`, `Notifier`, `Launches`.

The rule the traits follow today, "the interface asks, never waits; the
answer comes from anywhere", is the right one. It is written in each
trait's documentation, not checked: a source that computes inside
`request`, or a provider that answers inside `complete`, stalls the
frame (measured below).

## 3. The kernel on another thread

### 3.1 What it would mean

"The kernel" here is two different things:

1. **The application's kernel**: its documents, engine, commands,
   undo history, its bus. ADR 0010 already puts it behind `Host`.
2. **egui-pop's interface kernel**: the registries (commands, keymap,
   menus, panels, zones), the `when` context, steps and hooks, the
   workbench, the requests queue, the journal fallback, the value
   store.

(1) can run on its own thread today. `dispatch` returns
`Reply::Later(pending)` and the kernel resolves it from its thread;
the kernel publishes on its own bus and egui-pop reads it through
`since`; the kernel wakes the interface with the `Waker`. What is
missing is convenience, not possibility: `Journal` is not thread-safe
(a kernel keeps it behind its own lock, as the `threading` example's
`LockedHost` does), and `Host::store()` hands out an
`Arc<Mutex<Store>>` the interface locks every frame, so a kernel
writing the store from its thread contends with the frame (briefly).

(2) cannot move as it is, and should not.

### 3.2 What assumes the same thread

- **`Rc` and `RefCell` everywhere** in `egui-pop-api`: `AppHandle`,
  every handler and hook type, every block. Making them `Send` means
  `Arc<Mutex>` for each and `Send` bounds on every closure an
  application writes, a breaking change to the whole API.
- **Synchronous steps feeding the frame**: `AppHandle::step` returns an
  `Outcome` the caller uses at once (the workbench applies a dock
  change, the palette drag reads the result in the same frame, the
  file chooser resolves). A hook may veto or redirect, and the drawing
  code acts on that in the same frame.
- **`&mut` during the frame**: the keymap is borrowed mutably inside
  the rebind step's default; the workbench is borrowed mutably while
  areas are drawn (changes made while drawing are deferred, which
  already shows how much the frame depends on owning it).
- **Commands from keys run in the frame that saw the key**, and their
  `when` context is computed from the focus of that frame.
- **The host is taken out of its cell while it dispatches**; reads made
  meanwhile (`can_undo`, `events_since`) return defaults.

### 3.3 What a split would cost

- **Latency.** Input is read by the interface thread. If the kernel
  that decides what a key does runs elsewhere, the key's effect appears
  one frame later at best (the kernel answers before the next frame),
  two in practice (intent sent during frame N, applied by the kernel,
  snapshot visible at frame N+1 or N+2). At 60 Hz that is 17 to 33 ms
  added to every interaction: noticeable on drags and typing, where
  egui's immediate mode is otherwise zero-latency.
- **Consistency.** The frame draws a snapshot; the user acts on what
  was drawn. Two intents built from the same stale snapshot can
  conflict (two drags, a menu item greyed by a `when` that was true a
  frame ago). Every intent needs a version or a precondition, and a
  refusal path.
- **Hooks become asynchronous.** A before-hook that vetoes `frame.close`
  must answer before the window decides to close; with the kernel
  elsewhere the close waits a round trip, or the veto arrives after the
  fact. ADR 0011's promise ("observe, veto, replace, extend at the
  moment it happens") would weaken to "at the next frame".
- **Undo.** Undo is already the host's (`Host::history`): it moves with the
  application's kernel without change. The store's undo groups gestures
  by drag start and end on the interface thread; across threads a
  gesture's begin and end must travel with their writes, in order.
- **Gain.** The interface kernel's own work per frame is small (see
  the measurements: `pop.app.logic` and `pop.app.requests`); the heavy
  work is in components and in the application's kernel, which can
  already move.

### 3.4 Two designs

**A. Message-passing kernel.** The interface thread keeps egui and a
view model; the kernel thread owns the state. The interface sends
intents (a command and its arguments, a gesture's begin, changes and
end, a key that resolved to a command) over a channel; the kernel
applies them in order, publishes events, and posts a new view model
(or diffs) back, then wakes the interface.

```text
 interface thread                       kernel thread
 ----------------                       -------------
 input -> keymap -> intent  --chan-->   apply, validate, journal
 draw(view model vN)        <--chan--   view model vN+1 (or diffs)
 settle: take latest vN+1   <--wake--
```

- Pros: the kernel never waits on the frame, the frame never waits on
  the kernel; the journal is the log of intents and results; testable
  without egui; matches a kernel in another process.
- Cons: one to two frames of latency per interaction; every piece of
  interface state the kernel decides must be in the view model; steps
  and hooks either stay on the interface thread (then they are not the
  kernel's) or become asynchronous.

**B. Double-buffered state.** The kernel writes into a back buffer;
at the start of each frame the interface swaps (or the kernel
publishes an `Arc` of an immutable snapshot, swapped atomically, as
`arc-swap` does). The interface reads the front snapshot without locks
and writes nothing to it; its changes go back as intents, as in A.

- Pros: reads in the frame are lock-free and consistent (one version
  per frame); cheap when the state is a persistent structure (clone an
  `Arc` per frame).
- Cons: same latency as A for writes; a big state must be persistent
  (structural sharing) or the copy dominates; the interface sees a
  snapshot up to a frame old.

Both are a **kernel pattern for the application**, and egui-pop's part
is small: a thread-safe journal, a `Host` adapter over a channel, and
a snapshot service the blocks read. Neither requires moving egui-pop's
interface kernel.

### 3.5 Assessment

- **The application's kernel on its own thread: yes, today**, through
  `Host` with `Reply::Later`, its own journal behind a lock, and the
  waker. Cost: commands it runs complete a frame later (they already
  show as running), and its state is read through snapshots or the
  store. egui-pop can make this the paved road (follow-ups 5 and 6).
- **egui-pop's interface kernel on its own thread: possible only by
  rewriting `egui-pop-api` around `Send` types and asynchronous steps**,
  for a gain the measurements do not show: the interface kernel's own
  per-frame work is a small part of a frame. Not recommended; the
  owner decides.

## 4. Delegated display

A technical view (the image viewport, a node graph, histograms,
waveforms, scopes) is drawn by a worker into a texture and only shown
by the interface thread. With egui and wgpu:

- **CPU rendering on a worker** (histograms, scopes, a small graph):
  the worker renders pixels, calls `TextureHandle::set` (or sends the
  pixels), asks for a repaint; the interface draws the texture as an
  image. Cost on the interface thread: the upload during the render
  (1.4), proportional to the pixels changed. This is the viewport's
  CPU path today, except that its display transform runs on the
  interface thread at tile arrival.
- **GPU rendering on a worker**: the worker holds clones of `Device` and
  `Queue`, owns two or three textures it renders into (a ring), and
  registers each once with egui-wgpu (`register_native_texture`, under
  the renderer's write lock, once). Per frame it submits its passes and
  posts "texture k is ready"; the interface draws texture k. Nothing
  heavy runs on the interface thread. The worker must not take the
  renderer's lock per frame, and must keep a texture alive while the
  interface may still sample it (the ring's size covers the frames in
  flight).
- **GPU work recorded on the interface thread** (paint callbacks): what
  the viewport does today. Right for cheap draws (one quad per tile),
  wrong for heavy passes, which belong on a worker's submission.

What egui-pop would provide, as a component of the viewport crate:

- a `Surface` (a delegated display): a ring of textures registered
  with egui-wgpu, a `SurfaceSink` a worker renders into (`begin` gives
  a free texture and the device and queue, `present` marks it ready
  and wakes the interface), the widget that draws the latest ready
  texture with the viewport's mapping, and a CPU fallback (pixels set
  through a texture handle) for kittest and the web;
- the viewport's own tile path moved to it: f32 to f16 conversion and
  texture creation on the tile source's side (the sink converts on the
  worker's thread), the interface only binding ready textures;
- a frame budget for uploads (tiles per frame, bytes per frame), with
  the rest carried to the next frame and a repaint asked.

## 5. Measurements

The `threading` example of the gallery
(`crates/egui-pop-gallery/examples/threading.rs`, feature `trace`)
builds frames headless on its main thread at 60 Hz, as eframe would,
with no GPU (it measures the interface thread up to tessellation, not
the render), while worker threads load it. A minimal `tracing`
subscriber records egui-pop's profiling scopes per thread. It runs on
the bench box as a timing job (the machine alone, its frequency capped).

First run (2026-10-13, comparable conditions, 360 frames per scenario,
budget 16.7 ms). Frame build time on the interface thread, in ms:

| scenario | p50 | p95 | p99 | max | frames over budget | textures per frame | wakeups/s |
|---|---|---|---|---|---|---|---|
| idle | 1.02 | 1.03 | 1.05 | 1.26 | 0 | 0 | 0 |
| terminal, steady | 0.62 | 0.64 | 0.93 | 1.43 | 0 | 0 | 33 |
| terminal, flood | 3.61 | 6.12 | 7.12 | 8.07 | 0 | 0 | 60 |
| assist on a thread | 0.32 | 8.01 | 8.16 | 8.34 | 0 | 1 | 23 |
| assist on the interface thread | 0.31 | 38.33 | 38.50 | 38.58 | 60 | 1 | 23 |
| tile storm | 0.32 | 21.56 | 69.02 | 102.22 | 24 | 19 | 53 |
| jobs from the host | 0.29 | 0.42 | 0.49 | 0.84 | 0 | 1 | 60 |
| all three | 0.77 | 23.49 | 36.68 | 60.59 | 24 | 9 | 60 |
| idle, after | 0.54 | 0.55 | 0.59 | 0.95 | 0 | 0 | 0 |

What the scopes show:
- **Tiles are the real problem.** `pop.tiles.receive` takes up to 102 ms
  in one frame: every arrived tile is taken in, converted and uploaded
  on the interface thread, up to 19 textures (4.7 MB) in a frame. An
  upload budget per frame and the conversion on the worker side are
  the first fix (follow-ups 3 and 9).
- **The terminal flood costs the paint lock**, not the paint:
  `pop.terminal.lock` waits up to 7 ms while the reader parses under
  it. Parsing outside the lock removes it (follow-up 2).
- **Highlighting** spends up to 8 ms of a frame in its line budget
  (`pop.editor.highlight`); the budget holds, but a smaller one, or the
  highlighter on a worker, would halve the tail.
- **An assist provider answering on the interface thread** blocks it
  (38 ms frames): the documented rule ("never answer on the interface
  thread") matters, and a debug check would catch it (follow-up 7).
- **The jobs journal is cheap** on the interface thread (0.5 ms p99);
  its lock is contended on the worker side only.
- Wakeups reach 60 per second under load (one per frame), so coalescing
  them (follow-up 1) matters for power, not for frame time.

### 5.1 The tile intake off the interface thread (proof of concept)

Follow-up 3, as two settings of `ViewportState`, opt-in when measured
here and the default since (section 5.3):

- `set_prepare_on_worker(true)`: the tile sink converts each tile on
  the thread that delivers it, the source's worker (the display
  transform on the CPU path, the f32 to f16 texels on the GPU path).
  The interface thread only hands a ready image to egui (or ready
  texels to the paint callback). A tile converted with a display
  transform replaced meanwhile is shown, marked stale and asked for
  again.
- `set_upload_budget(UploadBudget::bytes(n))` (or `::tiles(n)`): a frame
  uploads at most that much (at least one tile). Tiles wait in a queue
  ordered as GIMP orders its work: the visible area first (the proxy,
  then the view's scale, then other scales), then the rest nearest the
  view's centre. When the view pans or zooms the order changes, and
  nothing waiting is dropped (a test, "leftover tiles survive a pan",
  holds it); only a changed region drops the waiting tiles it covers.

The `threading` example runs the tile storm in four variants (the
default, conversion on the workers, a 1 MiB budget, both), views held
for 30 frames so they can fill, and the combined load with both. It
also measures the cost side: the time from a view change to the first
full-resolution tile uploaded and drawn for it, and to every wanted
tile on screen (a view moved on before that is not counted; with a
0.1 s settle before exact tiles are asked for, 100 ms of each latency
is the settle).

Second run (2026-10-13, comparable conditions, the same job and
machine, all rows from one run). Frame build time on the interface
thread, in ms; uploads are textures handed to egui in one frame:

| scenario | p50 | p95 | p99 | max | over budget | uploads/frame (max) | MB/frame (max) | first full-res p50 / max | views with one | all tiles p50 / max | tiles served |
|---|---|---|---|---|---|---|---|---|---|---|---|
| tiles (documented baseline, first run) | 0.32 | 21.56 | 69.02 | 102.22 | 24 | 19 | 4.7 | | | | |
| tiles (default) | 0.32 | 20.17 | 41.52 | 61.69 | 23 | 10 | 2.6 | 141 / 159 | 22 / 49 | 125 / 185 | 224 |
| tiles, budget 1 MiB | 0.32 | 22.31 | 24.82 | 25.86 | 27 | 5 | 1.0 | 140 / 157 | 22 / 49 | 125 / 156 | 226 |
| tiles, conversion on workers | 0.32 | 0.44 | 0.48 | 0.51 | 0 | 4 | 0.8 | 119 / 119 | 23 / 49 | 102 / 120 | 163 |
| tiles, both | 0.32 | 0.42 | 0.47 | 0.58 | 0 | 4 | 1.0 | 119 / 119 | 23 / 49 | 102 / 119 | 162 |
| views held (default) | 0.32 | 11.72 | 28.49 | 50.75 | 9 | 9 | 2.1 | 146 / 151 | 10 / 13 | 165 / 300 | 104 |
| views held, both | 0.31 | 0.39 | 0.44 | 0.48 | 0 | 4 | 1.0 | 135 / 136 | 10 / 13 | 170 / 337 | 104 |
| all (default) | 0.80 | 17.35 | 25.91 | 31.83 | 20 | 5 | 1.3 | 141 / 157 | 20 / 49 | 124 / 157 | 157 |
| all, both | 0.79 | 1.04 | 1.12 | 1.42 | 0 | 3 | 0.5 | 118 / 118 | 22 / 49 | 101 / 118 | 125 |

The default storm measured lower this run than the first (p99 42
against 69 ms): the same code, another run; compare rows of one run.

What the scopes show: `pop.tiles.receive` on the interface thread goes
from 61 ms at most to 0.013 ms; the work moves to the workers
(`pop.tiles.prepare`, 6.5 ms per tile at most), where it lengthens each
tile's computation.

**Verdict.**

- **Keep the conversion on the workers.** It removes the tile storm
  from the interface thread entirely (p99 42 to 0.5 ms, no frame over
  budget, the combined load from 26 to 1.1 ms p99), and the first
  full-resolution tile shows sooner (141 to 119 ms after a view
  change, 146 to 135 ms with views held), since the interface no
  longer stalls the frame that draws it. Its cost is on the workers:
  in the storm they serve about a quarter fewer tiles (224 to 163)
  and more requests are cancelled before being served, so fewer views
  fill completely before the next move (21 to 9 of 49); with views
  held, a view fills slightly later (170 against 165 ms at the
  median). A source with spare workers does not see it. It should
  become the default once a source can opt out (one that already
  delivers display pixels from a GPU has nothing to convert).
- **The upload budget alone is not enough**: it caps the spike (max
  102 or 62 ms to 26 ms) but each tile still costs its conversion on
  the interface thread, so frames stay over budget. With the
  conversion on the workers, it no longer limits anything in this
  harness (the workers deliver at most four tiles a frame here): the
  headless harness has no GPU, so the cost it bounds, the copy to the
  GPU at the render, is not measured. Keep it, off by default, as the
  guard for fast sources (a cache, a GPU pipeline) and slow uploads;
  its price is latency to fill a view (a 1 MiB budget is about 32
  frames for a 4K view in 8-bit RGBA), which the visible-first order
  keeps away from what the user looks at.
- **A worker rendering into its own wgpu texture** (follow-up 9) could
  not be measured by this headless harness; section 5.2 measures it on
  a GPU.

### 5.2 On a real GPU

The viewport crate's `gpu_tiles` example
(`crates/egui-pop-viewport/examples/gpu_tiles.rs`) measures what the
headless harness cannot: the render. It builds frames at 60 Hz as
eframe would, tessellates them, renders them with egui-wgpu into an
offscreen target, submits, then waits for the GPU (as a present at most
one frame ahead does). The view jumps every 8 frames (a storm) or every
30 (views held), over zooms 25 % to 200 % of a 4096 × 3072 image whose
tiles come from two worker threads. Each intake runs in its own process:

- `gpu-ui`: the GPU path, as the old default: scene floats converted to
  half floats and copied into a new texture per tile, in the paint
  callback's `prepare`, on the interface thread, all in one frame;
- `gpu-worker`: the GPU path, the new default: half floats made on the
  source's workers, the texture created and filled in `prepare`, within
  the upload budget;
- `gpu-render`: the source's workers render each tile on the GPU, into a
  texture of their own, with the demo image's function ported to WGSL,
  and hand it over (`TileSink::deliver_device`, see
  [device tiles](viewport.md#device-tiles)): no pixel
  crosses the CPU, the interface thread only binds the texture;
- `cpu-ui`, `cpu-worker`: the same without the GPU path installed (the
  display transform on the CPU, egui textures), old and new default.

"Interface thread" is the frame's build, tessellation, and the render's
recording and submit (texture uploads included), not the wait for the
GPU, which stays under 2.1 ms at p99 everywhere. Latencies are from the
view change; 100 ms of each is the settle before exact tiles are asked
for. Memory: the tile textures the viewport holds at most, and the
process's peak resident memory.

First run (2026-10-13, the box's timing job `egui-pop-gpu-tiles` on an
NVIDIA GeForce RTX 3070 through Vulkan, CPU frequency capped at 65 %,
conditions comparable; all rows from one run):

| intake | interface p50 / p95 / p99 / max (ms) | over 16.7 ms | uploads/frame max | first full-res p50 / max (ms) | views complete | tile textures max (MB) | peak RSS (MB) |
|---|---|---|---|---|---|---|---|
| storm, `gpu-ui` (old default) | 0.38 / 14.28 / 14.92 / 18.12 | 4 | 5 (2.6 MB) | 133 / 136 | 18 of 45 | 91 | 199 |
| storm, `gpu-worker` (default) | 0.38 / 11.93 / 12.06 / 12.65 | 0 | 4 (2.1 MB) | 131 / 133 | 18 of 45 | 90 | 167 |
| storm, `gpu-render` | 0.40 / 0.57 / 0.65 / 1.69 | 0 | 24 (no copy) | 120 / 121 | 45 of 45 | 115 | 208 |
| storm, `cpu-ui` (old default) | 0.36 / 26.37 / 49.49 / 76.86 | 26 | 10 (2.6 MB) | 147 / 156 | 25 of 45 | 49 | 179 |
| storm, `cpu-worker` (default) | 0.37 / 3.33 / 3.45 / 6.05 | 0 | 4 (1.0 MB) | 122 / 123 | 14 of 45 | 34 | 168 |
| held, `gpu-ui` | 0.38 / 7.49 / 14.41 / 15.17 | 0 | 4 | 135 / 144 | 12 of 12, p50 157 ms | 63 | 173 |
| held, `gpu-worker` | 0.38 / 6.15 / 12.15 / 14.95 | 0 | 5 | 132 / 143 | 12 of 12, p50 158 ms | 63 | 170 |
| held, `gpu-render` | 0.38 / 0.47 / 0.68 / 1.81 | 0 | 24 (no copy) | 120 / 121 | 12 of 12, p50 120 ms | 63 | 172 |
| held, `cpu-ui` | 0.37 / 14.20 / 44.27 / 76.60 | 10 | 10 | 149 / 152 | 12 of 12, p50 161 ms | 32 | 179 |
| held, `cpu-worker` | 0.38 / 3.20 / 6.09 / 7.10 | 0 | 4 | 123 / 139 | 12 of 12, p50 172 ms | 32 | 168 |

wgpu's allocator report gave the same 201 MB reserved for every run (a
block reservation, not a measure of use); the tile textures held are the
better measure of GPU memory.

What the rows show:

- **On the CPU path, the conversion on the workers is the whole gain**:
  p99 49.5 to 3.5 ms in the storm, 26 frames over budget to none, the
  first full-resolution tile 25 ms sooner (147 to 122 ms). What is left
  on the interface thread is egui's texture copy at the render (3 ms at
  p95 for 1 MB a frame).
- **On the GPU path, the copy is the cost, not the conversion**: moving
  the f32 to f16 conversion to the workers saves 2 to 3 ms (p99 14.9 to
  12.1 ms) and the frames over budget, but the interface thread still
  spends up to 12 ms a frame in `prepare`, creating one texture per
  tile and copying 2 MB of half floats into it. The upload budget does
  not bite here (8 MiB; the workers deliver at most 2.6 MB a frame).
- **Tiles rendered on the GPU by the workers take the tiles off the
  interface thread entirely**: p99 0.65 ms, flat whatever the load, the
  first full-resolution tile sooner (120 ms, that is 20 ms after the
  settle), and every view of the storm filled (45 of 45, against 14 to
  25): the GPU computes a tile far faster than two CPU workers. It costs
  more texture memory in the storm (115 against 90 MB: more tiles
  arrive, and the budget does not hold them back), and it needs a
  source whose pixels are on the GPU already.

### 5.3 Decision (2026-10-13)

- **The default is the conversion on the source's workers, with the
  upload budget** (`ViewportState::new`: `prepare_on_worker` on,
  `UploadBudget::DEFAULT`, 8 MiB a frame). It removes every frame over
  budget in both paths and shows the first full-resolution tile sooner
  (2 to 25 ms), for the same memory or less. Its cost is on the
  workers (section 5.1: fewer tiles served in a storm). The opt-out
  stays: `set_prepare_on_worker(false)`, `UploadBudget::UNLIMITED`.
- **A source whose pixels are on the GPU hands over textures**
  (`TileSink::deliver_device`): no copy, the interface thread only
  binds, the best of the three by an order of magnitude. This is the
  path for an application whose pipeline renders on the GPU (Helios's
  processing): its output should reach the viewport as textures on
  the shared device, not as floats read back and uploaded again. The
  prototype stays experimental until such a source uses it; the
  delegated surface of section 4 generalises it.
- **Next, for CPU sources on the GPU path**: the 12 ms left in
  `prepare` is the texture creation and the copy. The workers can do
  both (wgpu's device and queue are shareable): the sink, given the
  device, creates and fills the texture on the delivering thread and
  hands it over as `deliver_device` does. A pool of tile textures
  reused instead of created would cut it further.

## 6. Proposal: a threading model for egui-pop

### 6.1 Rules

1. **The interface thread draws and decides; it never computes what can
   be computed elsewhere.** Never on it: decoding, parsing a program's
   output, highlighting a whole file, scanning a document, display
   transforms per pixel, f32 to f16 conversion, file I/O beyond a small
   settings document, waiting on a lock a worker holds while working.
2. **Every call egui-pop makes into application code from the frame
   returns at once** (`Host::dispatch`, `TileSource::request`,
   `TreeSource::load`, `AssistProvider::*`, `TerminalSource::write`):
   the trait documentation says so already; egui-pop adds a debug
   check (a call over a threshold is reported through the debug bridge
   and the log, with the trait and the method).
3. **Results come back through a queue egui-pop drains with a budget**:
   the queue is drained at the start of the frame, up to a budget (count
   or bytes or time), the rest stays queued and asks for another frame.
4. **One wakeup per queue per frame**: a producer wakes the interface
   only when its queue goes from empty to non-empty (an atomic flag the
   drain clears), not per item.
5. **No work under a lock the frame takes.** A worker prepares its
   result outside any shared lock and swaps it in (a short lock, or a
   channel); the frame copies what it draws under a short lock, or
   reads a snapshot.

### 6.2 Cancellation and back-pressure

- Every request has a cancel token (the viewport and assist have one;
  the tree source and file chooser do not yet); egui-pop cancels what
  the view no longer needs and drops stale answers (all components
  check versions already).
- Queues are bounded where a producer can outrun the interface: the
  terminal (a byte budget per frame, the reader blocking when the
  backlog is full, as a PTY's kernel buffer would), the viewport's
  tiles (the source's queue ordered by priority, cancelled jobs
  removed rather than skipped later), the tree's edits.
- Unbounded thread creation goes: actionable notifications share one
  waiting thread; a provider's slow requests share a pool.

### 6.3 The kernel-thread option

The application's kernel on its own thread is the supported path
(ADR 0010), made easier by egui-pop: a thread-safe journal, a
channel-backed `Host` and snapshot reads (follow-ups 5 and 6).
egui-pop's interface kernel stays on the interface thread; moving it is
a rewrite of the application API for a small gain (section 3.5). **For
the owner to decide.**

### 6.4 Delegated viewports

The viewport gets a delegated surface (section 4): its tiles converted
and uploaded by the source's workers into GPU textures, the interface
only drawing them; the same surface offered to applications for
graphs, scopes and histograms.

## 7. Follow-ups, by gain against cost

1. Coalesce wakeups: one repaint per queue per frame (terminal sink,
   tile sink, tree sink, assist reply). Small, local.
2. Terminal: feed the emulator on the interface thread from a bounded
   byte queue with a per-frame budget, or parse on the reader thread
   into a snapshot swapped under a short lock; never parse under the
   lock the painting holds.
3. Viewport: an upload budget per frame (tiles and bytes), the rest
   carried over; the CPU display transform and the f32 to f16
   conversion moved to the sink's (worker) side. Done, and the default
   since 2026-10-13 (sections 5.1 to 5.3); next, the GPU path's texture
   creation and copy on the workers too (section 5.3).
4. `App::logic` runs twice per eframe frame (from `eframe::App::logic`
   and again from `App::show`): run it once.
5. A thread-safe journal (`SharedJournal`) and a channel-backed `Host`
   adapter, so a kernel on its own thread is the paved road.
6. A snapshot service (an `Arc` swapped per version) for state a kernel
   publishes and blocks read lock-free.
7. A debug check for calls into application code that do not return
   at once (dispatch, request, load, assist), reported through the
   debug bridge.
8. Bounded queues and cancellation where missing: the demo source's
   queue (drop cancelled jobs), the tree's inbox, PTY input, one
   waiting thread for actionable notifications.
9. The delegated surface (section 4) in the viewport crate, then the
   viewport's tiles on it. Measured first as tiles rendered on the GPU
   by the source (`TileSink::deliver_device`, section 5.2): the
   interface thread's tile cost goes from 12 ms to under 1 ms at p99.
10. egui-pop's interface kernel on its own thread: not recommended
    (section 3.5); for the owner to decide.
