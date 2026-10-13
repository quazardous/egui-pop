//! Jobs: the application's background work, folded from the `job.*`
//! events its host publishes on the journal (ADR 0010). egui-pop never
//! runs jobs and never polls for them: the host publishes, [`Jobs`]
//! follows, and the views (bars, the status bar, the jobs centre) draw
//! what it holds.
//!
//! | Event | Data |
//! |---|---|
//! | `job.started` | `id`, `title`, `kind`, `parent`, `weight` (its share of its parent), `total` (none: indeterminate), `steps` (`[{id, title, weight}]` or names), `cancellable` (true), `pausable` (false), `retryable` (true) |
//! | `job.progress` | `id`, `done`, `total`, `failed`, `skipped`, `item` (the current item), `item_progress` (0 to 1), `phase`, `step` (the current step), `step_done`, `step_total`, `estimate` (the host's own fraction, 0 to 1) |
//! | `job.item_failed` | `id`, `item`, `error` (the job goes on) |
//! | `job.paused`, `job.resumed`, `job.cancelling` | `id` |
//! | `job.failed` | `id`, `error` |
//! | `job.finished` | `id`, `actions` (`[{title, command, args}]`) |
//! | `job.cancelled` | `id` |
//! | `job.log` | `id`, `text` (lines of the job's log) |
//!
//! A `job.started` for a job already known starts it again (a retry):
//! its progress is reset, its log kept, its finished children
//! forgotten.
//!
//! **Every job carries an estimated overall fraction** ([`Job::fraction`],
//! 0 to 1), or none while it cannot estimate (indeterminate):
//!
//! - a job with sub-steps (declared `steps`, and child jobs naming it as
//!   `parent`) rolls them up, each weighted by its estimate (`weight`,
//!   1 by default): finished steps count whole, the current one by its
//!   own progress, the others nothing. A parent with a `total` counts its
//!   children not started yet at weight 1 each;
//! - otherwise `done` of `total`, the current item's progress included;
//! - otherwise the host's `estimate`;
//! - otherwise none: the job is indeterminate until one of these comes,
//!   and once determinate it never goes back (its last estimate stays);
//! - [`Jobs::refine`] overrides it (the application's `jobs.estimate`
//!   hook, ADR 0011).
//!
//! Time left ([`Job::eta`]) comes from the rate of that fraction, an
//! exponential moving average over the times events arrived at, given
//! by an injected clock (`now`): it is not given before [`ETA_AFTER`]
//! seconds.
//!
//! Cancel, pause, resume and retry are commands the host runs
//! (`job.cancel`, `job.pause`, `job.resume`, `job.retry`, each with
//! `{ "id" }`): the model follows their `command.dispatched` and
//! `command.failed` events too, so a job shows "cancelling" from the
//! moment it was asked until the host says it is cancelled, and a
//! refused request says why.
//!
//! ```
//! use egui_pop_model::jobs::{Incoming, JobState, Jobs};
//! use serde_json::json;
//!
//! let mut jobs = Jobs::new();
//! let mut at = |seq, name, data: serde_json::Value, now| {
//!     jobs.apply(&Incoming::new(seq, name, &data), now);
//! };
//! at(1, "job.started", json!({ "id": "export", "title": "Export", "total": 4 }), 0.0);
//! at(2, "job.progress", json!({ "id": "export", "done": 1, "item": "a.raw" }), 1.0);
//! let job = jobs.get("export").unwrap();
//! assert_eq!(job.state, JobState::Running);
//! assert_eq!(job.fraction, Some(0.25));
//! assert_eq!(job.item.as_deref(), Some("a.raw"));
//! ```

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::Serialize;
use serde_json::{Value as Json, json};

/// A job started (or started again).
pub const STARTED: &str = "job.started";
/// A job moved on.
pub const PROGRESS: &str = "job.progress";
/// One item failed; the job goes on.
pub const ITEM_FAILED: &str = "job.item_failed";
pub const PAUSED: &str = "job.paused";
pub const RESUMED: &str = "job.resumed";
/// Cancelling was accepted; the job winds down.
pub const CANCELLING: &str = "job.cancelling";
pub const FAILED: &str = "job.failed";
pub const FINISHED: &str = "job.finished";
pub const CANCELLED: &str = "job.cancelled";
/// Lines of a job's log.
pub const LOG: &str = "job.log";

/// The commands the host runs on a job, each with `{ "id" }`.
pub const CMD_CANCEL: &str = "job.cancel";
pub const CMD_PAUSE: &str = "job.pause";
pub const CMD_RESUME: &str = "job.resume";
pub const CMD_RETRY: &str = "job.retry";

/// Time left is not given before a job ran this long (seconds).
pub const ETA_AFTER: f64 = 3.0;
/// How many lines of a job's log are kept by default.
pub const LOG_LINES: usize = 5000;
/// How many finished jobs are kept by default.
pub const HISTORY: usize = 100;
/// How many failed items a job keeps.
pub const ITEM_ERRORS: usize = 100;

/// How quickly the rate follows the latest events (0 to 1).
const ALPHA: f64 = 0.3;
/// How deep jobs may nest (a cycle of parents stops there).
const DEPTH: usize = 16;

