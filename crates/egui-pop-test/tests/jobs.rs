//! The fake host scripts jobs from start to failure without threads,
//! and answers the job commands now, later or never.

use egui_pop_api::App;
use egui_pop_api::jobs::{self, JOBS, JobsPlugin};
use egui_pop_model::jobs::{JobState, Request};
use egui_pop_test::jobs::{Answer, FakeHost};

fn app(host: &FakeHost) -> App {
    App::builder()
        .host(host.clone())
        .plugin(JobsPlugin::default())
        .build(&egui::Context::default())
        .unwrap()
}

fn pass(app: &mut App) {
    app.logic(&egui::Context::default());
}

#[test]
fn a_scripted_job_runs_from_start_to_failure_and_is_retried() {
    let host = FakeHost::new();
    let mut app = app(&host);
    let jobs = app.handle().service(&JOBS).unwrap();
    let export = host.job("export");
    export
        .start("Export 4 images", Some(4))
        .item(1, "a.raw")
        .log("exported a.raw\n")
        .item_failed("b.raw", "unreadable")
        .item(2, "c.raw");
    pass(&mut app);
    let job = jobs.job("export").unwrap();
    assert_eq!(job.fraction, Some(0.5));
    assert_eq!(job.failed, 1);
    assert_eq!(job.errors[0].item, "b.raw");
    assert_eq!(jobs.log_from("export", 0), ["exported a.raw"]);
    export.fail("disk full");
    pass(&mut app);
    assert_eq!(jobs.job("export").unwrap().state, JobState::Failed);
    assert_eq!(jobs.summary().failed, 1);
    // Retry: the host starts it again from its first data.
    jobs::ask(app.handle(), "export", Request::Retry);
    pass(&mut app);
    pass(&mut app);
    let job = jobs.job("export").unwrap();
    assert_eq!((job.state, job.attempts), (JobState::Running, 2));
    assert_eq!(job.total, Some(4));
    assert_eq!(host.asked()[0].0, "job.retry");
}

#[test]
fn a_cancel_answered_later_shows_until_the_host_confirms() {
    let host = FakeHost::new();
    host.answer(Answer::Later);
    let mut app = app(&host);
    let jobs = app.handle().service(&JOBS).unwrap();
    host.job("render").start("Render", None);
    pass(&mut app);
    assert_eq!(jobs.job("render").unwrap().fraction, None, "indeterminate");
    jobs::ask(app.handle(), "render", Request::Cancel);
    pass(&mut app);
    pass(&mut app);
    assert_eq!(host.waiting(), 1);
    assert!(app.handle().running(&jobs::CMD_CANCEL));
    let job = jobs.job("render").unwrap();
    assert_eq!(job.requested, Some(Request::Cancel));
    assert_eq!(job.state, JobState::Running);
    assert_eq!(host.release(), 1);
    pass(&mut app);
    pass(&mut app);
    let job = jobs.job("render").unwrap();
    assert_eq!((job.state, job.requested), (JobState::Cancelled, None));
    assert!(!app.handle().running(&jobs::CMD_CANCEL));
    assert!(jobs.idle());
}

#[test]
fn a_refused_request_says_why() {
    let host = FakeHost::new();
    host.answer(Answer::Refuse("the export cannot pause".into()));
    let mut app = app(&host);
    let jobs = app.handle().service(&JOBS).unwrap();
    host.job("export")
        .start("Export", Some(10))
        .progress(4)
        .pause();
    pass(&mut app);
    assert_eq!(jobs.job("export").unwrap().state, JobState::Paused);
    host.job("export").resume();
    pass(&mut app);
    jobs::ask(app.handle(), "export", Request::Pause);
    pass(&mut app);
    pass(&mut app);
    let job = jobs.job("export").unwrap();
    assert_eq!(job.state, JobState::Running);
    assert_eq!(job.requested, None);
    assert_eq!(
        job.request_error.as_deref(),
        Some("the export cannot pause")
    );
}
