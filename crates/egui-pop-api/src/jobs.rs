//! Jobs, as a plugin ([`JobsPlugin`], opt-in): the host's background
//! work, followed on the bus ([ADR 0029](crate::bus)). The host publishes
//! `job.*` events (see [`egui_pop_model::jobs`] for their data); the bus
//! delivers each once, in journal order, to the listeners of `pop.job.*`
//! (each pass, and before a read that must see them all), and the
//! [`Jobs`] model behind the [`JOBS`] service is one of them
//! ([`MODEL`], `pop.jobs.model`): no job state changes but on a bus event or
//! request. Nothing polls: without events, nothing is read.
//!
//! <div class="warning">Experimental: may change in any release
//! (docs/stability.md).</div>
//!
//! - **Every job carries an estimated overall fraction**
//!   ([`Job::fraction`]): sub-steps and child jobs roll up, weighted by
//!   their estimates; a job that cannot estimate is indeterminate until
//!   it can. The `jobs.estimate` step (ADR 0011) lets an application
//!   refine it: a run-hook or an after-hook sees the job and gives
//!   `{ "fraction": 0.4 }` (or `null`: the model's own). It runs only
//!   when a plugin hooks it, once per pass for each job that moved, so
//!   a job's progress does not double the journal.
//! - Cancel, pause, resume and retry are requests on the bus
//!   (`pop.job.cancel`, `pop.job.pause`, `pop.job.resume`, `pop.job.retry`,
//!   [`AppHandle::request_job`]; [`ask`] asks one): [`MODEL`] marks the
//!   job as waiting for the answer, then [`HOST`] runs its command (each
//!   `{ "id" }`), declared for the host to run, or run in-process by a
//!   runner the application gives ([`JobsPlugin::run_locally`]). The
//!   answer is the runner's `job.*` events.
//! - A job belongs to a plugin when its `job.started` names it (`owner`,
//!   or `command`, a command of the plugin), or when the plugin
//!   published it itself. When that plugin is disconnected (ADR 0028),
//!   [`CANCEL_ON_DISCONNECT`] (a listener of `pop.plugin.disconnect`, run
//!   before `plugin.disconnected` is said) asks the host to cancel its
//!   jobs that can be (`job.cancel` with `{ "id", "reason" }`, from
//!   `egui_pop`; the job
//!   keeps the reason, [`Job::reason`]) and publishes `job.orphaned` for
//!   the others, which go on, marked ([`Job::orphaned`]: the jobs centre
//!   says "its plugin is off") until they end or the plugin is back
//!   (`job.adopted`, said by [`ADOPT_ON_CONNECT`], a listener of
//!   `pop.plugin.connect`). `plugin.disconnected` counts both
//!   (`jobs_cancelled`, `jobs_orphaned`). An orphaned job is not waited
//!   on: [`JobsService::idle`] leaves it out (nothing owns it, it may
//!   never end) and [`JobsService::orphans`] lists it.
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
use egui_pop_core::strings::{self, Arg};
use egui_pop_core::theme::Tokens;
use egui_pop_core::ui::UiPop as _;
use egui_pop_model::clock::SharedClock;
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::{BlockId, CommandId, PluginId, StepId};
use egui_pop_model::jobs::{Incoming, Job, Jobs, Left, Request, Summary};
use egui_pop_model::panels::{Area, PanelSpec};
use serde_json::{Value as Json, json};

use crate::app::AppHandle;
use crate::bus::{
    self, JobAsk, JobCancel, JobEvents, JobPause, JobResume, JobRetry, PluginConnect,
    PluginDisconnect,
};
use crate::hooks::On;
use crate::host::Event;
use crate::services::ServiceToken;
use crate::steps::StepSpec;
use crate::strings as txt;
use crate::zones::{self, Block};
use crate::{Ctx, Plugin, PluginError};
use bus_contract::{Bus, ListenOpts, Point};

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

