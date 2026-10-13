//! Jobs, as a plugin ([`JobsPlugin`], opt-in): the host's background
//! work, followed from its journal. The host publishes `job.*` events
//! (see [`egui_pop_model::jobs`] for their data); each pass the plugin
//! folds the new ones into the [`Jobs`] model behind the [`JOBS`]
//! service. Nothing polls: without events, nothing is read.
//!
//! - **Every job carries an estimated overall fraction**
//!   ([`Job::fraction`]): sub-steps and child jobs roll up, weighted by
//!   their estimates; a job that cannot estimate is indeterminate until
//!   it can. The `jobs.estimate` step (ADR 0011) lets an application
//!   refine it: a run-hook or an after-hook sees the job and gives
//!   `{ "fraction": 0.4 }` (or `null`: the model's own). It runs only
//!   when a plugin hooks it, once per pass for each job that moved, so
//!   a job's progress does not double the journal.
//! - Cancel, pause, resume and retry are commands (`job.cancel`,
//!   `job.pause`, `job.resume`, `job.retry`, each `{ "id" }`) declared
//!   for the host to run, or run in-process by a runner the application
//!   gives ([`JobsPlugin::run_locally`]). [`ask`] runs one and marks the
//!   job as waiting until the host answers.
//! - Time left comes from the plugin's clock ([`JobsPlugin::clock`]),
//!   the motion clock by default (`egui_pop_core::motion`, frozen in
//!   tests); tests may give a
//!   [`ManualClock`](egui_pop_model::clock::ManualClock) of their own.
//! - The debug bridge's `jobs` collector shows every job (state,
//!   fraction, counts, time left, the end of its log) and whether the
//!   application is idle; its actions ask the host as the buttons do.
//!
//! ```
//! use egui_pop_api::App;
//! use egui_pop_api::jobs::{JOBS, JobsPlugin};
//! use egui_pop_model::commands::Origin;
//! use serde_json::json;
//!
//! let mut app = App::builder()
//!     .plugin(JobsPlugin::default())
//!     .build(&egui::Context::default())
//!     .unwrap();
//! let h = app.handle().clone();
//! h.publish("host", None, "job.started", json!({ "id": "export", "title": "Export", "total": 8 }));
//! h.publish("host", None, "job.progress", json!({ "id": "export", "done": 2 }));
//! app.logic(&egui::Context::default());
//! let jobs = h.service(&JOBS).unwrap();
//! assert_eq!(jobs.job("export").unwrap().fraction, Some(0.25));
//! assert!(!jobs.idle());
//! ```

use std::collections::BTreeSet;
use std::fmt;
use std::rc::Rc;
use std::sync::Mutex;

use egui::Ui;
use egui_pop_core::debug::FnCollector;
use egui_pop_core::lock;
use egui_pop_core::progress::{Tone, paint_ring};
use egui_pop_core::strings;
use egui_pop_core::theme::Tokens;
use egui_pop_model::clock::SharedClock;
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::ids::{BlockId, CommandId, PluginId, StepId};
use egui_pop_model::jobs::{Incoming, Job, Jobs, Request, Summary};
use serde_json::{Value as Json, json};

use crate::app::AppHandle;
use crate::hooks::On;
use crate::host::Seq;
use crate::services::ServiceToken;
use crate::steps::StepSpec;
use crate::strings as txt;
use crate::zones::{self, Block};
use crate::{Ctx, Plugin, PluginError};

/// The jobs, as a service.
pub const JOBS: ServiceToken<JobsService> = ServiceToken::new("egui_pop.jobs");

pub const CMD_CANCEL: CommandId = CommandId::from_static("job.cancel");
pub const CMD_PAUSE: CommandId = CommandId::from_static("job.pause");
pub const CMD_RESUME: CommandId = CommandId::from_static("job.resume");
pub const CMD_RETRY: CommandId = CommandId::from_static("job.retry");

/// A job's estimate, refined by the application: `{ "id", "title",
/// "kind", "fraction" (the model's own, `null` while it cannot
/// estimate), "done", "total", "steps", "step", "elapsed" }`; the result
/// is `{ "fraction": 0 to 1 }`, or the model's own (or `null`) to leave
/// it to the model.
pub const STEP_ESTIMATE: StepId = StepId::from_static("jobs.estimate");

/// The command asking `request`.
pub fn command_of(request: Request) -> CommandId {
    match request {
        Request::Cancel => CMD_CANCEL,
        Request::Pause => CMD_PAUSE,
        Request::Resume => CMD_RESUME,
        Request::Retry => CMD_RETRY,
    }
}

#[derive(Debug, Default)]
struct State {
    jobs: Jobs,
    /// The last event folded.
    seen: Seq,
    /// The motion clock's time at the last pass.
    now: f64,
    /// The jobs whose log console is open.
    expanded: BTreeSet<String>,
}

