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
while paused. "Now" comes from the plugin's clock: the interface's time
by default, a `ManualClock` in tests (`JobsPlugin::clock`).

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

## Gaps

- A job with no event for a long time is not shown as "not responding"
  yet (the host would declare its timeout).
- Queued jobs (a position in a queue, reordering) are not modelled.
