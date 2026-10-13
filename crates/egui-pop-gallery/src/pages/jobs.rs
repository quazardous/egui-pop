//! The jobs page: a pretend host running five demo jobs (an export with
//! a failed item, a render in weighted steps, a scan of unknown size, a
//! batch of child imports, an upload that fails), stepped by hand so
//! every state is reproducible, and the model they fold into.

use std::cell::RefCell;
use std::rc::Rc;

use egui::Ui;
use egui_pop::api::jobs::{self as api_jobs, JOBS, STEP_ESTIMATE};
use egui_pop::api::stories::Story;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::model::ids::{PluginId, StoryId};
use egui_pop::model::jobs::{Job, JobState, Request};
use serde_json::{Value as Json, json};

use super::dialogs::CMD_DONE;
use super::section;
use crate::shell::SHELL;

pub const JOBS_PAGE: PluginId = PluginId::from_static("gallery.jobs");

/// The context key the `jobs.estimate` hook reads.
const REFINE: &str = "galleryJobsRefine";

/// Seconds between two steps when the demo runs by itself.
const AUTO_STEP: f64 = 0.4;

const EXPORT: &str = "gallery-export";
const RENDER: &str = "gallery-render";
const SCAN: &str = "gallery-scan";
const IMPORT: &str = "gallery-import";
const UPLOAD: &str = "gallery-upload";
const IMPORTS: [&str; 3] = ["gallery-import-a", "gallery-import-b", "gallery-import-c"];

/// Where a demo job is, as the pretend host sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Run {
    Running,
    Paused,
    Ended,
}

#[derive(Debug)]
struct SimJob {
    id: &'static str,
    tick: u32,
    run: Run,
}

#[derive(Debug, Default)]
struct SimState {
    jobs: Vec<SimJob>,
    auto: bool,
    last: f64,
}

/// The pretend host: it publishes the demo jobs' events, one step at a
/// time, and answers cancel, pause, resume and retry.
#[derive(Clone, Debug, Default)]
pub struct Sim(Rc<RefCell<SimState>>);

/// A demo job's `job.started` data.
fn started(id: &str) -> Json {
    match id {
        EXPORT => json!({
            "id": id, "title": "Export 12 images", "kind": "export", "total": 12,
            "pausable": true,
        }),
        RENDER => json!({
            "id": id, "title": "Render the panorama", "kind": "render",
            "steps": [
                { "id": "decode", "title": "Decode", "weight": 1 },
                { "id": "process", "title": "Process", "weight": 3 },
                { "id": "write", "title": "Write", "weight": 1 },
            ],
        }),
        SCAN => json!({ "id": id, "title": "Scan the library", "kind": "scan" }),
        IMPORT => json!({
            "id": id, "title": "Import 3 folders", "kind": "import", "total": 3,
        }),
        UPLOAD => json!({
            "id": id, "title": "Upload to the server", "kind": "upload", "total": 10,
        }),
        child => json!({
            "id": child, "title": format!("Import {}", child.trim_start_matches("gallery-import-")),
            "kind": "import", "parent": IMPORT, "total": 4,
        }),
    }
}

fn publish(app: &AppHandle, name: &str, data: Json) {
    app.publish("gallery", None, name, data);
}

impl Sim {
    /// Starts (or starts again) the five demo jobs.
    pub fn start(&self, app: &AppHandle) {
        let mut s = self.0.borrow_mut();
        s.jobs.clear();
        for id in [EXPORT, RENDER, SCAN, IMPORT, UPLOAD, IMPORTS[0]] {
            publish(app, "job.started", started(id));
            s.jobs.push(SimJob {
                id,
                tick: 0,
                run: Run::Running,
            });
        }
    }