/// The jobs, folded from the journal: what views and agents read.
pub struct JobsService {
    state: Mutex<State>,
    clock: Option<SharedClock>,
}

impl fmt::Debug for JobsService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JobsService")
            .field("state", &self.state)
            .field("clock", &self.clock.is_some())
            .finish()
    }
}

impl JobsService {
    fn new(jobs: Jobs, clock: Option<SharedClock>) -> Self {
        Self {
            state: Mutex::new(State {
                jobs,
                ..State::default()
            }),
            clock,
        }
    }

    /// Now, by the plugin's clock (the motion clock by default:
    /// `egui_pop_core::motion`, frozen in tests).
    pub fn now(&self) -> f64 {
        match &self.clock {
            Some(c) => c.now(),
            None => lock(&self.state).now,
        }
    }

    /// Reads the model.
    pub fn with<R>(&self, f: impl FnOnce(&Jobs) -> R) -> R {
        f(&lock(&self.state).jobs)
    }

    pub fn job(&self, id: &str) -> Option<Job> {
        self.with(|j| j.get(id).cloned())
    }

    /// Every job, in the order they first started.
    pub fn all(&self) -> Vec<Job> {
        self.with(|j| j.all().into_iter().cloned().collect())
    }

    /// The top-level jobs, in order.
    pub fn roots(&self) -> Vec<Job> {
        self.with(|j| j.roots().into_iter().cloned().collect())
    }

    pub fn children(&self, id: &str) -> Vec<Job> {
        self.with(|j| j.children(id).into_iter().cloned().collect())
    }

    pub fn summary(&self) -> Summary {
        self.with(Jobs::summary)
    }

    /// Whether no job goes on.
    pub fn idle(&self) -> bool {
        self.with(Jobs::idle)
    }

    /// A job's log from line `from` on, as far as it is kept.
    pub fn log_from(&self, id: &str, from: u64) -> Vec<String> {
        self.with(|j| j.log_from(id, from).into_iter().map(String::from).collect())
    }

    /// Forgets a finished job.
    pub fn dismiss(&self, id: &str) -> bool {
        let mut s = lock(&self.state);
        s.expanded.remove(id);
        s.jobs.dismiss(id)
    }

    /// Forgets every finished job.
    pub fn clear_finished(&self) {
        let mut s = lock(&self.state);
        s.jobs.clear_finished();
        let State { jobs, expanded, .. } = &mut *s;
        expanded.retain(|id| jobs.get(id).is_some());
    }

    /// Overrides a job's estimate (0 to 1), or gives it back to the
    /// model.
    pub fn refine(&self, id: &str, fraction: Option<f64>) {
        let now = self.now();
        lock(&self.state).jobs.refine(id, fraction, now);
    }

    /// Notes that `request` was asked of the host for a job.
    pub fn mark(&self, id: &str, request: Request) {
        lock(&self.state).jobs.request(id, request);
    }

    /// Whether a job's log console is open (remembered per job, shared
    /// by every view of it).
    pub fn expanded(&self, id: &str) -> bool {
        lock(&self.state).expanded.contains(id)
    }

    pub fn set_expanded(&self, id: &str, open: bool) {
        let mut s = lock(&self.state);
        if open {
            s.expanded.insert(id.into());
        } else {
            s.expanded.remove(id);
        }
    }

    /// What the `jobs` collector shows.
    pub fn describe(&self) -> Json {
        let now = self.now();
        let s = lock(&self.state);
        let mut d = s.jobs.describe(now);
        d["expanded"] = json!(s.expanded);
        d["now"] = json!(now);
        d
    }

    /// Folds the journal's new events; the jobs that moved.
    fn feed(&self, app: &AppHandle) -> BTreeSet<String> {
        let time = egui_pop_core::motion::now(&app.egui());
        let seen = {
            let mut s = lock(&self.state);
            s.now = time;
            s.seen
        };
        let last = app.last_event();
        if last <= seen {
            return BTreeSet::new();
        }
        let events = app.events_since(seen);
        let now = self.now();
        let mut s = lock(&self.state);
        for e in &events {
            let incoming = Incoming {
                seq: e.seq,
                source: &e.source,
                cause: e.cause,
                name: &e.name,
                data: &e.data,
            };
            s.jobs.apply(&incoming, now);
        }
        s.seen = last;
        s.jobs.take_changed()
    }
}

/// Asks the host to cancel, pause, resume or retry a job: runs its
/// command as the user, and marks the job as waiting for the answer.
pub fn ask(app: &AppHandle, id: &str, request: Request) {
    if let Some(jobs) = app.service(&JOBS) {
        jobs.mark(id, request);
    }
    app.run(command_of(request), json!({ "id": id }));
}