/// Where a job is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    #[default]
    Running,
    Paused,
    /// Cancelling was accepted: the job winds down.
    Cancelling,
    Done,
    Failed,
    Cancelled,
}

impl JobState {
    /// Whether the job is still going (running, paused, cancelling).
    pub fn active(self) -> bool {
        matches!(
            self,
            JobState::Running | JobState::Paused | JobState::Cancelling
        )
    }

    pub fn name(self) -> &'static str {
        match self {
            JobState::Running => "running",
            JobState::Paused => "paused",
            JobState::Cancelling => "cancelling",
            JobState::Done => "done",
            JobState::Failed => "failed",
            JobState::Cancelled => "cancelled",
        }
    }
}

/// Something asked of the host about a job, waiting for its answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Request {
    Cancel,
    Pause,
    Resume,
    Retry,
}

impl Request {
    pub const ALL: [Request; 4] = [
        Request::Cancel,
        Request::Pause,
        Request::Resume,
        Request::Retry,
    ];

    /// The command that asks it.
    pub fn command(self) -> &'static str {
        match self {
            Request::Cancel => CMD_CANCEL,
            Request::Pause => CMD_PAUSE,
            Request::Resume => CMD_RESUME,
            Request::Retry => CMD_RETRY,
        }
    }

    pub fn from_command(command: &str) -> Option<Request> {
        Request::ALL.into_iter().find(|r| r.command() == command)
    }
}

/// A declared sub-step of a job.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Step {
    pub id: String,
    pub title: String,
    /// Its share of the job, against the other steps' (its estimate).
    pub weight: f64,
    pub done: u64,
    pub total: Option<u64>,
    pub started: bool,
    pub finished: bool,
}

impl Step {
    /// How far it is (0 to 1); none while it runs without a total.
    pub fn fraction(&self) -> Option<f64> {
        if self.finished {
            return Some(1.0);
        }
        if !self.started {
            return Some(0.0);
        }
        match self.total {
            Some(t) if t > 0 => Some((self.done as f64 / t as f64).clamp(0.0, 1.0)),
            _ => None,
        }
    }
}

/// An item that failed.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ItemError {
    pub item: String,
    pub error: String,
}

/// A command a finished job offers ("Show in folder").
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct JobAction {
    pub title: String,
    pub command: String,
    pub args: Json,
}

/// A job, as the events made it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Job {
    pub id: String,
    pub title: String,
    /// What kind of work (an icon role: `export`, `import`, `render`).
    pub kind: String,
    /// Who published it (the journal's source).
    pub source: String,
    pub parent: Option<String>,
    /// Its share of its parent.
    pub weight: f64,
    pub state: JobState,
    pub done: u64,
    pub total: Option<u64>,
    pub failed: u64,
    pub skipped: u64,
    /// The item being worked on, and how far it is (0 to 1).
    pub item: Option<String>,
    pub item_progress: Option<f64>,
    pub phase: Option<String>,
    pub steps: Vec<Step>,
    /// The current step's id.
    pub step: Option<String>,
    /// The host's own estimate (0 to 1).
    pub hint: Option<f64>,
    /// The application's estimate ([`Jobs::refine`]), overriding the
    /// model's.
    pub refined: Option<f64>,
    /// The estimated overall fraction (0 to 1); none: indeterminate.
    pub fraction: Option<f64>,
    pub cancellable: bool,
    pub pausable: bool,
    pub retryable: bool,
    pub error: Option<String>,
    /// The last failed items, oldest first.
    pub errors: Vec<ItemError>,
    pub actions: Vec<JobAction>,
    /// What was asked of the host, until it answers.
    pub requested: Option<Request>,
    /// Why the host refused the last request.
    pub request_error: Option<String>,
    pub started_at: f64,
    pub updated_at: f64,
    pub ended_at: Option<f64>,
    /// How many times it started (1, then retries).
    pub attempts: u32,
    /// The last event that changed it.
    pub seq: u64,
    /// Lines ever written to its log.
    pub log_total: u64,
    /// Its place in the list (the order jobs first started in).
    pub serial: u64,
    #[serde(skip)]
    rate: Option<f64>,
    #[serde(skip)]
    sample: Option<(f64, f64)>,
}

impl Job {
    fn new(id: &str, now: f64, serial: u64) -> Job {
        Job {
            id: id.into(),
            title: id.into(),
            kind: String::new(),
            source: String::new(),
            parent: None,
            weight: 1.0,
            state: JobState::Running,
            done: 0,
            total: None,
            failed: 0,
            skipped: 0,
            item: None,
            item_progress: None,
            phase: None,
            steps: Vec::new(),
            step: None,
            hint: None,
            refined: None,
            fraction: None,
            cancellable: true,
            pausable: false,
            retryable: true,
            error: None,
            errors: Vec::new(),
            actions: Vec::new(),
            requested: None,
            request_error: None,
            started_at: now,
            updated_at: now,
            ended_at: None,
            attempts: 1,
            seq: 0,
            log_total: 0,
            serial,
            rate: None,
            sample: None,
        }
    }

    /// Whether it still goes on.
    pub fn active(&self) -> bool {
        self.state.active()
    }

