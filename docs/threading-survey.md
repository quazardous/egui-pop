# Threading survey: how photo and desktop applications keep the UI responsive

This survey feeds the threading design of egui-pop (immediate-mode UI on egui)
and Helios (node-based raw editor on wgpu). It looks at how established
applications split work between the UI thread, processing threads and the GPU.
For each one it asks the same six questions: where the state lives, whether the
UI blocks, whether there is a low-resolution path, how tiles are scheduled and
cancelled, how GPU work reaches the screen, and what changed over time.

Sources are cited as repository-relative file paths with line ranges (and the
commit or branch read), or as URLs. Line numbers drift; they are given so a
reader can find the passage, not as a stable API.

**Not checked against a source.** A few statements come from general
knowledge rather than a file or page opened for this survey; check them
before relying on them:

- Krita's 64×64 tiles and its use of `glTexSubImage` for canvas updates;
- Blender's Cycles display driver, the draw manager rewrite and the
  Vulkan backend details;
- Rerun decoding video off the UI thread and showing the last decoded
  frame;
- ART keeping RawTherapee's processing engine;
- every note on Affinity;
- the exact egui-wgpu API names.

Versions read:

- darktable: master-based tree at commit `ce9ccb225` (October 2026),
  <https://github.com/darktable-org/darktable>.
- vkdt: commit `afc3425` (October 2026), <https://github.com/hanatos/vkdt>.
- RawTherapee: `dev` branch (October 2026),
  <https://github.com/RawTherapee/RawTherapee>.
- GIMP: `master` (October 2026), <https://gitlab.gnome.org/GNOME/gimp>.
- Krita: `master` (October 2026), <https://invent.kde.org/graphics/krita>.
- Blender, Zed/GPUI, Rerun, Adobe and Affinity: public docs and source, URLs
  inline.

## Comparison table

| Application | Where the state lives | UI blocks on processing? | Low-res preview path | Tile / region scheduling and cancellation | GPU to screen |
|---|---|---|---|---|---|
| darktable | GUI thread owns `dt_develop_t` and history; pipes copy params under `history_mutex` | Briefly: `backbuf_mutex` in expose, `history_mutex` on sync. Never for a full run | Yes: a separate always-whole-image preview pipe at small scale, drawn as fallback | No tiles for display: full pipe renders only the visible ROI; restart via atomic shutdown flag checked between modules | OpenCL compute, result copied to host `backbuf`, painted by Cairo on the GTK thread |
| vkdt | Main (render) thread owns the graph; graph runs are recorded and submitted from it | No: GPU runs async, double-buffered display images | Not as a second pipe; whole graph is fast enough on GPU at full res; thumbnails at low res | No tiling; re-run from the changed module (`runflags`); the GPU finishes, results are just superseded | Graph output stays on the GPU, sampled directly by the UI renderer (no readback) |
| RawTherapee / ART | `ImProcCoordinator` owns params; GUI posts `nextParams` + change mask | No: processing in a coordinator thread; results posted via idle callbacks | Yes: main preview at a reduced "skip" scale; detail windows (crops) at 1:1 | Crops are ROIs, not tiles; updates coalesced with "pending" flags, no mid-run cancel | CPU only; pixbufs handed to GTK on the main loop |
| GIMP + GEGL | Main thread owns the image and the GEGL graph | Partly: projection is rendered in main-loop idle chunks sized to ~1/15 s; GEGL parallelises each chunk | Yes: GEGL buffer mipmap levels when zoomed out; canvas render cache | Chunk iterator with a priority rect (the viewport) served first; chunks can be stopped and the leftover region kept | CPU render into a Cairo surface cache, then GTK draw |
| Krita | `KisImage` and nodes, mutated only through strokes on the update scheduler's worker threads | No for painting; some global operations take an exclusive lock and show a busy state | Yes: Instant Preview (level-of-detail strokes) paints on a downscaled copy, full-res catches up | Strokes queue with job types (sequential, concurrent, barrier, exclusive); updates by dirty rect; strokes cancellable with revert | Workers convert dirty tiles into texture update info; GUI thread uploads to OpenGL tile textures |
| Blender | Main thread owns original DNA; depsgraph evaluates a copy-on-write copy | Yes for depsgraph evaluation (parallel inside, but synchronous to the UI); no for wmJobs and render engines | Viewport render "start resolution" / pixel size; progressive refinement in Cycles | Cycles restarts the session on change; wmJobs are replaced per owner and asked to stop | Draw manager on the main thread; Cycles uploads its buffer to a texture through a display driver |
| Lightroom / Photoshop / Affinity | Not public in detail | Documented as asynchronous for previews; Photoshop has background save | Lightroom Smart Previews and preview cache; Photoshop cache levels (image pyramid) | Photoshop tile size setting; details not public | GPU display and processing toggles (Lightroom, Photoshop); Metal/DirectX compute (Affinity) |
| Zed GPUI | Main thread owns all entities in `App`; `!Send` foreground executor | No by convention: heavy work goes to the background executor and returns via the main-thread executor | n/a | Tasks are futures; dropping a `Task` cancels it | Scene built on main thread, rendered by the platform renderer each frame |
| Rerun (egui) | UI thread owns the viewer state; ingestion on background receivers | No: the store is queried every frame; ingestion and decoding are off-thread | Level-of-detail for large point clouds/plots; shows the last decoded video frame | Per-frame queries; work superseded rather than cancelled | `re_renderer` on wgpu, drawn inside egui via paint callbacks |