/// Runs a job command in-process: the application's own job runner.
pub type Runner = Rc<dyn Fn(&AppHandle, Request, &str) -> Result<Json, String>>;

/// The plugin.
#[derive(Clone, Default)]
pub struct JobsPlugin {
    clock: Option<SharedClock>,
    runner: Option<Runner>,
    log_lines: Option<usize>,
    history: Option<usize>,
}

impl fmt::Debug for JobsPlugin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JobsPlugin")
            .field("clock", &self.clock.is_some())
            .field("runner", &self.runner.is_some())
            .field("log_lines", &self.log_lines)
            .field("history", &self.history)
            .finish()
    }
}

impl JobsPlugin {
    /// Where time left and durations take "now" from (the motion clock
    /// by default).
    pub fn clock(mut self, clock: SharedClock) -> Self {
        self.clock = Some(clock);
        self
    }

    /// Runs the job commands in-process instead of declaring them for
    /// the host: `f(app, request, job id)`.
    pub fn run_locally(
        mut self,
        f: impl Fn(&AppHandle, Request, &str) -> Result<Json, String> + 'static,
    ) -> Self {
        self.runner = Some(Rc::new(f));
        self
    }

    /// Keeps `lines` lines of each job's log
    /// ([`LOG_LINES`](egui_pop_model::jobs::LOG_LINES) by default).
    pub fn log_lines(mut self, lines: usize) -> Self {
        self.log_lines = Some(lines);
        self
    }

    /// Keeps `jobs` finished jobs
    /// ([`HISTORY`](egui_pop_model::jobs::HISTORY) by default).
    pub fn history(mut self, jobs: usize) -> Self {
        self.history = Some(jobs);
        self
    }
}

fn spec(request: Request) -> CommandSpec {
    let (title, description) = match request {
        Request::Cancel => (
            "Cancel a job",
            "Asks the host to cancel the job `id`; it shows as cancelling until the host says it is cancelled.",
        ),
        Request::Pause => ("Pause a job", "Asks the host to pause the job `id`."),
        Request::Resume => (
            "Resume a job",
            "Asks the host to resume the paused job `id`.",
        ),
        Request::Retry => (
            "Retry a job",
            "Asks the host to run the failed or cancelled job `id` again.",
        ),
    };
    CommandSpec::new(command_of(request), title, "Jobs")
        .description(description)
        .args(json!({ "id": "export-1" }))
        .hidden()
}

/// What the status bar's jobs item says: how many go on (with their
/// overall estimate), and how many failed.
pub fn status_text(ctx: &egui::Context, summary: &Summary) -> (String, Option<String>) {
    let count = summary.active.to_string();
    let active = match summary.fraction {
        Some(f) => strings::tr_args(
            ctx,
            &txt::JOBS_ACTIVE_PERCENT,
            &[
                ("count", &count),
                ("percent", &strings::number(ctx, (f * 100.0).round(), 0)),
            ],
        ),
        None => strings::tr_args(ctx, &txt::JOBS_ACTIVE, &[("count", &count)]),
    };
    let failed = (summary.failed > 0).then(|| {
        strings::tr_args(
            ctx,
            &txt::JOBS_FAILED,
            &[("count", &summary.failed.to_string())],
        )
    });
    (active, failed)
}

/// The status bar's jobs item: a ring with the jobs' overall estimate
/// (turning while one cannot estimate), how many go on and how many
/// failed; nothing when there is nothing to say.
fn status_item(ui: &mut Ui, app: &AppHandle) {
    let Some(jobs) = app.service(&JOBS) else {
        return;
    };
    let summary = jobs.summary();
    if summary.active == 0 && summary.failed == 0 {
        return;
    }
    let ctx = ui.ctx().clone();
    let t = Tokens::of(&ctx);
    let (active, failed) = status_text(&ctx, &summary);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = t.space.s;
        if summary.active > 0 {
            // The ring is drawn only: the text names the item.
            let side = t.size.icon_inline;
            let (rect, _) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
            paint_ring(
                ui,
                rect,
                summary.fraction.map(|f| f as f32),
                Tone::Accent.color(&ctx),
            );
            ui.label(egui::RichText::new(&active).small());
        }
        if let Some(failed) = failed {
            ui.label(egui::RichText::new(failed).small().color(t.status.danger));
        }
    });
}

/// Whether a plugin hooks `jobs.estimate`.
fn estimate_hooked(app: &AppHandle) -> bool {
    app.steps()
        .iter()
        .any(|s| s.spec.id == STEP_ESTIMATE && !s.hooks.is_empty())
}