    /// Whether its fraction is known.
    pub fn determinate(&self) -> bool {
        self.fraction.is_some()
    }

    /// The estimated overall percentage (0 to 100); none: indeterminate.
    pub fn percent(&self) -> Option<f64> {
        self.fraction.map(|f| f * 100.0)
    }

    /// How long it ran, until now or its end.
    pub fn elapsed(&self, now: f64) -> f64 {
        (self.ended_at.unwrap_or(now) - self.started_at).max(0.0)
    }

    /// The seconds left, from the rate of its fraction: none before
    /// [`ETA_AFTER`] seconds, while indeterminate, paused or ended.
    pub fn eta(&self, now: f64) -> Option<f64> {
        if self.state != JobState::Running || self.elapsed(now) < ETA_AFTER {
            return None;
        }
        let f = self.fraction?;
        let rate = self.rate.filter(|r| *r > 1e-9)?;
        Some(((1.0 - f) / rate).max(0.0))
    }

    fn step_index(&self, id: &str) -> Option<usize> {
        self.steps.iter().position(|s| s.id == id)
    }

    /// Starts again (a retry): progress reset, log kept.
    fn restart(&mut self, now: f64) {
        let fresh = Job::new(&self.id, now, self.serial);
        let (log_total, attempts) = (self.log_total, self.attempts + 1);
        *self = Job {
            log_total,
            attempts,
            ..fresh
        };
    }

    fn sample(&mut self, now: f64) {
        let Some(f) = self.fraction else {
            return;
        };
        // Progress starts from nothing when the job starts.
        let (t0, f0) = *self.sample.get_or_insert((self.started_at, 0.0));
        if now - t0 > 1e-3 {
            let inst = (f - f0) / (now - t0);
            if inst >= 0.0 {
                self.rate = Some(self.rate.map_or(inst, |r| r + ALPHA * (inst - r)));
            }
            self.sample = Some((now, f));
        }
    }
}

/// An event as the model reads it.
#[derive(Clone, Copy, Debug)]
pub struct Incoming<'a> {
    pub seq: u64,
    pub source: &'a str,
    pub cause: Option<u64>,
    pub name: &'a str,
    pub data: &'a Json,
}

impl<'a> Incoming<'a> {
    pub fn new(seq: u64, name: &'a str, data: &'a Json) -> Self {
        Self {
            seq,
            source: "",
            cause: None,
            name,
            data,
        }
    }

    pub fn source(mut self, source: &'a str) -> Self {
        self.source = source;
        self
    }

    pub fn cause(mut self, cause: Option<u64>) -> Self {
        self.cause = cause;
        self
    }
}

/// All jobs at a glance.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Summary {
    /// Jobs going on (top-level ones).
    pub active: usize,
    /// Jobs that failed and were not dismissed.
    pub failed: usize,
    /// The mean fraction of the active jobs; none when one of them is
    /// indeterminate (or none is active).
    pub fraction: Option<f64>,
}

/// The jobs, folded from the journal.
#[derive(Clone, Debug)]
pub struct Jobs {
    jobs: BTreeMap<String, Job>,
    /// Each job's log, the last lines kept.
    logs: BTreeMap<String, VecDeque<String>>,
    /// Job commands dispatched, by their `command.dispatched` sequence
    /// number: which job, what was asked.
    dispatched: BTreeMap<u64, (String, Request)>,
    serial: u64,
    log_lines: usize,
    history: usize,
    changed: BTreeSet<String>,
}

impl Default for Jobs {
    fn default() -> Self {
        Self::new()
    }
}