## Per-application notes

### darktable

**Threads.** darktable runs GTK on a single GUI thread and a pool of job
workers. `src/control/jobs.h` lines 27-52 define three *reserved* workers, one
per display pipe (`DT_CTL_WORKER_ZOOM_1` for the full pipe, `ZOOM_FILL` for the
preview pipe, `ZOOM_2` for the second window), plus five prioritised queues for
the general pool (user foreground, system foreground, user background, export,
system background). The scheduler in `src/control/jobs.c` lines 251-275 picks
the highest-priority queue head and ages the losers. Adding a job that already
sits in a queue moves it to the top instead of duplicating it, and queues are
capped at 30 jobs with the oldest discarded (`jobs.c` lines 440-480). Reserved
workers sleep on a condition variable (`jobs.c` lines 506-535).

**State.** The develop state (`dt_develop_t`, history stack, module params)
lives on the GUI thread. A pipe run copies parameters into its own per-node
"pieces" under `dev->history_mutex` (`src/develop/pixelpipe_hb.c` lines
898-937, `dt_dev_pixelpipe_change`). The pipe never reads live GUI state during
processing, which is what makes restarting safe.

**Three pipes.** `src/develop/develop.c` lines 296-330 schedule the full pipe,
the preview pipe and the second-window pipe as separate jobs on their reserved
workers. The preview pipe processes the whole image at small scale; the full
pipe processes only the visible region of interest at the current zoom. While
the full pipe is not ready, the view paints the preview pipe's buffer, scaled,
as a fallback (`src/views/view.c` lines 1990-2010).

**Cancellation.** There is no per-tile cancellation. A pipe has an atomic
`shutdown` field set by `dt_dev_pixelpipe_set_shutdown` (`pixelpipe_hb.c`
lines 409-425). The recursive processor checks it between modules
(`_module_pipe_stop`, `pixelpipe_hb.c` lines 1461-1530) and long modules can
poll `dt_dev_piece_shutdown` (lines 1534-1552). The image job then restarts
from the top (`develop.c` lines 887-945). A recent refinement avoids
wasteful restarts: if a parameter change arrives less than half an average run
after the pipe started, the running pipe is *not* stopped, and the next run is
simply queued (`develop.c` lines 283-305, "How do we handle UI responsiveness").
Some stop reasons are ignored when the pipe is close to finishing, since a
result that is slightly stale but visible is better than none
(`pixelpipe_hb.c` lines 1474-1508).

**Cache.** Each pipe keeps a per-module output cache keyed by a hash of
params and ROI; lines are weighted, and outputs of modules flagged as
"important" (the one being edited) are protected from eviction
(`src/develop/pixelpipe_cache.c` lines 199-297, 400). A slider drag therefore
re-runs only from the edited module down.

