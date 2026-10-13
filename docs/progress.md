# Jobs and progress

egui-pop never runs an application's background work. The host (ADR
0010) runs it and publishes `job.*` events on its journal; egui-pop folds
them into a model, draws them, and sends the user's requests back to the
host as commands. The design notes and the survey behind this page are
in [Advanced widgets](widgets.md), section 3.

## The jobs model

`egui_pop::model::jobs::Jobs` (no egui) folds the events into jobs;
`JobsPlugin` (opt-in, `egui_pop::api::jobs`) feeds it on each pass from
the journal, behind the `JOBS` service. Nothing polls: without new
events, nothing is read.

| Event | Data |
|---|---|
| `job.started` | `id`, `title`, `kind`, `parent`, `weight`, `total` (none: indeterminate), `steps` (`[{id, title, weight}]`), `cancellable` (true), `pausable` (false), `retryable` (true) |
| `job.progress` | `id`, `done`, `total`, `failed`, `skipped`, `item`, `item_progress` (0 to 1), `phase`, `step`, `step_done`, `step_total`, `estimate` (the host's own fraction) |
| `job.item_failed` | `id`, `item`, `error` (the job goes on) |
| `job.paused`, `job.resumed`, `job.cancelling` | `id` |
| `job.failed` | `id`, `error` |
| `job.finished` | `id`, `actions` (`[{title, command, args}]`: "Show in folder") |
| `job.cancelled` | `id` |
| `job.log` | `id`, `text` (lines of the job's log) |

A `job.started` for a job already known starts it again (a retry): its
progress is reset, its log kept, its finished children forgotten.

**Every job carries an estimated overall percentage** (`Job::fraction`,
0 to 1, `Job::percent`), or none while it cannot estimate:

- a job with sub-steps rolls them up, each weighted by its estimate: its
  declared `steps` (a render: decode 1, process 3, write 1) and its
  child jobs (`parent`, each with its `weight`). Finished parts count
  whole, the current one by its own progress, the others nothing; a
  batch with a `total` counts its children not started yet;
- otherwise `done` of `total`, the current item's progress included;
- otherwise the host's `estimate`;
- otherwise the job is **indeterminate until it can estimate**, and once
  determinate it never goes back (a growing total moves the bar back
  instead);
- the application refines it with a hook on the `jobs.estimate` step
  (ADR 0011).

Time left (`Job::eta`) follows the rate of that fraction (a moving
average over the times events arrived), never before 3 seconds, never
while paused. "Now" comes from the plugin's clock: the motion clock by
default (below), a `ManualClock` of the test's own if it wants
(`JobsPlugin::clock`).

```rust
use egui_pop::api::App;
use egui_pop::api::jobs::{JOBS, JobsPlugin};
use serde_json::json;

let mut app = App::builder()
    .plugin(JobsPlugin::default())
    .build(&egui_pop::egui::Context::default())
    .unwrap();
let h = app.handle().clone();
// What a host publishes, from its own bus.
h.publish("host", None, "job.started", json!({
    "id": "render", "title": "Render",
    "steps": [
        { "id": "decode", "weight": 1 },
        { "id": "process", "weight": 3 },
        { "id": "write", "weight": 1 },
    ],
}));
h.publish("host", None, "job.progress", json!({
    "id": "render", "step": "process", "step_done": 1, "step_total": 2,
}));
app.logic(&egui_pop::egui::Context::default());
let render = h.service(&JOBS).unwrap().job("render").unwrap();
// Decode done (1), half of process (1.5), of 5.
assert_eq!(render.percent(), Some(50.0));
```

### Cancel, pause, resume, retry

They are commands the host runs, each with `{ "id" }`: `job.cancel`,
`job.pause`, `job.resume`, `job.retry`. `JobsPlugin` declares them for
the host (`Ctx::declare`), or runs them in-process with a runner the
application gives (`JobsPlugin::run_locally`). `jobs::ask(app, id,
request)` runs one as the user and marks the job: it shows "cancelling"
from the moment it was asked until the host publishes `job.cancelled`;
a refused command leaves the reason in `Job::request_error`. The model
follows the commands' `command.dispatched` and `command.failed` events,
so a request from an agent or a script shows the same way.

### The `jobs.estimate` step

| Step | Arguments | Result |
|---|---|---|
| `jobs.estimate` | `id`, `title`, `kind`, `fraction` (the model's own, `null` while it cannot), `done`, `total`, `steps`, `step`, `elapsed` | `{ "fraction": 0 to 1 }`, or the model's own (or `null`) to leave it to the model |

It runs only when a plugin hooks it, once per pass for each active job
that moved, so a job's progress does not double the journal.

```rust
use egui_pop::prelude::*;
use egui_pop::api::jobs::STEP_ESTIMATE;

struct ScanEstimate;

impl Plugin for ScanEstimate {
    fn id(&self) -> PluginId {
        PluginId::from_static("scan_estimate")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        // A scan cannot count what is left; a library holds about 500
        // folders.
        ctx.replace(STEP_ESTIMATE, |_, args| {
            if args["kind"] != "scan" || !args["fraction"].is_null() {
                return Ok(serde_json::json!({ "fraction": args["fraction"] }));
            }
            let done = args["done"].as_f64().unwrap_or(0.0);
            Ok(serde_json::json!({ "fraction": (done / 500.0).min(0.95) }))
        });
        Ok(())
    }
}
# let _ = ScanEstimate;
```

### Agents and tests

The debug bridge's `jobs` collector shows every job (state, fraction,
counts, time left, the end of its log) and `idle`, what an agent waits
for; its actions `cancel`, `pause`, `resume`, `retry`, `dismiss` and
`clear` do what the buttons do.

`egui_pop_test::jobs::FakeHost` is a host whose jobs a test scripts,
event by event, without threads; it answers the job commands now, later
(until `release`) or never (`Answer::Refuse`):

```rust,ignore
let host = FakeHost::new();
host.answer(Answer::Later);
let mut app = App::builder().host(host.clone()).plugin(JobsPlugin::default())
    .build(&ctx).unwrap();
host.job("export").start("Export", Some(4)).item(1, "a.raw").fail("disk full");
```

## The motion clock

Spinners, travelling bars, shimmer and the busy delay read the motion
clock (`egui_pop::core::motion`), not the input's time directly:

- `motion::now(ctx)`: the interface's time, or a clock the application
  or a test gives (`set_clock`, `freeze`). A given clock moves only when
  told: a test freezes motion, steps it, and gets the same frames on
  every run. The jobs model reads the same clock, so a job's start and a
  busy indicator's delay agree;
- `motion::phase(ctx, period, still)`: where a looping animation is (0
  to 1); `still` under reduced motion (`motion.duration` is 0), so
  animated widgets give still frames;
- `motion::animate(ctx)`: asks for the next frame of an animation on
  screen, never at once (a headless harness settles), four a second
  while the window is not focused, never under reduced motion or a given
  clock;
- `motion::Busy`: the delayed busy indicator. Nothing shows before
  `motion.spinner.delay` (300 ms): most work ends sooner, and a flash of
  spinner reads as a glitch. Once shown it stays `motion.spinner.min`
  (500 ms), so it does not flash off. The delay counts from when the
  work started (a job's or a command's start), not from when the widget
  was first drawn, so a panel opened midway shows the right state at
  once; the frame where the indicator appears is asked for, no timer
  thread runs.

| Token | Default | What |
|---|---|---|
| `motion.spinner.delay` | 300 ms | work shorter than this shows no busy indicator |
| `motion.spinner.min` | 500 ms | a busy indicator, once shown, stays this long |
| `motion.spinner.turn` | 900 ms | one turn of a spinner |
| `motion.progress.travel` | 1500 ms | an indeterminate bar's segment crossing its track |
| `motion.skeleton.shimmer` | 1600 ms | a skeleton's shimmer crossing it |

```rust
use egui_pop::core::motion::{self, Busy};

let ctx = egui_pop::egui::Context::default();
// In a test: motion stands still until told.
let clock = motion::freeze(&ctx, 0.0);
let busy = Busy::new(egui_pop::egui::Id::new("export"), 0.0);
clock.set(0.2);
assert!(!busy.visible(&ctx, true));
clock.set(0.4);
assert!(busy.visible(&ctx, true));
```

## Progress widgets

`egui_pop::core::progress` draws progress from a fraction; the jobs
centre and the status bar feed them a job's estimate. They share the
`progress.*` tokens (`track`, `fill`, `height`, `line`, `ring`) and the
motion clock.

| Widget | What |
|---|---|
| `ProgressBar` | determinate (`new(Some(f))`, `counts(done, total)`) or indeterminate (`new(None)`: a segment travelling along the track; a still striped track under reduced motion); tones (`Tone::Accent` while running, `Info`, `Success`, `Warning`, `Danger`, `Muted` when paused); stacked `section`s after the fill (failed, skipped); `line()` for the thin line; `caption(true)` draws the name and the value text above it |
| `Ring` | a small circle: an arc from the top, or a turning arc when indeterminate |
| `Meter` | a value against thresholds (disk space, memory), its tone the highest threshold reached; AccessKit role `Meter` |
| `Spinner` | a turning arc with a label; `delayed(id, since, busy)` shows it only after `motion.spinner.delay` and keeps it `motion.spinner.min`, its space kept while hidden |

Each is an AccessKit `ProgressIndicator` with its name (`label`); a
determinate one has a numeric value and its range (0 to the total with
`counts`, otherwise 0 to 100) and a value text ("340 of 1,204", or the
`text` given); an indeterminate one has no value. Colour is never the
only signal: the state is in the name or the text.

```rust
use egui_pop::core::progress::{ProgressBar, Spinner, Tone};

fn ui(ui: &mut egui_pop::egui::Ui, started: f64, loading: bool) {
    ui.add(
        ProgressBar::counts(340, 1204)
            .label("Importing photos")
            .section(61.0 / 1204.0, Tone::Danger, "failed")
            .text("340 of 1,204, 61 failed")
            .caption(true),
    );
    ui.add(
        Spinner::new()
            .label("Loading thumbnails")
            .delayed(egui_pop::egui::Id::new("thumbnails"), started, loading),
    );
}
```

## Busy controls and the status bar

A command the host answers later (`Reply::Later`) shows as running
until its result arrives (`AppHandle::running`, and
`AppHandle::running_since`, by the motion clock). Controls bound to it
show it:

- `egui_pop::api::busy::CommandButton`: a button running a command,
  greyed while it waits; after `motion.spinner.delay` a spinner takes
  its icon's place for at least `motion.spinner.min`. Its width and its
  name stay, AccessKit's `busy` flag is set, and a failed run says why
  on hover;
- menu items bound to the command do the same;
- a status bar indicator (`strips::IndicatorState::busy`, the time the
  work started) shows a delayed spinner before its text;
- `JobsPlugin` adds the status bar's jobs item: a ring with the jobs'
  overall estimate (turning while one cannot estimate), "Jobs: 3
  (42%)", and "Failed: 1"; nothing while there is nothing to say.

```rust
use egui_pop::api::AppHandle;
use egui_pop::api::busy::CommandButton;
use egui_pop::core::icon::Icon;
use egui_pop::model::ids::CommandId;

fn toolbar(ui: &mut egui_pop::egui::Ui, app: &AppHandle) {
    ui.add(CommandButton::new(app, CommandId::from_static("doc.export")).icon(Icon::Save));
}
```

## The jobs centre

`egui_pop::api::jobs_centre` draws every job the host runs, the same
list in two places: the **Jobs panel** (declared by `JobsPlugin`,
closed until asked for; `without_panel()` leaves it out) and a
**popover** from the status bar's jobs item.

- Each job: a chevron, its title and state, a bar of its estimated
  overall percentage (failed and skipped items stacked after the fill),
  the percentage, "n of m", the current step and item, the time left,
  the reason it failed or why the host refused a request, its failed
  items (folded), and buttons asking the host: Pause, Resume, Cancel,
  Retry; Dismiss forgets a finished one, its actions ("Show") run their
  commands. A batch's children fold under it. Finished jobs go to the
  history, cleared by hand ("Clear finished").
- **The chevron opens the job's log console**, collapsed by default and
  remembered per job (`JobsService::expanded`), so the panel, the
  popover and a toast agree. The console streams the job's `job.log`
  lines.
- Buttons are named with their job for assistive technologies ("Cancel:
  Export 12 images"); the chevron says what it does ("Show the log of
  Export 12 images").
- **A toast can carry a job** (`Toast::info("Exporting").job(id)`): its
  bar, Cancel while it runs, its first action once it ended. It stays
  while the job goes on; closing it does not cancel the job.

**Where the console comes from (layering).** The jobs centre is part of
the application API (L3), which sits below `egui-pop-editors` (L4,
ADR 0001), so it cannot use the editors' `Terminal`. It draws plain
lines (ANSI escapes dropped) by itself, and looks for a `JOB_LOG_VIEW`
service first: `egui_pop::editors::JobConsolesPlugin` provides one, a
`Terminal` over a `Console` source per job, fed the job's new lines as
they come (colours, scrollback, selection, copy; with `TerminalPlugin`,
agents read it as the terminal `job:<id>`). The facade offers it with
its `editors` feature; an application without the editors keeps the
plain console.

```rust
use egui_pop::prelude::*;
use egui_pop::api::jobs::JobsPlugin;
use egui_pop::editors::{JobConsolesPlugin, TerminalPlugin};

let app = App::builder()
    .plugin(JobsPlugin::default())
    .plugin(TerminalPlugin)
    .plugin(JobConsolesPlugin)
    .build(&egui_pop::egui::Context::default())
    .unwrap();
# let _ = app;
```

## Skeletons and empty states

`egui_pop::core::placeholder`:

- `Skeleton` stands in for content that fills progressively, in its
  layout: `Skeleton::text(lines)` for a paragraph, `Skeleton::rows(n)`
  for list rows (a block and two lines each). Shapes in `skeleton.fill`,
  a shimmer crossing them in `skeleton.shimmer` every
  `motion.skeleton.shimmer`; no shimmer under reduced motion nor under a
  given motion clock, so tests get still frames. Its region carries
  AccessKit's `busy` flag and a name ("Loading the metadata", "Loading"
  by default). Not for thumbnails: show what is known instead (an
  embedded preview, an average colour, as the thumbnail grid does).
- `EmptyState` says why a place is empty and what to do: an icon, a
  title, a sentence, an action (`show` returns whether it was clicked;
  the application runs its command), `danger()` when something went
  wrong.

```rust
use egui_pop::core::icon::Icon;
use egui_pop::core::placeholder::{EmptyState, Skeleton};

fn queue(ui: &mut egui_pop::egui::Ui, rows: Option<&[String]>) -> bool {
    match rows {
        None => {
            ui.add(Skeleton::rows(4).label("Loading the queue"));
            false
        }
        Some([]) => EmptyState::new(Icon::Folder, "The queue is empty")
            .text("Export images to fill it.")
            .action("Export")
            .show(ui),
        Some(rows) => {
            for r in rows {
                ui.label(r);
            }
            false
        }
    }
}
```

## Gaps

- A job with no event for a long time is not shown as "not responding"
  yet (the host would declare its timeout).
- Queued jobs (a position in a queue, reordering) are not modelled.