/// The jobs model's listener: of `pop.job.*` (it folds each event), and of
/// the job requests (the job shows as asked).
pub const MODEL: &str = "pop.jobs.model";
/// Runs a job request's command, after [`MODEL`]: the host (or the
/// application's runner) answers with its events.
pub const HOST: &str = "pop.jobs.host";
/// Of `pop.plugin.disconnect`: the jobs a plugin leaves are cancelled, or
/// said orphaned.
pub const CANCEL_ON_DISCONNECT: &str = "pop.jobs.cancel-on-disconnect";
/// Of `pop.plugin.connect`: the jobs a plugin had left are its own again
/// (`job.adopted`).
pub const ADOPT_ON_CONNECT: &str = "pop.jobs.adopt-on-connect";

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
    /// The motion clock's time at the last event or pass.
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

    /// Whether no job that something still owns goes on; orphaned jobs
    /// are not waited on ([`Jobs::idle`]).
    pub fn idle(&self) -> bool {
        self.with(Jobs::idle)
    }

    /// The orphaned jobs still going on ([`Jobs::orphans`]).
    pub fn orphans(&self) -> Vec<Job> {
        self.with(|j| j.orphans().into_iter().cloned().collect())
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
    /// model: the `jobs.estimate` step's answer.
    pub(crate) fn refine(&self, id: &str, fraction: Option<f64>) {
        let now = self.now();
        lock(&self.state).jobs.refine(id, fraction, now);
    }

    /// Notes that `request` was asked for a job ([`MODEL`] on a job
    /// request); whether the job is known.
    pub(crate) fn mark(&self, id: &str, request: Request) -> bool {
        let mut s = lock(&self.state);
        if s.jobs.get(id).is_none() {
            return false;
        }
        s.jobs.request(id, request);
        true
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

    /// A pass: the motion clock's time kept; the jobs that moved since
    /// the last one.
    fn pass(&self, app: &AppHandle) -> BTreeSet<String> {
        let time = egui_pop_core::motion::now(&app.egui());
        let mut s = lock(&self.state);
        s.now = time;
        s.jobs.take_changed()
    }

    /// Folds one event the bus delivered ([`MODEL`] on `pop.job.*`).
    fn follow(&self, app: &AppHandle, e: &Event) {
        let time = egui_pop_core::motion::now(&app.egui());
        lock(&self.state).now = time;
        let now = self.now();
        let incoming = Incoming {
            seq: e.seq,
            source: &e.source,
            cause: e.cause,
            name: &e.name,
            data: &e.data,
        };
        lock(&self.state).jobs.apply(&incoming, now);
    }
}

/// The journal's source of what the framework itself asks or says.
const FRAMEWORK: &str = "egui_pop";

/// Does what [`Jobs::left_by`] found, on the bus alone (the model
/// follows from there, as for any job event): `job.cancel` is asked for
/// the jobs that can be, with the reason, and the others are published
/// as orphaned.
fn owner_gone(app: &AppHandle, plugin: &str, left: &Left) {
    if left.is_empty() {
        return;
    }
    let reason = format!("its plugin `{plugin}` is disconnected");
    for id in &left.cancel {
        log::info!(target: "egui_pop", "job `{id}`: the host is asked to cancel it: {reason}");
        // The framework's own request, whoever asked for the plugin to go.
        app.request_job(
            JobAsk::new(id.clone(), Request::Cancel)
                .reason(reason.clone())
                .by_framework(),
        );
    }
    for id in &left.orphan {
        log::info!(target: "egui_pop", "job `{id}` is orphaned: {reason}");
        app.publish(
            FRAMEWORK,
            None,
            egui_pop_model::jobs::ORPHANED,
            json!({ "id": id, "plugin": plugin, "reason": reason }),
        );
    }
}

/// Asks the host to cancel, pause, resume or retry a job, on the bus
/// ([`AppHandle::request_job`]): the job shows as waiting for the
/// answer, and its command runs for whoever acts now.
pub fn ask(app: &AppHandle, id: &str, request: Request) {
    app.request_job(JobAsk::new(id, request));
}

/// Registers `request`'s listeners: [`MODEL`] marks the job, then
/// [`HOST`] runs its command.
fn listen_request<P: Point<Input = JobAsk, Value = Vec<String>>>(
    kernel: &dyn Bus,
    owner: &str,
) -> Vec<Result<(), bus_contract::Refused>> {
    let mine = || ListenOpts::new().owner(owner);
    vec![
        kernel.listen::<P>(MODEL, mine().before(HOST), |ask, did| {
            let Some(app) = bus::asking() else {
                return Ok(());
            };
            if app
                .service(&JOBS)
                .is_some_and(|j| j.mark(&ask.id, ask.request))
            {
                did.push(format!("{MODEL}: `{}` waits for the answer", ask.id));
            }
            Ok(())
        }),
        kernel.listen::<P>(HOST, mine().after(MODEL), |ask, did| {
            let Some(app) = bus::asking() else {
                return Ok(());
            };
            let command = command_of(ask.request);
            if ask.is_framework() {
                // Queued as the framework's own request.
                app.push_request(crate::app::Request::Run {
                    command: command.clone(),
                    args: ask.args(),
                    origin: Origin::Script(FRAMEWORK.into()),
                });
            } else {
                app.run(command.clone(), ask.args());
            }
            did.push(format!("{HOST}: `{command}` asked"));
            Ok(())
        }),
    ]
}

/// Registers the jobs plugin's listeners on the bus, replacing those of
/// an earlier connection.
fn listen(app: &AppHandle, owner: &str) {
    let kernel = app.bus();
    for point in bus::JOB_POINTS.iter().chain(&bus::PLUGIN_POINTS) {
        for id in [MODEL, HOST, CANCEL_ON_DISCONNECT, ADOPT_ON_CONNECT] {
            kernel.unlisten(point, id);
        }
    }
    let mine = || ListenOpts::new().owner(owner);
    let mut listened = Vec::new();
    listened.extend(listen_request::<JobCancel>(&*kernel, owner));
    listened.extend(listen_request::<JobPause>(&*kernel, owner));
    listened.extend(listen_request::<JobResume>(&*kernel, owner));
    listened.extend(listen_request::<JobRetry>(&*kernel, owner));
    listened.push(kernel.listen::<JobEvents>(MODEL, mine(), |e, _| {
        if let Some(app) = bus::asking()
            && let Some(jobs) = app.service(&JOBS)
        {
            jobs.follow(&app, e);
        }
        Ok(())
    }));
    listened.push(kernel.listen::<PluginDisconnect>(
        CANCEL_ON_DISCONNECT,
        mine(),
        |change, counts| {
            let Some(app) = bus::asking() else {
                return Ok(());
            };
            let Some(jobs) = app.service(&JOBS) else {
                return Ok(());
            };
            let commands: Vec<&str> = change.commands.iter().map(String::as_str).collect();
            let left = jobs
                .with(|j| j.left_by(&[change.plugin.as_str(), change.owner.as_str()], &commands));
            owner_gone(&app, &change.plugin, &left);
            // The model follows what was said, in this call.
            app.deliver_events();
            counts.insert("jobs_cancelled".into(), json!(left.cancel.len()));
            counts.insert("jobs_orphaned".into(), json!(left.orphan.len()));
            Ok(())
        },
    ));
    listened.push(
        kernel.listen::<PluginConnect>(ADOPT_ON_CONNECT, mine(), |change, did| {
            let Some(app) = bus::asking() else {
                return Ok(());
            };
            let Some(jobs) = app.service(&JOBS) else {
                return Ok(());
            };
            let back = jobs.with(|j| j.orphaned_by(&change.plugin));
            if !back.is_empty() {
                app.publish(
                    FRAMEWORK,
                    None,
                    egui_pop_model::jobs::ADOPTED,
                    json!({ "plugin": change.plugin, "jobs": back }),
                );
                app.deliver_events();
                did.push(format!(
                    "{ADOPT_ON_CONNECT}: {} job(s) its own again",
                    back.len()
                ));
            }
            Ok(())
        }),
    );
    for l in listened {
        if let Err(e) = l {
            log::error!(target: "egui_pop", "jobs: {e}");
        }
    }
}

/// Runs a job command in-process: the application's own job runner.
pub type Runner = Rc<dyn Fn(&AppHandle, Request, &str) -> Result<Json, String>>;

/// The plugin.
#[derive(Clone)]
pub struct JobsPlugin {
    clock: Option<SharedClock>,
    runner: Option<Runner>,
    log_lines: Option<usize>,
    history: Option<usize>,
    panel: bool,
}

impl Default for JobsPlugin {
    fn default() -> Self {
        Self {
            clock: None,
            runner: None,
            log_lines: None,
            history: None,
            panel: true,
        }
    }
}

impl fmt::Debug for JobsPlugin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JobsPlugin")
            .field("clock", &self.clock.is_some())
            .field("runner", &self.runner.is_some())
            .field("log_lines", &self.log_lines)
            .field("history", &self.history)
            .field("panel", &self.panel)
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

    /// Without the jobs panel (the status bar's popover stays).
    pub fn without_panel(mut self) -> Self {
        self.panel = false;
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
    let count = Arg::from(summary.active);
    let active = match summary.fraction {
        Some(f) => strings::tr_with(
            ctx,
            &txt::JOBS_ACTIVE_PERCENT,
            &[
                ("count", count),
                (
                    "percent",
                    Arg::Str(&strings::number(ctx, (f * 100.0).round(), 0)),
                ),
            ],
        ),
        None => strings::tr_with(ctx, &txt::JOBS_ACTIVE, &[("count", count)]),
    };
    let failed = (summary.failed > 0)
        .then(|| strings::tr_with(ctx, &txt::JOBS_FAILED, &[("count", summary.failed.into())]));
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
        let mut text = active;
        if summary.active > 0 {
            // The ring is drawn only: the button's text names the item.
            let side = t.size.icon_inline;
            let (rect, _) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
            // Only orphaned jobs going on: nothing owns them and they may
            // never end, so the ring does not spin for them (a spinning
            // ring asks for a repaint each pass, and the application
            // would never settle).
            let owned = jobs.with(|j| j.roots().iter().any(|r| r.active() && r.orphaned.is_none()));
            let fraction = summary.fraction.map(|f| f as f32);
            paint_ring(
                ui,
                rect,
                if owned {
                    fraction
                } else {
                    Some(fraction.unwrap_or(0.0))
                },
                Tone::Accent.color(&ctx),
            );
        } else {
            text.clear();
        }
        let mut label = egui::text::LayoutJob::default();
        let small = egui::TextFormat {
            font_id: egui::FontId::proportional(t.font.small),
            color: t.text.default,
            ..Default::default()
        };
        label.append(&text, 0.0, small.clone());
        if let Some(failed) = &failed {
            if !text.is_empty() {
                label.append("  ", 0.0, small.clone());
            }
            label.append(
                failed,
                0.0,
                egui::TextFormat {
                    color: t.status.danger,
                    ..small
                },
            );
        }
        let response = ui.add(egui::Button::new(label).frame(false));
        crate::jobs_centre::popover(ui, app, &response);
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
        let service = JobsService::new(model, self.clock.clone());
        // Connected late: the job events said before, up to the last the
        // bus delivered, from the journal's record; the bus delivers the
        // rest.
        let app = ctx.app();
        let upto = app.delivered_event();
        if upto > 0 {
            for e in app
                .events_since(0)
                .iter()
                .filter(|e| e.seq <= upto && bus::is_job_event(e))
            {
                service.follow(&app, e);
            }
        }
        ctx.service(&JOBS, service)?;
        listen(&app, ctx.owner().as_str());
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
            let moved = jobs.pass(app);
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
        if self.panel {
            ctx.panel(
                PanelSpec::new(crate::jobs_centre::PANEL_JOBS, "Jobs", Area::Bottom)
                    .min_size(120.0)
                    .default_size(240.0)
                    .icon("history")
                    .closed(),
                |ui, app| {
                    ui.scroll_area(
                        egui::ScrollArea::vertical().auto_shrink([false, false]),
                        |ui| crate::jobs_centre::show(ui, app),
                    );
                },
            )?;
        }
        Ok(())
    }
}
