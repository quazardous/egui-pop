//! Jobs followed from the journal: the model behind the service, the
//! commands run in-process or refused by a host without them, the
//! `jobs.estimate` step refining a job's fraction, the collector.

use std::cell::Cell;
use std::rc::Rc;

use egui_pop_api::jobs::{self, CMD_CANCEL, JOBS, JobsPlugin, STEP_ESTIMATE};
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_model::clock::ManualClock;
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::PluginId;
use egui_pop_model::jobs::{JobState, Request};
use serde_json::json;

fn pass(app: &mut App) {
    app.logic(&egui::Context::default());
}

#[test]
fn jobs_follow_the_journal_without_polling() {
    let mut app = App::builder()
        .plugin(JobsPlugin::default())
        .build(&egui::Context::default())
        .unwrap();
    let h = app.handle().clone();
    let jobs = h.service(&JOBS).unwrap();
    assert!(jobs.idle());
    h.publish(
        "host",
        None,
        "job.started",
        json!({ "id": "e", "title": "Export", "total": 4, "pausable": true }),
    );
    h.publish(
        "host",
        None,
        "job.progress",
        json!({ "id": "e", "done": 3, "item": "c.raw" }),
    );
    // Nothing is read until a pass.
    assert!(jobs.job("e").is_none());
    pass(&mut app);
    let e = jobs.job("e").unwrap();
    assert_eq!(e.fraction, Some(0.75));
    assert_eq!(e.item.as_deref(), Some("c.raw"));
    assert!(e.pausable);
    assert!(!jobs.idle());
    h.publish("host", None, "job.finished", json!({ "id": "e" }));
    pass(&mut app);
    assert_eq!(jobs.job("e").unwrap().state, JobState::Done);
    assert!(jobs.idle());
    let snap = h.debug_snapshot();
    assert_eq!(snap["jobs"]["idle"], true, "{snap}");
    assert_eq!(snap["jobs"]["jobs"][0]["percent"], 100.0);
}

#[test]
fn a_host_without_the_commands_refuses_and_the_job_says_why() {
    let mut app = App::builder()
        .plugin(JobsPlugin::default())
        .build(&egui::Context::default())
        .unwrap();
    let h = app.handle().clone();
    h.publish("host", None, "job.started", json!({ "id": "e" }));
    pass(&mut app);
    jobs::ask(&h, "e", Request::Cancel);
    let jobs = h.service(&JOBS).unwrap();
    assert_eq!(jobs.job("e").unwrap().requested, Some(Request::Cancel));
    pass(&mut app);
    pass(&mut app);
    let e = jobs.job("e").unwrap();
    assert_eq!(e.requested, None);
    assert!(
        e.request_error
            .as_deref()
            .is_some_and(|w| w.contains("no handler")),
        "{e:?}"
    );
    assert_eq!(e.state, JobState::Running);
}

#[test]
fn an_in_process_runner_answers_the_commands() {
    let mut app = App::builder()
        .plugin(JobsPlugin::default().run_locally(|app, request, id| {
            let event = match request {
                Request::Cancel => "job.cancelled",
                Request::Pause => "job.paused",
                Request::Resume => "job.resumed",
                Request::Retry => "job.started",
            };
            app.publish("runner", None, event, json!({ "id": id }));
            Ok(json!({ "id": id }))
        }))
        .build(&egui::Context::default())
        .unwrap();
    let h = app.handle().clone();
    h.publish("host", None, "job.started", json!({ "id": "e" }));
    pass(&mut app);
    let out = app.run(&CMD_CANCEL, json!({ "id": "e" }), Origin::User);
    assert_eq!(out.now(), Some(Ok(json!({ "id": "e" }))));
    pass(&mut app);
    let jobs = h.service(&JOBS).unwrap();
    assert_eq!(jobs.job("e").unwrap().state, JobState::Cancelled);
    let out = app.run(&jobs::CMD_RETRY, json!({ "id": "e" }), Origin::User);
    assert!(out.now().is_some_and(|r| r.is_ok()));
    pass(&mut app);
    let e = jobs.job("e").unwrap();
    assert_eq!((e.state, e.attempts), (JobState::Running, 2));
    // Without an id, the command says so.
    let out = app.run(&CMD_CANCEL, json!({}), Origin::User);
    assert_eq!(out.now(), Some(Err("`id` is missing".to_string())));
}

struct Refiner(Rc<Cell<usize>>);

impl Plugin for Refiner {
    fn id(&self) -> PluginId {
        PluginId::from_static("refiner")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let runs = self.0.clone();
        // A scan of unknown size: the application knows it is about a
        // tenth done per hundred files.
        ctx.replace(STEP_ESTIMATE, move |_, args| {
            runs.set(runs.get() + 1);
            let done = args["done"].as_f64().unwrap_or(0.0);
            Ok(json!({ "fraction": (done / 1000.0).min(0.99) }))
        });
        Ok(())
    }
}

#[test]
fn a_hook_on_jobs_estimate_refines_an_indeterminate_job() {
    let runs = Rc::new(Cell::new(0));
    let clock = ManualClock::new(100.0);
    let mut app = App::builder()
        .plugin(JobsPlugin::default().clock(clock.shared()))
        .plugin(Refiner(runs.clone()))
        .build(&egui::Context::default())
        .unwrap();
    let h = app.handle().clone();
    h.publish("host", None, "job.started", json!({ "id": "scan" }));
    h.publish(
        "host",
        None,
        "job.progress",
        json!({ "id": "scan", "done": 200 }),
    );
    pass(&mut app);
    let jobs = h.service(&JOBS).unwrap();
    let scan = jobs.job("scan").unwrap();
    assert_eq!(scan.fraction, Some(0.2));
    assert_eq!(scan.started_at, 100.0, "the plugin's clock");
    assert_eq!(runs.get(), 1, "once per pass for the job that moved");
    // A pass without events runs nothing.
    pass(&mut app);
    assert_eq!(runs.get(), 1);
    clock.advance(4.0);
    h.publish(
        "host",
        None,
        "job.progress",
        json!({ "id": "scan", "done": 500 }),
    );
    pass(&mut app);
    assert_eq!(jobs.job("scan").unwrap().fraction, Some(0.5));
    assert_eq!(runs.get(), 2);
    assert!(jobs.job("scan").unwrap().eta(jobs.now()).is_some());
}

#[test]
fn the_collector_asks_the_host_and_forgets_finished_jobs() {
    let mut app = App::builder()
        .plugin(JobsPlugin::default().run_locally(|app, _, id| {
            app.publish("runner", None, "job.cancelled", json!({ "id": id }));
            Ok(json!(null))
        }))
        .build(&egui::Context::default())
        .unwrap();
    let h = app.handle().clone();
    h.publish("host", None, "job.started", json!({ "id": "e" }));
    pass(&mut app);
    let out = app.debug_call("jobs", "cancel", &json!({ "id": "e" }));
    assert!(out.ok, "{out:?}");
    let out = app.debug_call("jobs", "cancel", &json!({ "id": "nope" }));
    assert!(!out.ok);
    pass(&mut app);
    pass(&mut app);
    let jobs = h.service(&JOBS).unwrap();
    assert_eq!(jobs.job("e").unwrap().state, JobState::Cancelled);
    assert!(app.debug_call("jobs", "dismiss", &json!({ "id": "e" })).ok);
    assert!(jobs.job("e").is_none());
}