    /// Moves every running job one step on.
    pub fn step(&self, app: &AppHandle) {
        let mut s = self.0.borrow_mut();
        let mut next_import = None;
        for j in s.jobs.iter_mut().filter(|j| j.run == Run::Running) {
            j.tick += 1;
            let n = j.tick;
            let id = j.id;
            match id {
                EXPORT => {
                    let file = format!("IMG_{n:04}.raw");
                    publish(
                        app,
                        "job.log",
                        json!({ "id": id, "text": format!("\x1b[32mexported\x1b[0m {file} \x1b[2m-> out/IMG_{n:04}.jpg\x1b[0m") }),
                    );
                    if n == 6 {
                        publish(
                            app,
                            "job.item_failed",
                            json!({ "id": id, "item": file, "error": "unreadable" }),
                        );
                        publish(
                            app,
                            "job.log",
                            json!({ "id": id, "text": format!("\x1b[31merror\x1b[0m {file}: unreadable") }),
                        );
                    }
                    if n >= 12 {
                        publish(
                            app,
                            "job.finished",
                            json!({ "id": id, "actions": [
                            { "title": "Show", "command": CMD_DONE.as_str(), "args": { "what": "show the export" } },
                        ] }),
                        );
                        j.run = Run::Ended;
                    } else {
                        publish(
                            app,
                            "job.progress",
                            json!({ "id": id, "done": n, "item": format!("IMG_{:04}.raw", n + 1) }),
                        );
                    }
                }
                RENDER => {
                    let data = match n {
                        1 => json!({ "id": id, "step": "decode", "phase": "decode" }),
                        2..=5 => {
                            json!({ "id": id, "step": "process", "step_done": n - 2, "step_total": 4, "phase": "process" })
                        }
                        6 => json!({ "id": id, "step": "write", "phase": "write" }),
                        _ => {
                            publish(app, "job.finished", json!({ "id": id }));
                            j.run = Run::Ended;
                            continue;
                        }
                    };
                    publish(app, "job.progress", data);
                }
                SCAN => {
                    if n <= 3 {
                        publish(
                            app,
                            "job.progress",
                            json!({ "id": id, "done": 30 * n, "item": format!("folder {n}") }),
                        );
                    } else if n < 8 {
                        publish(
                            app,
                            "job.progress",
                            json!({ "id": id, "done": 80 * (n - 3), "total": 400 }),
                        );
                    } else {
                        publish(app, "job.finished", json!({ "id": id }));
                        j.run = Run::Ended;
                    }
                }
                UPLOAD => {
                    if n < 3 {
                        publish(app, "job.progress", json!({ "id": id, "done": n * 2 }));
                    } else {
                        publish(
                            app,
                            "job.log",
                            json!({ "id": id, "text": "\x1b[31mconnection reset by peer\x1b[0m" }),
                        );
                        publish(
                            app,
                            "job.failed",
                            json!({ "id": id, "error": "connection reset by peer" }),
                        );
                        j.run = Run::Ended;
                    }
                }
                IMPORT => {}
                child => {
                    if n < 4 {
                        publish(app, "job.progress", json!({ "id": child, "done": n }));
                    } else {
                        publish(app, "job.finished", json!({ "id": child }));
                        j.run = Run::Ended;
                        next_import =
                            Some(IMPORTS.iter().position(|c| *c == child).unwrap_or(2) + 1);
                    }
                }
            }
        }
        if let Some(i) = next_import {
            match IMPORTS.get(i) {
                Some(child) => {
                    publish(app, "job.started", started(child));
                    s.jobs.push(SimJob {
                        id: child,
                        tick: 0,
                        run: Run::Running,
                    });
                }
                None => {
                    publish(app, "job.finished", json!({ "id": IMPORT }));
                    if let Some(j) = s.jobs.iter_mut().find(|j| j.id == IMPORT) {
                        j.run = Run::Ended;
                    }
                }
            }
        }
    }

    /// Answers a job command, as a host would.
    pub fn answer(&self, app: &AppHandle, request: Request, id: &str) -> Result<Json, String> {
        let mut s = self.0.borrow_mut();
        let ids: Vec<&'static str> = s
            .jobs
            .iter()
            .filter(|j| j.id == id || (id == IMPORT && IMPORTS.contains(&j.id)))
            .map(|j| j.id)
            .collect();
        if ids.is_empty() {
            return Err(format!("no job `{id}` in the demo"));
        }
        if request == Request::Retry && id == IMPORT {
            // The batch starts again from its first folder.
            s.jobs.retain(|j| !IMPORTS.contains(&j.id));
            if let Some(j) = s.jobs.iter_mut().find(|j| j.id == IMPORT) {
                j.run = Run::Running;
                j.tick = 0;
            }
            publish(app, "job.started", started(IMPORT));
            publish(app, "job.started", started(IMPORTS[0]));
            s.jobs.push(SimJob {
                id: IMPORTS[0],
                tick: 0,
                run: Run::Running,
            });
            return Ok(json!({ "id": id }));
        }
        for j in s.jobs.iter_mut().filter(|j| ids.contains(&j.id)) {
            match request {
                Request::Cancel if j.run != Run::Ended => {
                    j.run = Run::Ended;
                    publish(app, "job.cancelled", json!({ "id": j.id }));
                }
                Request::Pause if j.run == Run::Running => {
                    j.run = Run::Paused;
                    publish(app, "job.paused", json!({ "id": j.id }));
                }
                Request::Resume if j.run == Run::Paused => {
                    j.run = Run::Running;
                    publish(app, "job.resumed", json!({ "id": j.id }));
                }
                Request::Retry if j.run == Run::Ended => {
                    j.run = Run::Running;
                    j.tick = 0;
                    publish(app, "job.started", started(j.id));
                }
                _ => {}
            }
        }
        Ok(json!({ "id": id }))
    }
}

/// The jobs page and its pretend host.
#[derive(Debug, Default)]
pub struct JobsPage {
    pub sim: Sim,
}

impl Plugin for JobsPage {
    fn id(&self) -> PluginId {
        JOBS_PAGE
    }