fn text(data: &Json, key: &str) -> Option<String> {
    match &data[key] {
        Json::String(s) => Some(s.clone()),
        Json::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn count(data: &Json, key: &str) -> Option<u64> {
    data[key]
        .as_u64()
        .or_else(|| data[key].as_f64().map(|f| f.max(0.0) as u64))
}

fn fraction(data: &Json, key: &str) -> Option<f64> {
    data[key].as_f64().map(|f| f.clamp(0.0, 1.0))
}

fn flag(data: &Json, key: &str, default: bool) -> bool {
    data[key].as_bool().unwrap_or(default)
}

impl Jobs {
    pub fn new() -> Self {
        Self {
            jobs: BTreeMap::new(),
            logs: BTreeMap::new(),
            dispatched: BTreeMap::new(),
            serial: 0,
            log_lines: LOG_LINES,
            history: HISTORY,
            changed: BTreeSet::new(),
        }
    }

    /// Keeps `lines` lines of each job's log.
    pub fn with_log_lines(mut self, lines: usize) -> Self {
        self.log_lines = lines.max(1);
        self
    }

    /// Keeps `jobs` finished top-level jobs.
    pub fn with_history(mut self, jobs: usize) -> Self {
        self.history = jobs;
        self
    }

    /// Folds an event at time `now`; whether it changed a job. Events
    /// other than `job.*` and the job commands' are ignored.
    pub fn apply(&mut self, e: &Incoming<'_>, now: f64) -> bool {
        if let Some(rest) = e.name.strip_prefix("command.") {
            return self.command(rest, e);
        }
        if !e.name.starts_with("job.") {
            return false;
        }
        let d = e.data;
        let Some(id) = text(d, "id") else {
            return false;
        };
        if e.name == STARTED {
            self.start(&id, e, now);
        } else if !self.jobs.contains_key(&id) {
            // A host that skipped `job.started`: the job is made up.
            self.serial += 1;
            let mut job = Job::new(&id, now, self.serial);
            job.source = e.source.into();
            self.jobs.insert(id.clone(), job);
        }
        let job = self.jobs.get_mut(&id).expect("just made");
        job.seq = e.seq;
        job.updated_at = now;
        match e.name {
            STARTED => {}
            PROGRESS => {
                if let Some(n) = count(d, "total") {
                    job.total = Some(n);
                }
                if let Some(n) = count(d, "done") {
                    job.done = n;
                }
                if let Some(n) = count(d, "failed") {
                    job.failed = n;
                }
                if let Some(n) = count(d, "skipped") {
                    job.skipped = n;
                }
                if let Some(item) = text(d, "item") {
                    if job.item.as_ref() != Some(&item) {
                        job.item_progress = None;
                    }
                    job.item = Some(item);
                }
                if !d["item_progress"].is_null() {
                    job.item_progress = fraction(d, "item_progress");
                }
                if let Some(p) = text(d, "phase") {
                    job.phase = Some(p);
                }
                if let Some(h) = fraction(d, "estimate") {
                    job.hint = Some(h);
                }
                if let Some(step) = text(d, "step") {
                    let at = match job.step_index(&step) {
                        Some(i) => i,
                        None => {
                            // A step nobody declared: added, at weight 1.
                            job.steps.push(Step {
                                id: step.clone(),
                                title: step.clone(),
                                weight: 1.0,
                                done: 0,
                                total: None,
                                started: false,
                                finished: false,
                            });
                            job.steps.len() - 1
                        }
                    };
                    for s in &mut job.steps[..at] {
                        s.started = true;
                        s.finished = true;
                    }
                    let s = &mut job.steps[at];
                    s.started = true;
                    if let Some(n) = count(d, "step_total") {
                        s.total = Some(n);
                    }
                    if let Some(n) = count(d, "step_done") {
                        s.done = n;
                    }
                    s.finished = s.total.is_some_and(|t| s.done >= t);
                    job.step = Some(step);
                }
                if job.state == JobState::Paused {
                    job.state = JobState::Running;
                }
            }
            ITEM_FAILED => {
                job.failed += 1;
                job.errors.push(ItemError {
                    item: text(d, "item").unwrap_or_default(),
                    error: text(d, "error").unwrap_or_default(),
                });
                let len = job.errors.len();
                if len > ITEM_ERRORS {
                    job.errors.drain(..len - ITEM_ERRORS);
                }
            }
            PAUSED => {
                job.state = JobState::Paused;
                job.requested = None;
            }
            RESUMED => {
                job.state = JobState::Running;
                job.requested = None;
                // The pause is not part of the rate.
                job.sample = job.fraction.map(|f| (now, f));
            }
            CANCELLING => {
                job.state = JobState::Cancelling;
                job.requested = None;
            }
            FAILED => {
                job.state = JobState::Failed;
                job.error = text(d, "error").or_else(|| Some(String::new()));
                job.ended_at = Some(now);
                job.requested = None;
            }
            FINISHED => {
                job.state = match d["outcome"].as_str() {
                    Some("failed") => JobState::Failed,
                    Some("cancelled") => JobState::Cancelled,
                    _ => JobState::Done,
                };
                if let Some(err) = text(d, "error") {
                    job.error = Some(err);
                }
                if let Some(n) = count(d, "failed") {
                    job.failed = n;
                }
                if let Some(n) = count(d, "skipped") {
                    job.skipped = n;
                }
                if let Some(list) = d["actions"].as_array() {
                    job.actions = list
                        .iter()
                        .filter_map(|a| {
                            Some(JobAction {
                                title: text(a, "title")?,
                                command: text(a, "command")?,
                                args: a["args"].clone(),
                            })
                        })
                        .collect();
                }
                if job.state == JobState::Done {
                    for s in &mut job.steps {
                        s.started = true;
                        s.finished = true;
                    }
                    if let Some(t) = job.total {
                        job.done = job.done.max(t);
                    }
                }
                job.item = None;
                job.item_progress = None;
                job.ended_at = Some(now);
                job.requested = None;
            }
            CANCELLED => {
                job.state = JobState::Cancelled;
                job.ended_at = Some(now);
                job.requested = None;
            }
            LOG => {
                let text = text(d, "text").unwrap_or_default();
                let text = text.strip_suffix('\n').unwrap_or(&text);
                let log = self.logs.entry(id.clone()).or_default();
                for line in text.split('\n') {
                    log.push_back(line.strip_suffix('\r').unwrap_or(line).into());
                    job.log_total += 1;
                }
                while log.len() > self.log_lines {
                    log.pop_front();
                }
                self.changed.insert(id);
                return true;
            }
            _ => return false,
        }
        self.recompute(&id, now);
        if !self.jobs[&id].active() {
            self.trim_history();
        }
        true
    }

    fn start(&mut self, id: &str, e: &Incoming<'_>, now: f64) {
        let d = e.data;
        let serial = match self.jobs.get_mut(id) {
            Some(job) => {
                job.restart(now);
                let serial = job.serial;
                // Its finished children belong to the last attempt.
                let ended: Vec<String> = self
                    .jobs
                    .values()
                    .filter(|c| c.parent.as_deref() == Some(id) && !c.active())
                    .map(|c| c.id.clone())
                    .collect();
                for c in ended {
                    self.dismiss(&c);
                }
                serial
            }
            None => {
                self.serial += 1;
                self.jobs.insert(id.into(), Job::new(id, now, self.serial));
                self.serial
            }
        };
        let job = self.jobs.get_mut(id).expect("just made");
        job.serial = serial;
        job.title = text(d, "title").unwrap_or_else(|| id.into());
        job.kind = text(d, "kind").unwrap_or_default();
        job.source = text(d, "source").unwrap_or_else(|| e.source.into());
        job.parent = text(d, "parent").filter(|p| p != id);
        job.weight = d["weight"].as_f64().unwrap_or(1.0).max(0.0);
        job.total = count(d, "total");
        job.hint = fraction(d, "estimate");
        job.cancellable = flag(d, "cancellable", true);
        job.pausable = flag(d, "pausable", false);
        job.retryable = flag(d, "retryable", true);
        if let Some(steps) = d["steps"].as_array() {
            job.steps = steps
                .iter()
                .filter_map(|s| {
                    let (id, title, weight) = match s {
                        Json::String(name) => (name.clone(), name.clone(), 1.0),
                        _ => {
                            let id = text(s, "id")?;
                            let title = text(s, "title").unwrap_or_else(|| id.clone());
                            (id, title, s["weight"].as_f64().unwrap_or(1.0))
                        }
                    };
                    Some(Step {
                        id,
                        title,
                        weight: weight.max(0.0),
                        done: 0,
                        total: None,
                        started: false,
                        finished: false,
                    })
                })
                .collect();
        }
    }

    /// Follows the job commands: asked, refused.
    fn command(&mut self, what: &str, e: &Incoming<'_>) -> bool {
        match what {
            "dispatched" => {
                let Some(req) = e.data["command"].as_str().and_then(Request::from_command) else {
                    return false;
                };
                let Some(id) = text(&e.data["args"], "id") else {
                    return false;
                };
                let Some(job) = self.jobs.get_mut(&id) else {
                    return false;
                };
                job.requested = Some(req);
                job.request_error = None;
                self.dispatched.insert(e.seq, (id.clone(), req));
                self.changed.insert(id);
                true
            }
            "done" | "failed" => {
                let Some((id, req)) = e.cause.and_then(|c| self.dispatched.remove(&c)) else {
                    return false;
                };
                let Some(job) = self.jobs.get_mut(&id) else {
                    return false;
                };
                if what == "failed" {
                    job.request_error = Some(text(e.data, "error").unwrap_or_default());
                    if job.requested == Some(req) {
                        job.requested = None;
                    }
                } else if req == Request::Retry && job.requested == Some(req) {
                    // A retry may start a job of another id: the request
                    // is answered.
                    job.requested = None;
                }
                self.changed.insert(id);
                true
            }
            _ => false,
        }
    }

    /// Notes a request made before its command's event comes back (a
    /// view greys its button at once).
    pub fn request(&mut self, id: &str, request: Request) {
        if let Some(job) = self.jobs.get_mut(id) {
            job.requested = Some(request);
            job.request_error = None;
            self.changed.insert(id.into());
        }
    }

    /// Overrides a job's estimate (0 to 1), or gives it back to the
    /// model (`None`); its ancestors follow.
    pub fn refine(&mut self, id: &str, fraction: Option<f64>, now: f64) {
        let Some(job) = self.jobs.get_mut(id) else {
            return;
        };
        let fraction = fraction.map(|f| f.clamp(0.0, 1.0));
        if job.refined == fraction {
            return;
        }
        job.refined = fraction;
        self.recompute(id, now);
    }

    /// The model's own estimate of a job, before any refinement and
    /// without the last one kept: what the `jobs.estimate` step starts
    /// from.
    pub fn own_fraction(&self, id: &str) -> Option<f64> {
        self.jobs.get(id).and_then(|j| self.own(j))
    }

    /// A job's estimate: the application's, or the model's own.
    fn estimate(&self, job: &Job) -> Option<f64> {
        if job.state == JobState::Done {
            return Some(1.0);
        }
        job.refined.or_else(|| self.own(job))
    }

    /// A job's estimate from its parts, its counts or its host.
    fn own(&self, job: &Job) -> Option<f64> {
        if job.state == JobState::Done {
            return Some(1.0);
        }
        let mut parts: Vec<(f64, Option<f64>)> = job
            .steps
            .iter()
            .map(|s| (s.weight, s.fraction().filter(|_| s.started)))
            .collect();
        let mut known = job
            .steps
            .iter()
            .any(|s| s.started && s.fraction().is_some());
        let children: Vec<&Job> = self
            .jobs
            .values()
            .filter(|c| c.parent.as_deref() == Some(job.id.as_str()))
            .collect();
        for c in &children {
            let f = if c.state == JobState::Done {
                Some(1.0)
            } else {
                c.fraction
            };
            known |= f.is_some();
            parts.push((c.weight, f));
        }
        if !children.is_empty()
            && let Some(t) = job.total
        {
            let missing = (t as usize).saturating_sub(children.len());
            parts.extend(std::iter::repeat_n((1.0, Some(0.0)), missing));
        }
        // Parts telling something: the unknown ones count as nothing
        // yet. Parts telling nothing yet: the counts may.
        let weight: f64 = parts.iter().map(|(w, _)| w).sum();
        if known && weight > 0.0 {
            let sum: f64 = parts.iter().map(|(w, f)| w * f.unwrap_or(0.0)).sum();
            return Some((sum / weight).clamp(0.0, 1.0));
        }
        if let Some(t) = job.total.filter(|t| *t > 0) {
            let done = job.done as f64 + job.item_progress.unwrap_or(0.0);
            return Some((done / t as f64).clamp(0.0, 1.0));
        }
        job.hint
    }

    /// Recomputes a job's fraction, then its ancestors'.
    fn recompute(&mut self, id: &str, now: f64) {
        let mut at = Some(id.to_string());
        for _ in 0..DEPTH {
            let Some(id) = at.take() else {
                break;
            };
            let Some(job) = self.jobs.get(&id) else {
                break;
            };
            let f = self.estimate(job).or(job.fraction);
            let job = self.jobs.get_mut(&id).expect("there");
            job.fraction = f;
            job.updated_at = job.updated_at.max(now);
            if job.state == JobState::Running {
                job.sample(now);
            }
            at = job.parent.clone();
            self.changed.insert(id);
        }
    }

    /// Drops the oldest finished top-level jobs past the history's size
    /// (with their children).
    fn trim_history(&mut self) {
        let mut ended: Vec<(u64, String)> = self
            .jobs
            .values()
            .filter(|j| j.parent.is_none() && !j.active())
            .map(|j| (j.serial, j.id.clone()))
            .collect();
        if ended.len() <= self.history {
            return;
        }
        ended.sort();
        let drop = ended.len() - self.history;
        for (_, id) in ended.into_iter().take(drop) {
            self.dismiss(&id);
        }
    }

    /// Forgets a job and its children (a dismissed error, a cleared
    /// history entry); an active job is kept.
    pub fn dismiss(&mut self, id: &str) -> bool {
        if self.jobs.get(id).is_none_or(Job::active) {
            return false;
        }
        let mut gone = vec![id.to_string()];
        let mut i = 0;
        while i < gone.len() {
            let parent = gone[i].clone();
            gone.extend(
                self.jobs
                    .values()
                    .filter(|j| j.parent.as_deref() == Some(parent.as_str()))
                    .map(|j| j.id.clone()),
            );
            i += 1;
            if i > 4096 {
                break;
            }
        }
        for id in gone {
            self.jobs.remove(&id);
            self.logs.remove(&id);
            self.changed.insert(id);
        }
        true
    }

    /// Forgets every finished top-level job.
    pub fn clear_finished(&mut self) {
        let ended: Vec<String> = self
            .jobs
            .values()
            .filter(|j| j.parent.is_none() && !j.active())
            .map(|j| j.id.clone())
            .collect();
        for id in ended {
            self.dismiss(&id);
        }
    }

    pub fn get(&self, id: &str) -> Option<&Job> {
        self.jobs.get(id)
    }

    /// The lines of a job's log still kept, oldest first.
    pub fn log(&self, id: &str) -> Vec<&str> {
        self.log_from(id, 0)
    }

    /// The lines of a job's log from line `from` on (numbered from 0 since
    /// the job first started), as far as they are still kept: what a
    /// console that showed `from` lines writes next.
    pub fn log_from(&self, id: &str, from: u64) -> Vec<&str> {
        let (Some(job), Some(log)) = (self.jobs.get(id), self.logs.get(id)) else {
            return Vec::new();
        };
        let first = job.log_total - log.len() as u64;
        let skip = from.saturating_sub(first) as usize;
        log.iter().skip(skip).map(String::as_str).collect()
    }

    /// Every job, in the order they first started.
    pub fn all(&self) -> Vec<&Job> {
        let mut v: Vec<&Job> = self.jobs.values().collect();
        v.sort_by_key(|j| j.serial);
        v
    }

    /// Top-level jobs (no parent, or a parent not known), in order.
    pub fn roots(&self) -> Vec<&Job> {
        let mut v: Vec<&Job> = self
            .jobs
            .values()
            .filter(|j| j.parent.as_ref().is_none_or(|p| !self.jobs.contains_key(p)))
            .collect();
        v.sort_by_key(|j| j.serial);
        v
    }

    /// A job's children, in order.
    pub fn children(&self, id: &str) -> Vec<&Job> {
        let mut v: Vec<&Job> = self
            .jobs
            .values()
            .filter(|j| j.parent.as_deref() == Some(id))
            .collect();
        v.sort_by_key(|j| j.serial);
        v
    }

    /// The jobs going on, in order.
    pub fn active(&self) -> Vec<&Job> {
        self.all().into_iter().filter(|j| j.active()).collect()
    }

    /// Whether no job goes on (what an agent waits for).
    pub fn idle(&self) -> bool {
        self.jobs.values().all(|j| !j.active())
    }

    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }

    /// All jobs at a glance (the status bar's).
    pub fn summary(&self) -> Summary {
        let roots: Vec<&Job> = self.roots();
        let active: Vec<&&Job> = roots.iter().filter(|j| j.active()).collect();
        let fraction = if active.is_empty() || active.iter().any(|j| j.fraction.is_none()) {
            None
        } else {
            Some(active.iter().filter_map(|j| j.fraction).sum::<f64>() / active.len() as f64)
        };
        Summary {
            active: active.len(),
            failed: roots.iter().filter(|j| j.state == JobState::Failed).count(),
            fraction,
        }
    }

    /// The jobs changed since the last call (by id), for views that
    /// coalesce.
    pub fn take_changed(&mut self) -> BTreeSet<String> {
        std::mem::take(&mut self.changed)
    }

    /// What the `jobs` debug collector shows.
    pub fn describe(&self, now: f64) -> Json {
        json!({
            "idle": self.idle(),
            "summary": self.summary(),
            "jobs": self.all().into_iter().map(|j| {
                let mut v = serde_json::to_value(j).unwrap_or(Json::Null);
                v["percent"] = json!(j.percent());
                v["eta"] = json!(j.eta(now));
                v["elapsed"] = json!(j.elapsed(now));
                let log = self.log(&j.id);
                v["log_tail"] = json!(log[log.len().saturating_sub(5)..]);
                v
            }).collect::<Vec<_>>(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Feed {
        jobs: Jobs,
        seq: u64,
    }

    impl Feed {
        fn new() -> Self {
            Self {
                jobs: Jobs::new(),
                seq: 0,
            }
        }

        fn at(&mut self, now: f64, name: &str, data: Json) -> u64 {
            self.seq += 1;
            self.jobs.apply(&Incoming::new(self.seq, name, &data), now);
            self.seq
        }

        fn job(&self, id: &str) -> &Job {
            self.jobs.get(id).expect("the job")
        }
    }

    fn near(a: Option<f64>, b: f64) -> bool {
        a.is_some_and(|a| (a - b).abs() < 1e-9)
    }

    #[test]
    fn a_job_runs_from_start_to_finish() {
        let mut f = Feed::new();
        f.at(
            0.0,
            STARTED,
            json!({ "id": "e", "title": "Export", "total": 4 }),
        );
        assert!(!f.jobs.idle());
        assert!(near(f.job("e").fraction, 0.0));
        f.at(
            1.0,
            PROGRESS,
            json!({ "id": "e", "done": 2, "item": "b.raw" }),
        );
        assert!(near(f.job("e").percent(), 50.0));
        f.at(2.0, FINISHED, json!({ "id": "e" }));
        let j = f.job("e");
        assert_eq!(j.state, JobState::Done);
        assert!(near(j.fraction, 1.0));
        assert_eq!(j.elapsed(10.0), 2.0);
        assert!(f.jobs.idle());
    }

    #[test]
    fn without_a_total_a_job_is_indeterminate_until_it_can_estimate() {
        let mut f = Feed::new();
        f.at(0.0, STARTED, json!({ "id": "scan" }));
        f.at(1.0, PROGRESS, json!({ "id": "scan", "done": 30 }));
        assert_eq!(f.job("scan").fraction, None);
        f.at(2.0, PROGRESS, json!({ "id": "scan", "total": 120 }));
        assert!(near(f.job("scan").fraction, 0.25));
        // The total grows: the bar goes back, never indeterminate again.
        f.at(3.0, PROGRESS, json!({ "id": "scan", "total": 300 }));
        assert!(near(f.job("scan").fraction, 0.1));
    }

    #[test]
    fn steps_roll_up_weighted_by_their_estimates() {
        let mut f = Feed::new();
        f.at(
            0.0,
            STARTED,
            json!({ "id": "r", "steps": [
                { "id": "decode", "weight": 1 },
                { "id": "process", "weight": 3 },
                { "id": "write", "weight": 1 },
            ] }),
        );
        f.at(0.5, PROGRESS, json!({ "id": "r", "step": "decode" }));
        // The current step tells nothing yet: indeterminate.
        assert_eq!(f.job("r").fraction, None);
        f.at(
            1.0,
            PROGRESS,
            json!({ "id": "r", "step": "process", "step_done": 1, "step_total": 2 }),
        );
        // decode whole (1), half of process (1.5), of 5.
        assert!(near(f.job("r").fraction, 0.5));
        f.at(2.0, PROGRESS, json!({ "id": "r", "step": "write" }));
        // write without a total: decode and process count (4 of 5).
        assert!(near(f.job("r").fraction, 0.8));
    }

    #[test]
    fn child_jobs_roll_up_into_their_parent() {
        let mut f = Feed::new();
        f.at(0.0, STARTED, json!({ "id": "batch", "total": 4 }));
        f.at(
            0.0,
            STARTED,
            json!({ "id": "a", "parent": "batch", "total": 10 }),
        );
        f.at(1.0, PROGRESS, json!({ "id": "a", "done": 5 }));
        // a half done, three more to come: 0.5 of 4.
        assert!(near(f.job("batch").fraction, 0.125));
        f.at(2.0, FINISHED, json!({ "id": "a" }));
        assert!(near(f.job("batch").fraction, 0.25));
        assert_eq!(f.jobs.roots().len(), 1);
        assert_eq!(f.jobs.children("batch").len(), 1);
    }

    #[test]
    fn a_refined_estimate_wins_and_can_be_given_back() {
        let mut f = Feed::new();
        f.at(0.0, STARTED, json!({ "id": "x" }));
        f.jobs.refine("x", Some(0.4), 1.0);
        assert!(near(f.job("x").fraction, 0.4));
        f.jobs.refine("x", None, 2.0);
        // Determinate once: the last estimate stays.
        assert!(near(f.job("x").fraction, 0.4));
    }

    #[test]
    fn time_left_follows_the_rate_after_a_while() {
        let mut f = Feed::new();
        f.at(0.0, STARTED, json!({ "id": "e", "total": 100 }));
        f.at(1.0, PROGRESS, json!({ "id": "e", "done": 10 }));
        assert_eq!(f.job("e").eta(1.0), None, "too early");
        f.at(2.0, PROGRESS, json!({ "id": "e", "done": 20 }));
        f.at(3.0, PROGRESS, json!({ "id": "e", "done": 30 }));
        let eta = f.job("e").eta(3.0).expect("after 3 s");
        assert!((eta - 7.0).abs() < 0.5, "{eta}");
        f.at(4.0, PAUSED, json!({ "id": "e" }));
        assert_eq!(f.job("e").eta(4.0), None, "paused");
    }

    #[test]
    fn requests_wait_for_the_host_and_a_refusal_says_why() {
        let mut f = Feed::new();
        f.at(0.0, STARTED, json!({ "id": "e" }));
        let asked = f.at(
            1.0,
            "command.dispatched",
            json!({ "command": "job.cancel", "args": { "id": "e" } }),
        );
        assert_eq!(f.job("e").requested, Some(Request::Cancel));
        f.seq += 1;
        let data = json!({ "command": "job.cancel", "error": "too late" });
        f.jobs.apply(
            &Incoming::new(f.seq, "command.failed", &data).cause(Some(asked)),
            1.5,
        );
        assert_eq!(f.job("e").requested, None);
        assert_eq!(f.job("e").request_error.as_deref(), Some("too late"));
        f.jobs.request("e", Request::Cancel);
        f.at(2.0, CANCELLED, json!({ "id": "e" }));
        assert_eq!(f.job("e").state, JobState::Cancelled);
        assert_eq!(f.job("e").requested, None);
    }

    #[test]
    fn a_retry_starts_again_and_keeps_the_log() {
        let mut f = Feed::new();
        f.at(0.0, STARTED, json!({ "id": "e", "total": 2 }));
        f.at(0.1, LOG, json!({ "id": "e", "text": "one\ntwo\n" }));
        f.at(1.0, FAILED, json!({ "id": "e", "error": "disk full" }));
        assert_eq!(f.job("e").error.as_deref(), Some("disk full"));
        assert_eq!(f.jobs.summary().failed, 1);
        f.at(2.0, STARTED, json!({ "id": "e", "total": 2 }));
        let j = f.job("e");
        assert_eq!(
            (j.state, j.attempts, j.error.clone()),
            (JobState::Running, 2, None)
        );
        assert_eq!(f.jobs.log("e"), ["one", "two"]);
        assert_eq!(f.jobs.log_from("e", 1), ["two"]);
    }

    #[test]
    fn the_log_and_the_history_are_bounded() {
        let mut f = Feed::new();
        f.jobs = Jobs::new().with_log_lines(2).with_history(1);
        f.at(0.0, STARTED, json!({ "id": "a" }));
        f.at(0.0, LOG, json!({ "id": "a", "text": "1\n2\n3" }));
        assert_eq!(f.jobs.log("a"), ["2", "3"]);
        assert_eq!(f.job("a").log_total, 3);
        assert_eq!(f.jobs.log_from("a", 0).len(), 2);
        f.at(1.0, FINISHED, json!({ "id": "a" }));
        f.at(1.0, STARTED, json!({ "id": "b" }));
        f.at(2.0, FINISHED, json!({ "id": "b" }));
        assert!(f.jobs.get("a").is_none(), "past the history");
        f.jobs.clear_finished();
        assert!(f.jobs.is_empty());
    }

    #[test]
    fn the_summary_is_indeterminate_while_one_job_is() {
        let mut f = Feed::new();
        f.at(0.0, STARTED, json!({ "id": "a", "total": 2 }));
        f.at(0.0, PROGRESS, json!({ "id": "a", "done": 1 }));
        f.at(0.0, STARTED, json!({ "id": "b", "total": 4 }));
        assert!(near(f.jobs.summary().fraction, 0.25));
        f.at(0.0, STARTED, json!({ "id": "c" }));
        assert_eq!(f.jobs.summary().fraction, None);
        assert_eq!(f.jobs.summary().active, 3);
        let d = f.jobs.describe(1.0);
        assert_eq!(d["jobs"].as_array().unwrap().len(), 3);
        assert_eq!(d["idle"], false);
    }
}
