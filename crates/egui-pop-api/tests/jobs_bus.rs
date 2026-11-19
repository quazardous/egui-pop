//! The job lifecycle on the bus (ADR 0029): asking about a job is a
//! request whose listeners run in a declared, reviewed order; the jobs
//! model is a listener of `pop.job.event`, typed events the host
//! publishes, and changes on nothing else (never on a journal entry's
//! name);
//! a plugin's jobs follow its disconnection and its return through
//! listeners of the plugin points.

use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use egui_pop_api::bus::egui_pop_hooks::ListenOpts;
use egui_pop_api::bus::{self, JOB_POINTS, JobAsk, JobEvents, PLUGIN_POINTS};
use egui_pop_api::jobs::{self, JOBS, JobsPlugin, MODEL};
use egui_pop_api::{App, AppHandle};
use egui_pop_model::jobs::{JobEventKind, JobState, Request};
use egui_pop_test::jobs::{Answer, FakeHost};
use serde_json::json;

fn with_host(host: &FakeHost) -> App {
    App::builder()
        .host(host.clone())
        .plugin(JobsPlugin::default())
        .build(&egui::Context::default())
        .unwrap()
}

fn pass(app: &mut App) {
    app.logic(&egui::Context::default());
}

fn points() -> Vec<&'static str> {
    JOB_POINTS.iter().chain(&PLUGIN_POINTS).copied().collect()
}

/// The reviewed order of the job and plugin points' listeners: a change
/// to it is a change to this file, read in review.
#[test]
fn the_job_points_run_in_their_reviewed_order() {
    let app = with_host(&FakeHost::new());
    let h = app.handle();
    let dump = h.bus_order_of(&points());
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/bus/jobs-points.txt");
    if std::env::var_os("UPDATE_BUS_ORDER").is_some() {
        fs::write(&path, &dump).unwrap();
    }
    let reviewed = fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        dump,
        reviewed,
        "the job points' order changed: review it, then UPDATE_BUS_ORDER=1 writes {}",
        path.display()
    );
    assert_eq!(
        bus::missing_targets(&h.bus(), &points()),
        Vec::<String>::new()
    );
}

/// What a listener after the model saw: each event's name and the job's
/// state as the model had it then.
type Seen = Arc<Mutex<Vec<(String, Option<JobState>, Option<Request>)>>>;

fn watch(h: &AppHandle) -> Seen {
    let seen = Seen::default();
    let s = seen.clone();
    // The service is `Send + Sync`: the listener may keep it.
    let jobs = h.service(&JOBS).unwrap();
    h.bus()
        .listen::<JobEvents>("test.watch", ListenOpts::new().after(MODEL), move |e, _| {
            let job = jobs.job("export");
            s.lock().unwrap().push((
                e.kind.name().to_string(),
                job.as_ref().map(|j| j.state),
                job.and_then(|j| j.requested),
            ));
            Ok(())
        })
        .unwrap();
    seen
}

#[test]
fn a_job_driven_through_the_bus_changes_only_on_its_events() {
    let host = FakeHost::new();
    host.answer(Answer::Later);
    let mut app = with_host(&host);
    let h = app.handle().clone();
    let jobs = h.service(&JOBS).unwrap();
    let seen = watch(&h);

    // Said by the host: nothing changes until the bus delivers it.
    host.job("export").start("Export", Some(4)).progress(1);
    assert!(jobs.job("export").is_none());
    h.deliver_events();
    assert_eq!(jobs.job("export").unwrap().fraction, Some(0.25));
    // Delivered once: a pass delivers nothing again.
    pass(&mut app);
    assert_eq!(seen.lock().unwrap().len(), 2);

    // Asked on the bus: the model marks it, then the host is asked; the
    // listeners say so, in order.
    let did = h.request_job(JobAsk::new("export", Request::Cancel));
    assert_eq!(
        did,
        [
            "pop.jobs.model: `export` waits for the answer",
            "pop.jobs.host: `job.cancel` asked"
        ]
    );
    assert_eq!(jobs.job("export").unwrap().requested, Some(Request::Cancel));
    pass(&mut app);
    assert_eq!(host.asked().len(), 1);
    assert_eq!(host.asked()[0].0, "job.cancel");
    assert_eq!(jobs.job("export").unwrap().state, JobState::Running);

    // The host answers: the model follows its event, and a listener after
    // `pop.jobs.model` finds it followed already.
    assert_eq!(host.release(), 1);
    pass(&mut app);
    pass(&mut app);
    assert_eq!(jobs.job("export").unwrap().state, JobState::Cancelled);
    let seen = seen.lock().unwrap();
    let cancelled = seen
        .iter()
        .find(|(name, ..)| name == "job.cancelled")
        .expect("delivered");
    assert_eq!(cancelled.1, Some(JobState::Cancelled));
    assert_eq!(
        seen.iter()
            .filter(|(name, ..)| name == "job.started")
            .count(),
        1,
        "each event delivered once"
    );
}