**GPU.** OpenCL devices are locked per pipe type with a configurable priority
list per pipe (image, preview, export, thumbnail, preview2). A pipe that does
not get its preferred device falls back to the CPU, or for "mandatory"
devices retries every 5 ms up to a timeout (`src/common/opencl.c` lines
1843-1979 for the priority parsing, 2012-2105 for `dt_opencl_lock_device`).
Results are always copied back to host memory: the pipe's final buffer is
`memcpy`'d into `pipe->backbuf` under `backbuf_mutex` (`pixelpipe_hb.c` lines
3607-3639, with a `FIXME` about copying), and the GTK expose handler locks the
same mutex to paint it with Cairo (`src/views/darkroom.c` lines 576-592,
`src/views/view.c` lines 1861-2037). Signals raised from workers are marshalled
to the GUI thread with `g_main_context_invoke_full` (`src/control/signal.c`
lines 378-392).

**Where the UI blocks.** Never on a whole pipe run, but in a few short
critical sections: the expose handler waits on `backbuf_mutex` while a worker
copies a frame; history edits take `history_mutex`, which a pipe holds while
syncing pieces; leaving darkroom waits for running pipes (`develop.c` lines
185-200).

**Change over time.** The second-window pipe (preview2) was added later with
its own reserved worker. OpenCL scheduling evolved from "first free device"
to per-pipe priority lists with mandatory devices and scheduling profiles
(see the darktable user manual, "memory & performance" preferences,
<https://docs.darktable.org/usermanual/development/en/preferences-settings/processing/>).
The shutdown mechanism grew from a boolean into a set of stop reasons with
rules for ignoring late requests. The pixelpipe cache moved from a fixed number
of lines to memory-based limits with "important" lines. The remaining pain
points are visible in code comments: host round-trips for display, and pipe
restarts that waste work.

### vkdt

vkdt is Johannes Hanika's GPU-first successor to darktable's processing
model: every processing node is a Vulkan compute shader, and the whole DAG is
scheduled as one command buffer (`src/pipe/readme.md`; "this complete DAG of
all atomic nodes directly translates into vulkan/glsl with one compute shader
per node").

**State and threads.** The GUI thread (GLFW + Nuklear in current versions;
early versions used Dear ImGui) owns the graph and decides each frame what to
re-run through a `runflags` bit set: recompute ROIs, re-create nodes,
re-allocate, re-record the command buffer, upload the source, download sinks,
wait for the fence (`src/pipe/modules/api.h` lines 24-37). A parameter change
usually only needs `s_graph_run_record_cmd_buf`, re-running from the changed
module onwards.

**UI never blocks.** `src/gui/render_darkroom.c` lines 1247-1258 run the graph
"double buffered async": the graph is submitted without waiting for the fence,
on whichever of two output buffers the display is not currently using, and only
if `dt_graph_display_have_free_process` says one is free. Otherwise the request
stays in `runflags` for the next frame, which coalesces changes naturally.
`src/gui/graph-display.h` lines 1-23 explain the buffer protocol: the render
thread locks at most one buffer, and a third one may be needed briefly when the
graph topology (and thus image size) changes.

**GPU to screen.** The output never leaves the GPU: the display node's image is
sampled by the UI renderer in the same Vulkan device. This is the main
structural difference from darktable.

**Low-res path and tiles.** There is no separate preview pipe and no tiled
display rendering; the design relies on the GPU processing the full image fast
enough per interaction. Thumbnails for the lighttable are generated by
background tasks on a small thread pool (`src/db/thumbnails.c` around line 400,
`threads_task("thumb", ...)`), tagged with a job timestamp so stale results are
ignored. Exports also run as thread-pool tasks
(`src/gui/render_lighttable.c` line 593).

**Lesson.** When the whole graph fits on the GPU, a single owner thread that
records command buffers and never waits on fences gives responsiveness without
any locking of image data. The cost is memory: everything is allocated up
front in one big allocation per graph.

### RawTherapee and ART

ART is a fork of RawTherapee and keeps the same engine structure
(`ImProcCoordinator`, crops), so the notes below apply to both.

**State and threads.** The GUI writes new parameters into `nextParams` and
ORs a change code into `changeSinceLast`, then calls `startProcessing`, which
spawns the coordinator thread if it is not already running
(`rtengine/improccoordinator.cc` lines 3491-3515). The coordinator's
`process()` loops while `changeSinceLast` is non-zero, copying `nextParams`
into its working `params` at the start of each loop (lines 3517-3570). This is a
classic "latest wins" mailbox: intermediate slider values are dropped, and the
running pass is never interrupted.

**Low-res path.** The main preview is computed from the raw at a reduced
scale ("skip" factor), which keeps the editor interactive. Detail windows
(the 1:1 crop windows, and the main view when zoomed to 100%) are `Crop`
objects that process only their region at `skip == 1`
(`rtengine/dcrop.cc` lines 160-250).

**Crops.** Each crop is updated by the coordinator when a change affects it
(`improccoordinator.cc` lines 2749-2753), or by its own low-priority thread
when the user pans (`rtgui/crophandler.cc` line 453). `Crop::tryUpdate` and
`Crop::fullUpdate` (`dcrop.cc` lines 2237-2286) coalesce requests with an
`updating` / `newUpdatePending` pair: a request during an update only sets the
pending flag, and the running thread loops once more. Some expensive tools
(noise reduction) are computed tile-wise inside the crop, and the preview
shows an approximation computed on representative tiles.

**To screen.** Results are converted to 8-bit pixbufs and handed to GTK
through an idle callback (`rtgui/crophandler.cc` lines 303-420, `idle_register`).
There is no GPU path.

**Known issues.** The preview at reduced scale is not exactly the full result
for scale-dependent tools (sharpening, noise reduction), which is why
RawTherapee tells users to judge such tools in a 1:1 detail window
(RawPedia, <https://rawpedia.rawtherapee.com/Editor>). The mailbox model also
means a long pass (for example wavelets on a large file) delays the next
update by up to a full pass.

### GIMP with GEGL

**State.** The image, its layers and the GEGL graph live on the main thread.
GEGL itself parallelises processing of a requested rectangle across a thread
pool, but the request is made from the main thread.

**Projection.** `app/core/gimpprojection.c` lines 505-525 state the contract:
requesting a render is thread-safe, but "the actual projection painting will
happen in the main thread", in chunks scheduled from a high-priority idle
source. `GimpChunkIterator` (`app/core/gimpchunkiterator.c` lines 38-54)
dispenses rectangular chunks whose size adapts so that each iteration takes
about 1/15 s (`DEFAULT_INTERVAL`), with power-of-two sides and a short history
of measured throughput. UI events are handled between iterations, never during
one (commit "app: add GimpChunkIterator", January 2019,
<https://lists.gnome.org/archives/commits-list/2019-January/msg04994.html>;
adaptive chunk size,
<https://mail.gnome.org/archives/commits-list/2019-July/msg10864.html>).

**Priority and cancellation.** The iterator has a *priority rect*, set to the
visible viewport (`gimp_projection_set_priority_rect`, `gimpprojection.c` lines
483-493, 670-692). The iterator first exhausts the intersection of the dirty
region with the priority rect, then the rest (`gimpchunkiterator.c` lines
167-212). Stopping an iteration returns the unprocessed region so it can be
merged into the next one (`gimpprojection.c` lines 786-820): pan and zoom move
the priority rect instead of discarding work.

**Low-res path and display.** GEGL buffers have mipmap levels, so a
zoomed-out view renders at a lower level. The canvas keeps a Cairo surface
"render cache" with a valid region; exposes paint from the cache and only
missing chunks are rendered (`app/display/gimpdisplayshell-render.c` lines
70-250).

**Change over time.** GIMP 2.10 moved to GEGL with high bit depth and made the
projection asynchronous in chunks; later work (by the developer Ell in
2018-2019) added the adaptive chunk iterator, the render cache and
`gimp_parallel_run_async` for work such as histograms and font loading.
The design stays main-thread-centric: GEGL tiles are parallel, but scheduling
is in the GTK main loop, so a very slow single chunk (a large area filter)
still stalls the UI; the July 2019 commit raises the interval for area filters
precisely because they are sensitive to chunk size.

### Krita

**State.** `KisImage` and its nodes are only mutated through *strokes* run by
`KisUpdateScheduler` on worker threads. The scheduler interleaves two queues:
strokes (user actions) and updates (recomposition of dirty rects), balanced by
a simple metric (Krita developer docs, "Strokes queue",
<https://docs.krita.org/en/untranslatable_pages/strokes_documentation.html>).

**Stroke strategies.** A stroke strategy creates init, dab, cancel and
finish jobs. Dab jobs are SEQUENTIAL (no reordering, e.g. freehand),
CONCURRENT (any order in parallel, e.g. scaling each layer), or BARRIER (waits
for all pending updates). The EXCLUSIVE property blocks every other stroke and
update. Strokes can be cancelled at any time: unstarted jobs are dropped and a
cancel job reverts done work, which is why undo commands are registered only
after execution (`KisPostExecutionUndoAdapter`). A running job is never
interrupted (`libs/image/kis_update_scheduler.cpp` lines 193-202).

**Low-res path.** Instant Preview (formerly "level of detail strokes", funded
by the 2015 Kickstarter) runs a stroke first on a downscaled copy of the image
(LOD N) for immediate feedback, then replays it at full resolution (LOD 0) in
the background (<https://docs.krita.org/en/reference_manual/instant_preview.html>;
`kis_update_scheduler.cpp` lines 215-278 for the LOD sync hooks). Known costs:
a visible "pop" when the full-res result differs (auto spacing, texture
brushes), and corner cases such as vector selections that cannot be previewed
(<https://mail.kde.org/pipermail/kimageshop/2020-October/016062.html>).

**Tiles and canvas.** Paint devices are tiled (64x64 copy-on-write tiles,
`KisTiledDataManager`, with a swapper to disk). The OpenGL canvas holds the
image as a grid of textures. When the image updates, `sigImageUpdated` is
connected with `Qt::DirectConnection` (`libs/ui/canvas/kis_canvas2.cpp` line
815), so `startUpdateCanvasProjection` runs on the worker thread: it converts
the dirty rect into display color space and prepares texture tile data
(`kis_canvas2.cpp` lines 1055-1061;
`libs/ui/opengl/kis_opengl_image_textures.cpp` lines 274-291). The result is
queued into a compressor; the GUI thread later takes all pending infos, uploads
them with `glTexSubImage`, and issues one repaint (`kis_canvas2.cpp` lines
1063-1100). Updates are throttled by signal compressors (lines 184-256).

**Change over time.** The strokes framework replaced direct, locking edits in
the Krita 2.x era; the LOD system arrived in Krita 3.0 (2016); the region of
interest for updates and multiple compressors were added to keep large
canvases responsive. Exclusive strokes (e.g. some transforms and global
filters) are still the place where the UI shows a busy cursor.

### Blender

**State.** The main thread owns the original data (DNA). Since 2.80 the
dependency graph evaluates a *copy-on-write copy*: "Dependency graph always
performs evaluation on a copy of original data"; render engines and
viewports read only evaluated data, which is "the way to solve threading
conflicts between render and viewport threads"
(<https://developer.blender.org/docs/features/core/depsgraph/>).

**UI blocking.** Depsgraph evaluation runs on the main thread's event loop
iteration, parallelised internally with a task pool (TBB), but synchronous to
the UI: a heavy geometry-nodes tree or modifier stack freezes the interface
until evaluation ends. Long work that is meant to be asynchronous goes through
`wmJob`: `startjob` runs on a worker thread, while `initjob`, `update`, `free`
and `endjob` run on the main thread, `update` at most once per timer step;
jobs never draw, they send notifiers; a new job for the same owner signals the
running one to stop and reuses it (`source/blender/windowmanager/WM_api.hh`
callback comments; "Cleanup: Comments of wmJobs callbacks", 2020,
<https://lists.blender.org/pipermail/bf-blender-cvs/2020-March/137316.html>).

**Low-res path.** The Cycles viewport starts at a reduced "start resolution"
(pixel size) and refines progressively, sample by sample. Any change resets the
session, which restarts at low resolution.

**GPU to screen.** The draw manager draws on the main thread (the GPU context
is single-threaded). Cycles renders in its own session thread and pushes its
buffer through a `DisplayDriver` (`intern/cycles/session/display_driver.h`;
Blender's implementation in `intern/cycles/blender/display_driver.cpp`), which
maps a pixel buffer and updates a texture, using a separate GPU context, so
the main thread only draws a textured quad.

**Change over time.** The 2.80 copy-on-write depsgraph replaced evaluation
on original data and enabled threaded render alongside the viewport. The draw
manager was rewritten ("draw manager next", with multithreaded mesh batch
extraction) and a Vulkan backend was added in the 4.x series. Shader
compilation moved to parallel workers to avoid UI hitches. The synchronous
depsgraph remains the main source of UI freezes.

### Lightroom, Photoshop, Affinity (public information only)

- **Lightroom Classic** builds and caches previews (standard and 1:1) and can
  edit from Smart Previews, lossy DNGs about 2560 px on the long edge, which
  makes Develop responsive and works offline
  (<https://helpx.adobe.com/lightroom-classic/help/lightroom-smart-previews.html>).
  Develop shows the cached preview first and refines to a full render. GPU use
  is split into "display" and "image processing" settings
  (<https://helpx.adobe.com/lightroom-classic/kb/lightroom-gpu-faq.html>).
- **Photoshop** exposes its tiling: "Cache Levels" (a pyramid of downscaled
  copies used to redraw zoomed-out views quickly) and "Cache Tile Size" in
  Performance preferences, plus background saving since CS6
  (<https://helpx.adobe.com/photoshop/kb/optimize-performance-photoshop.html>).
- **Affinity Photo** advertises live, non-destructive filters rendered on the
  GPU (Metal on macOS, Direct3D compute on Windows) with the result kept on the
  GPU for display. Internals are not documented publicly.

### Immediate-mode GUIs with heavy backends

**Zed GPUI.** All entities are owned by a single `App` on the main thread;
handles reach state through `App::update` and `read`, and `emit`/`notify`
are deferred and flushed at the end of an update, which gives
run-to-completion semantics (<https://zed.dev/blog/gpui-ownership>). Async
work uses two executors (`crates/gpui/src/executor.rs`): `ForegroundExecutor`
is deliberately `!Send` and polls futures on the main thread, while
`BackgroundExecutor` runs `Send` futures on a thread pool. The pattern is
"spawn on background, await, then update the entity on the foreground". A
`Task` is cancelled when dropped, so storing the latest task in an entity field
cancels the previous one automatically. Recent work replaces the executors'
internals with a shared scheduler crate while keeping the split.

**Rerun.** The viewer is an eframe/egui app on wgpu. Each frame the whole GUI
is laid out from scratch and views "query the in-RAM data store, massage the
results, and feed it to the renderer" (`ARCHITECTURE.md`,
<https://github.com/rerun-io/rerun/blob/main/ARCHITECTURE.md>). Data arrives
on background receiver threads; a "ui_waker" thread calls
`request_repaint` when a message lands (`crates/viewer/re_viewer/src/lib.rs`).
The renderer `re_renderer` assumes "most data may change every frame" and is
drawn inside egui through paint callbacks. Video frames are decoded off the UI
thread and the view shows the most recent decoded frame while the requested
one is pending. No threads on wasm32.

**egui itself.** `egui::Context` is `Send + Sync`; any thread may call
`ctx.request_repaint()` or `ctx.load_texture()`. With the wgpu backend, the
`wgpu::Device` and `Queue` are shareable across threads, so a worker can render
into its own `wgpu::Texture` and the UI registers it once as a native texture
(`egui_wgpu::Renderer::register_native_texture`) and then draws it as an
image; or a `egui_wgpu::CallbackTrait` paint callback can draw custom wgpu
passes inside the egui frame (<https://docs.rs/egui-wgpu>).

## Patterns for egui-pop and Helios

### Patterns that recur

1. **The UI thread owns the editable model; workers get snapshots.**
   darktable (params copied into pieces under a mutex), Blender (copy-on-write
   evaluated copy), RawTherapee (`nextParams`), GPUI (entities on the main
   thread). Processing never reads live UI state. For egui-pop and Helios this
   means the document (node graph, params, history) is plain data on the UI
   thread, and a run receives an immutable, cheaply cloned snapshot (`Arc` of
   the graph plus a generation number).

2. **Latest-wins mailbox instead of a queue of edits.** RawTherapee's
   `changeSinceLast`, Krita's update compressors, vkdt's `runflags`,
   darktable's "move duplicate job to top". Slider drags produce dozens of
   changes per second; only the newest matters. One slot per pipe, a
   generation counter, and stale results dropped on arrival.

3. **A separate low-resolution preview pipe.** darktable's preview pipe,
   RawTherapee's scaled preview, Krita's Instant Preview, Lightroom's Smart
   Previews, Cycles' start resolution. The cheap result is shown first and
   kept as a fallback under the expensive one. Helios should run a small
   whole-image pipe and a viewport pipe; the view composes "full ROI if valid
   for this zoom, else scaled preview". Known cost: scale-dependent nodes
   (sharpening, denoise) differ between the two, so the UI must say which one
   is shown.

4. **Viewport-first scheduling, keep the leftovers.** GIMP's priority rect is
   the clearest model: pan and zoom move the priority rect, the leftover dirty
   region is kept rather than discarded. darktable renders only the visible
   ROI; Krita converts only dirty rects. For Helios tiles: a dirty-region set
   per zoom level, served visible-first, nearest-to-centre next.

5. **Cooperative cancellation at node boundaries, with hysteresis.** Nobody
   kills a running kernel. darktable checks an atomic flag between modules and
   lets long modules poll; Krita never interrupts a running job; GPUI cancels
   at await points. darktable's later rule is worth copying: do not cancel a
   run that is close to done, since a stale frame now beats a fresh one later.

6. **Cache per node output, keyed by a hash of inputs.** darktable's pixelpipe
   cache with "important" lines for the edited module, vkdt re-running from the
   changed module. A slider drag on node N re-runs only from N.

7. **Results reach the UI thread by message plus repaint request.**
   darktable via `g_main_context_invoke`, RawTherapee via idle callbacks,
   Blender via notifiers, Rerun via `request_repaint`. Workers never touch
   widgets.

8. **Worker-side preparation, UI-side upload or no upload at all.** Krita
   prepares texture tiles on workers and uploads on the GUI thread; Cycles
   uploads through its own context; vkdt keeps the image on the GPU and the UI
   samples it. On wgpu, the vkdt model is directly available: the processing
   thread writes into a wgpu texture on the shared device, and egui draws it
   through a registered native texture id. Double (sometimes triple) buffering
   of the display texture, as in vkdt's `graph-display.h`, avoids the UI
   waiting for the GPU or the worker overwriting a texture being sampled.

### Patterns that fail or cost

- **Processing on the UI thread, even chunked.** GIMP shows the limit: a
  chunk that takes longer than expected (area filters) stalls input. Blender's
  synchronous depsgraph is its main freeze source. In egui this is worse:
  any blocking in `update()` freezes every widget and the repaint loop.
- **Host round-trips for display.** darktable copies the GPU result to a host
  `backbuf` and paints with Cairo, with a `FIXME` on the copy. For Helios on
  wgpu, reading back to the CPU only to upload again to egui doubles memory
  traffic; keep display output on the GPU.
- **Shared mutable buffers guarded by a mutex the UI also takes.** darktable's
  `backbuf_mutex` and `history_mutex` are short but are exactly where the GTK
  thread can wait on a worker. Prefer handing over ownership (swap an `Arc`,
  or a texture index) over locking.
- **Restart storms.** darktable needed restart counters ("LOOPING") and rules
  to ignore late stop requests; RawTherapee's no-cancel mailbox delays updates
  by a full pass. Both extremes hurt; cancel at node boundaries, with a
  "nearly done" exception.
- **One global exclusive lock.** Krita's exclusive strokes and darktable's
  pipe-wide mutex serialise everything; needed for topology changes, but they
  should be rare and visible (busy state), never on parameter edits.
- **Preview and full results disagreeing silently.** Krita's "pop",
  RawTherapee's advice to check 1:1 crops. The UI should label which
  resolution is displayed and swap without flicker.

### Fit for an egui app

- **Kernel thread:** fits. The egui `update()` loop stays the owner of the
  model and of egui state; a "kernel" thread (Helios's processing scheduler)
  owns the graph runtime, caches and wgpu command recording, and talks to the
  UI only through channels: snapshots in, results and progress out, with
  `request_repaint` on arrival. This mirrors GPUI's foreground/background
  split and vkdt's single recording thread.
- **Preview pipe:** fits and is cheap to add given a node graph: the same graph
  instantiated at a lower scale, scheduled at higher priority.
- **Tile priority and cancellation:** fits as GIMP-style priority regions plus
  darktable-style cooperative checks at node boundaries. Dropping a task
  handle (GPUI style) is a clean Rust idiom for "this request is obsolete".
- **Worker-rendered textures:** fits well on wgpu (shared device, native
  texture registration, double buffering). On the egui glow backend it does
  not: OpenGL contexts are thread-bound, so the Krita model (prepare on worker,
  upload on UI thread) would be needed there.
- **Fails in egui:** anything that blocks inside `update()` (waiting on a
  fence, a mutex held by a worker, or a synchronous evaluation), and per-widget
  locking of shared state, since immediate mode re-reads everything every
  frame.