/// Runs `jobs.estimate` for the jobs that moved, and keeps what the
/// hooks say.
fn refine(app: &AppHandle, jobs: &JobsService, moved: &BTreeSet<String>) {
    let now = jobs.now();
    for id in moved {
        let Some(job) = jobs.job(id).filter(Job::active) else {
            continue;
        };
        let own = jobs.with(|j| j.own_fraction(id));
        let args = json!({
            "id": job.id,
            "title": job.title,
            "kind": job.kind,
            "fraction": own,
            "done": job.done,
            "total": job.total,
            "steps": job.steps,
            "step": job.step,
            "elapsed": job.elapsed(now),
        });
        let out = app.step(&STEP_ESTIMATE, args, |_, args| {
            Ok(json!({ "fraction": args["fraction"] }))
        });
        if let Some(Ok(v)) = out.result() {
            // The model's own given back is no refinement.
            let refined = v["fraction"].as_f64().filter(|f| Some(*f) != own);
            if refined != job.refined {
                jobs.refine(id, refined);
            }
        }
    }
    // What the refinement changed is not news.
    let _ = lock(&jobs.state).jobs.take_changed();
}

impl Plugin for JobsPlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("egui_pop.jobs")
    }

    fn title(&self) -> String {
        "Jobs".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let mut model = Jobs::new();
        if let Some(n) = self.log_lines {
            model = model.with_log_lines(n);
        }
        if let Some(n) = self.history {
            model = model.with_history(n);
        }
        ctx.service(&JOBS, JobsService::new(model, self.clock.clone()))?;
        for request in Request::ALL {
            match &self.runner {
                Some(run) => {
                    let run = run.clone();
                    ctx.command(spec(request), move |call| {
                        let id = call.args["id"]
                            .as_str()
                            .ok_or_else(|| "`id` is missing".to_string())?;
                        run(call.app, request, id)
                    })?;
                }
                None => {
                    ctx.declare(spec(request))?;
                }
            }
        }
        ctx.step(
            StepSpec::new(
                STEP_ESTIMATE,
                "A job's estimated fraction, refined by the application (run only when hooked).",
            )
            .args(json!({ "id": "export-1", "title": "Export", "kind": "export", "fraction": 0.25, "done": 2, "total": 8, "steps": [], "step": null, "elapsed": 4.0 })),
        )?;
        ctx.on(On::Pass, |app, _| {
            let Some(jobs) = app.service(&JOBS) else {
                return;
            };
            let moved = jobs.feed(app);
            if !moved.is_empty() && estimate_hooked(app) {
                refine(app, &jobs, &moved);
            }
        });
        let weak = ctx.app().downgrade();
        let w = weak.clone();
        let mut collector = FnCollector::new(
            "jobs",
            "The host's jobs folded from its `job.*` events: state, estimated fraction, counts, time left, the end of each log; `idle` says whether any goes on. Actions ask the host (`cancel`, `pause`, `resume`, `retry`, with `id`) or forget finished ones (`dismiss`, `clear`).",
        )
        .snapshot(move |_| {
            w.get()
                .ok()
                .and_then(|app| app.service(&JOBS))
                .map_or(Json::Null, |j| j.describe())
        });
        for request in Request::ALL {
            let w = weak.clone();
            let name = match request {
                Request::Cancel => "cancel",
                Request::Pause => "pause",
                Request::Resume => "resume",
                Request::Retry => "retry",
            };
            collector = collector
                .action(
                    name,
                    "Asks the host about the job `id`, as its button does.",
                    true,
                    move |_, args| {
                        let app = w.get()?;
                        let id = args["id"].as_str().ok_or("`id` is missing")?;
                        if app.service(&JOBS).and_then(|j| j.job(id)).is_none() {
                            return Err(format!("no job `{id}`"));
                        }
                        ask(&app, id, request);
                        Ok(json!({ "asked": command_of(request), "id": id }))
                    },
                )
                .args(json!({ "id": "export-1" }));
        }
        let w = weak.clone();
        collector = collector
            .action(
                "dismiss",
                "Forgets a finished job (`id`).",
                true,
                move |_, args| {
                    let app = w.get()?;
                    let id = args["id"].as_str().ok_or("`id` is missing")?;
                    let jobs = app.service(&JOBS).ok_or("no jobs")?;
                    if jobs.dismiss(id) {
                        Ok(json!({ "dismissed": id }))
                    } else {
                        Err(format!("no finished job `{id}`"))
                    }
                },
            )
            .args(json!({ "id": "export-1" }));
        let w = weak;
        collector = collector.action("clear", "Forgets every finished job.", true, move |_, _| {
            let app = w.get()?;
            app.service(&JOBS).ok_or("no jobs")?.clear_finished();
            Ok(Json::Null)
        });
        ctx.collector(collector)?;
        ctx.block(
            zones::STATUS_BAR,
            Block::add(
                BlockId::from_static("egui_pop.jobs.status"),
                900,
                status_item,
            ),
        );
        Ok(())
    }
}