#[test]
fn asked_without_the_jobs_plugin_a_request_is_refused_on_the_journal() {
    let app = App::builder().build(&egui::Context::default()).unwrap();
    let h = app.handle();
    let before = h.last_event();
    assert!(
        h.request_job(JobAsk::new("export", Request::Pause))
            .is_empty()
    );
    let refused: Vec<_> = h
        .events_since(before)
        .into_iter()
        .filter(|e| e.name == "job.refused")
        .collect();
    assert_eq!(refused.len(), 1);
    assert_eq!(refused[0].data["request"], "job.pause");
    assert_eq!(refused[0].data["id"], "export");
    // `jobs::ask` is the same request.
    jobs::ask(h, "export", Request::Retry);
    assert_eq!(
        h.events_since(before)
            .iter()
            .filter(|e| e.name == "job.refused")
            .count(),
        2
    );
}

/// The jobs centre follows the declared point, not the journal: an entry
/// merely named like a job event changes nothing.
#[test]
fn a_journal_entry_named_like_a_job_event_is_not_one() {
    let mut app = with_host(&FakeHost::new());
    let h = app.handle().clone();
    let jobs = h.service(&JOBS).unwrap();
    h.publish(
        "host",
        None,
        "job.started",
        json!({ "id": "export", "title": "Export", "total": 4 }),
    );
    h.publish(
        "host",
        None,
        "job.progress",
        json!({ "id": "export", "done": 2 }),
    );
    h.deliver_events();
    pass(&mut app);
    assert!(jobs.job("export").is_none());
    assert!(jobs.all().is_empty());
}

/// The host publishes `pop.job.event` from any thread, with the
/// publisher the application handed it: the centre follows at the next
/// delivery, and the journal records each event under its label.
#[test]
fn the_centre_follows_the_job_points_the_host_publishes() {
    let mut app = with_host(&FakeHost::new());
    let h = app.handle().clone();
    let jobs = h.service(&JOBS).unwrap();
    let publisher = h.job_publisher();
    std::thread::spawn(move || {
        publisher.publish(
            "host",
            JobEventKind::Started,
            json!({ "id": "export", "title": "Export", "total": 4 }),
        );
        publisher.publish(
            "host",
            JobEventKind::Progress,
            json!({ "id": "export", "done": 2 }),
        );
    })
    .join()
    .unwrap();
    assert!(jobs.job("export").is_none(), "nothing before a delivery");
    pass(&mut app);
    let job = jobs.job("export").unwrap();
    assert_eq!(job.fraction, Some(0.5));
    assert_eq!(job.state, JobState::Running);
    h.publish_job("host", JobEventKind::Finished, json!({ "id": "export" }));
    pass(&mut app);
    assert_eq!(jobs.job("export").unwrap().state, JobState::Done);
    let names: Vec<String> = h
        .events_since(0)
        .into_iter()
        .map(|e| e.name)
        .filter(|n| n.starts_with("job."))
        .collect();
    assert_eq!(names, ["job.started", "job.progress", "job.finished"]);
}