    fn title(&self) -> String {
        "Jobs".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL, super::dialogs::DIALOGS_PAGE]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        // The scan cannot count what is left; the application knows a
        // library holds about 500 folders.
        ctx.replace(STEP_ESTIMATE, |app, args| {
            let own = args["fraction"].clone();
            if app.context().get(REFINE) != "true" || args["id"] != SCAN || !own.is_null() {
                return Ok(json!({ "fraction": own }));
            }
            let done = args["done"].as_f64().unwrap_or(0.0);
            Ok(json!({ "fraction": (done / 500.0).min(0.95) }))
        });
        let sim = self.sim.clone();
        ctx.story(
            Story::new(StoryId::from_static("jobs"), "Jobs", move |ui, app| {
                show(ui, app, &sim)
            })
            .description("Background jobs followed from the host's journal: every job with an estimated overall percentage (steps and child jobs rolled up), cancel, pause, resume and retry as commands.")
            .group("Progress and jobs", 130),
        )?;
        Ok(())
    }
}

/// A job's estimate as text.
pub fn estimate(job: &Job) -> String {
    match job.percent() {
        Some(p) => format!("{p:.0}%"),
        None => "indeterminate".into(),
    }
}

fn show(ui: &mut Ui, app: &AppHandle, sim: &Sim) {
    let Some(jobs) = app.service(&JOBS) else {
        ui.weak("The jobs plugin is not connected.");
        return;
    };
    ui.label("A pretend host publishes job.* events on the journal; the model folds them, nothing polls. Step the demo by hand, or let it run.");
    section(ui, "The demo host");
    ui.horizontal_wrapped(|ui| {
        if ui.button("Start the demo jobs").clicked() {
            sim.start(app);
        }
        if ui.button("Advance one step").clicked() {
            sim.step(app);
        }
        if ui.button("Advance five steps").clicked() {
            for _ in 0..5 {
                sim.step(app);
            }
        }
        let mut auto = sim.0.borrow().auto;
        if ui.checkbox(&mut auto, "Advance by itself").changed() {
            sim.0.borrow_mut().auto = auto;
        }
        let mut refine = app.context().get(REFINE) == "true";
        if ui
            .checkbox(&mut refine, "Estimate the scan (a hook on jobs.estimate)")
            .changed()
        {
            app.set_context(REFINE, if refine { "true" } else { "false" });
        }
    });
    if sim.0.borrow().auto {
        let now = ui.input(|i| i.time);
        let due = {
            let mut s = sim.0.borrow_mut();
            let due = now - s.last >= AUTO_STEP;
            if due {
                s.last = now;
            }
            due
        };
        if due {
            sim.step(app);
        }
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs_f64(AUTO_STEP));
    }
    section(ui, "The model");
    let now = jobs.now();
    let roots = jobs.roots();
    if roots.is_empty() {
        ui.weak("No job yet.");
    }
    egui::Grid::new("gallery.jobs.model")
        .num_columns(5)
        .striped(true)
        .show(ui, |ui| {
            for job in &roots {
                row(ui, app, job, now, 0);
                for child in jobs.children(&job.id) {
                    row(ui, app, &child, now, 1);
                }
            }
        });
}

fn row(ui: &mut Ui, app: &AppHandle, job: &Job, now: f64, depth: usize) {
    let indent = "    ".repeat(depth);
    ui.label(format!("{indent}{}", job.title));
    let mut state = job.state.name().to_string();
    if let Some(r) = job.requested {
        state = format!("{state} ({} asked)", api_jobs::command_of(r));
    }
    ui.label(state);
    ui.label(estimate(job));
    let mut detail = Vec::new();
    if let Some(t) = job.total {
        detail.push(format!("{} of {t}", job.done));
    }
    if let Some(step) = &job.step {
        detail.push(format!("step {step}"));
    }
    if let Some(item) = &job.item {
        detail.push(item.clone());
    }
    if job.failed > 0 {
        detail.push(format!("{} failed", job.failed));
    }
    if let Some(e) = &job.error {
        detail.push(format!("error: {e}"));
    }
    if let Some(eta) = job.eta(now) {
        detail.push(format!("{eta:.0} s left"));
    }
    ui.label(detail.join(", "));
    ui.horizontal(|ui| {
        let ask = |ui: &mut Ui, label: &str, request: Request| {
            let r = ui.small_button(label);
            egui_pop::core::a11y::name(
                &r,
                egui::WidgetType::Button,
                &format!("{label} {}", job.title),
            );
            if r.clicked() {
                api_jobs::ask(app, &job.id, request);
            }
        };
        if job.active() && job.parent.is_none() {
            if job.pausable && job.state == JobState::Running {
                ask(ui, "Pause", Request::Pause);
            }
            if job.state == JobState::Paused {
                ask(ui, "Resume", Request::Resume);
            }
            if job.cancellable {
                ask(ui, "Cancel", Request::Cancel);
            }
        } else if job.retryable && job.parent.is_none() && !job.active() {
            ask(ui, "Retry", Request::Retry);
        }
    });
    ui.end_row();
}
