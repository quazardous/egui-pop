//! The viewer's video page over a pretend player, so these run in every build:
//! what the links policy sends there, the controls, the keys, the repaints,
//! when the player stops, and the error page (ADR 0017).

#![cfg(feature = "markdown")]

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::dialogs::DialogsPlugin;
use egui_pop_api::links::{self, How, LinksPlugin, RecordingOpener, Request, Source};
use egui_pop_api::{App, AppHandle};
use egui_pop_views::viewer::{
    self, Docs, PANEL, VIEWER, VideoBackend, VideoFrame, VideoPlayer, VideoRequest, VideoState,
    VideoStatus, Viewer, ViewerPlugin,
};
use serde_json::{Value, json};

const LENGTH: Duration = Duration::from_secs(60);

/// What the pretend player was asked, and what it answers.
#[derive(Default)]
struct Log {
    opened: Vec<(PathBuf, f32)>,
    open: usize,
    state: VideoState,
    position: Duration,
    volume: f32,
    seeks: Vec<Duration>,
    frames: Vec<VideoFrame>,
    frames_taken: usize,
    /// `open` answers this.
    refuse: Option<String>,
    /// `poll` answers this.
    fail: Option<String>,
}

#[derive(Clone, Default)]
struct Pretend(Arc<Mutex<Log>>);

impl Pretend {
    fn log(&self) -> std::sync::MutexGuard<'_, Log> {
        self.0.lock().unwrap()
    }

    /// A frame for the page to take, its colour naming it.
    fn frame(&self, shade: u8) {
        let image = egui::ColorImage::filled([4, 2], egui::Color32::from_gray(shade));
        self.log().frames.push(VideoFrame {
            image: Arc::new(image),
            aspect: 2.0,
        });
    }
}

struct Player(Pretend);

impl VideoPlayer for Player {
    fn poll(&mut self) -> Result<VideoStatus, String> {
        let log = self.0.log();
        match &log.fail {
            Some(e) => Err(e.clone()),
            None => Ok(VideoStatus {
                state: log.state,
                position: Some(log.position),
                duration: Some(LENGTH),
                note: None,
            }),
        }
    }

    fn frame(&mut self) -> Option<VideoFrame> {
        let mut log = self.0.log();
        let frame = log.frames.pop();
        if frame.is_some() {
            log.frames_taken += 1;
        }
        log.frames.clear();
        frame
    }

    fn play(&mut self) -> Result<(), String> {
        self.0.log().state = VideoState::Playing;
        Ok(())
    }

    fn pause(&mut self) -> Result<(), String> {
        self.0.log().state = VideoState::Paused;
        Ok(())
    }

    fn seek(&mut self, to: Duration) -> Result<(), String> {
        let mut log = self.0.log();
        log.position = to;
        log.seeks.push(to);
        Ok(())
    }

    fn set_volume(&mut self, volume: f32) {
        self.0.log().volume = volume;
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.0.log().open -= 1;
    }
}

impl VideoBackend for Pretend {
    fn name(&self) -> &str {
        "pretend"
    }

    fn open(&self, request: &VideoRequest) -> Result<Box<dyn VideoPlayer>, String> {
        let mut log = self.log();
        if let Some(e) = &log.refuse {
            return Err(e.clone());
        }
        log.opened.push((request.path.clone(), request.volume));
        log.open += 1;
        log.state = VideoState::Paused;
        Ok(Box::new(Player(self.clone())))
    }
}

/// A file named like a video: nothing here reads it but the existence check.
struct Clip(PathBuf);

impl Clip {
    fn new(test: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "egui-pop-viewer-video-{}-{test}.mp4",
            std::process::id()
        ));
        std::fs::write(&path, b"bytes").unwrap();
        Self(path)
    }

    fn target(&self) -> String {
        format!("file://{}", self.0.display())
    }
}

impl Drop for Clip {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn plugins(pretend: Option<&Pretend>) -> egui_pop_api::AppBuilder {
    let viewer = ViewerPlugin::default().docs(Docs::new().page("other", "# Other"));
    let viewer = match pretend {
        Some(p) => viewer.video(Arc::new(p.clone())),
        None => viewer,
    };
    App::builder()
        .plugin(DialogsPlugin)
        .plugin(LinksPlugin::default())
        .plugin(viewer)
}

fn viewer_of(app: &AppHandle) -> Arc<Viewer> {
    app.service(&VIEWER).unwrap()
}

type Shown<'a> = Harness<'a, Option<App>>;

/// The application in a window of its own, the clip open in its viewer.
fn shown<'a>(pretend: &Pretend, target: &str) -> Shown<'a> {
    let p = pretend.clone();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1000.0, 640.0))
        .build_ui_state(
            move |ui, app: &mut Option<App>| {
                let app = app.get_or_insert_with(|| plugins(Some(&p)).build(ui.ctx()).unwrap());
                app.show(ui);
            },
            None,
        );
    h.run();
    let app = handle(&h);
    links::open(&app, Request::new(target));
    h.run();
    h
}

fn handle(h: &Shown<'_>) -> AppHandle {
    h.state().as_ref().unwrap().handle().clone()
}

fn video(h: &Shown<'_>) -> Value {
    viewer_of(&handle(h)).state()["video"].clone()
}

#[test]
fn a_local_video_opens_in_the_viewer_when_it_has_a_player() {
    let clip = Clip::new("inside");
    let pretend = Pretend::default();
    let h = shown(&pretend, &clip.target());
    let s = viewer_of(&handle(&h)).state();
    assert_eq!(
        (s["status"].as_str(), s["kind"].as_str()),
        (Some("shown"), Some("video")),
        "{s}"
    );
    assert_eq!(s["plays_video"], true);
    assert_eq!(s["video"]["backend"], "pretend");
    assert_eq!(pretend.log().opened, [(clip.0.clone(), 1.0)]);
}

#[test]
fn a_remote_video_stays_outside_with_a_player_in_the_viewer() {
    let pretend = Pretend::default();
    let app = plugins(Some(&pretend))
        .build(&egui::Context::default())
        .unwrap();
    let h = app.handle();
    let rec = RecordingOpener::default();
    links::links(h).unwrap().set_opener(Arc::new(rec.clone()));
    for target in ["https://example.org/clip.mp4", "https://youtu.be/abc"] {
        rec.clear();
        links::open(h, Request::new(target).source(Source::Declared));
        let records = rec.records();
        assert_eq!(records.len(), 1, "{target}: {records:?}");
        assert_eq!(records[0].how, How::Outside);
    }
    assert!(pretend.log().opened.is_empty());
    assert_eq!(viewer_of(h).state()["status"], "empty");
}

#[test]
fn without_a_player_a_local_video_goes_outside_after_the_usual_question() {
    let clip = Clip::new("outside");
    let app = plugins(None).build(&egui::Context::default()).unwrap();
    let h = app.handle();
    let v = viewer_of(h);
    if v.plays_video() {
        // This build has GStreamer: what is said here is of a viewer without.
        v.set_video_backend(None);
    }
    assert_eq!(v.state()["plays_video"], false);
    let rec = RecordingOpener::default();
    links::links(h).unwrap().set_opener(Arc::new(rec.clone()));
    links::open(h, Request::new(&clip.target()).source(Source::Declared));
    assert_eq!(links::recent(h).last().unwrap()["status"], "asked");
    assert_eq!(v.state()["status"], "empty");
}

#[test]
fn the_controls_are_named_and_drive_the_player() {
    let clip = Clip::new("controls");
    let pretend = Pretend::default();
    let mut h = shown(&pretend, &clip.target());
    let name = clip.0.file_name().unwrap().to_str().unwrap();
    h.get_by_label(&format!("Video: {name}"));
    h.get_by_label("Position");
    h.get_by_label("Volume");
    assert!(
        h.query_all_by_value("0:00 / 1:00").next().is_some(),
        "0:00 / 1:00"
    );

    h.get_by_role_and_label(Role::Button, "Play").click();
    h.step();
    h.step();
    assert_eq!(pretend.log().state, VideoState::Playing);
    h.get_by_role_and_label(Role::Button, "Pause").click();
    h.run();
    assert_eq!(pretend.log().state, VideoState::Paused);

    let app = handle(&h);
    let v = viewer_of(&app);
    v.video_seek_to(&app, 30.0).unwrap();
    h.run();
    assert_eq!(pretend.log().seeks, [Duration::from_secs(30)]);
    assert!(
        h.query_all_by_value("0:30 / 1:00").next().is_some(),
        "0:30 / 1:00"
    );
    // Kept within the video.
    v.video_seek_to(&app, 1000.0).unwrap();
    assert_eq!(pretend.log().position, LENGTH);
    v.video_seek_to(&app, -3.0).unwrap();
    assert_eq!(pretend.log().position, Duration::ZERO);
    v.video_volume(&app, 3.0).unwrap();
    assert_eq!(pretend.log().volume, 1.0);
    v.video_volume(&app, 0.25).unwrap();
    assert_eq!(pretend.log().volume, 0.25);
    assert_eq!(v.state()["video"]["volume"], 0.25);
}

#[test]
fn the_keys_work_while_the_picture_has_the_focus() {
    let clip = Clip::new("keys");
    let pretend = Pretend::default();
    let mut h = shown(&pretend, &clip.target());

    pretend.log().position = Duration::from_secs(20);
    h.run();
    h.key_press(egui::Key::ArrowRight);
    h.run();
    assert_eq!(pretend.log().position, Duration::from_secs(25));
    h.key_press(egui::Key::ArrowLeft);
    h.run();
    assert_eq!(pretend.log().position, Duration::from_secs(20));
    h.key_press(egui::Key::ArrowDown);
    h.run();
    assert_eq!(video(&h)["volume"], 0.95);
    h.key_press(egui::Key::ArrowUp);
    h.run();
    assert_eq!(video(&h)["volume"], 1.0);
    h.key_press(egui::Key::Space);
    h.step();
    h.step();
    assert_eq!(pretend.log().state, VideoState::Playing);
    h.key_press(egui::Key::Space);
    h.run();
    assert_eq!(pretend.log().state, VideoState::Paused);
}

#[test]
fn its_commands_refuse_without_a_video_page() {
    let app = plugins(Some(&Pretend::default()))
        .build(&egui::Context::default())
        .unwrap();
    let h = app.handle();
    assert!(viewer_of(h).video_toggle(h).is_err());
    assert_eq!(
        viewer_of(h).state()["video"],
        Value::Null,
        "no page, nothing to say"
    );
    assert!(h.command(&viewer::CMD_VIDEO_TOGGLE).is_some());
}

#[test]
fn only_a_playing_video_asks_for_repaints() {
    let clip = Clip::new("repaints");
    let pretend = Pretend::default();
    // `shown` ran until nothing asked for a pass: paused, it settles.
    let mut h = shown(&pretend, &clip.target());
    assert!(h.try_run().is_ok(), "paused");
    h.get_by_role_and_label(Role::Button, "Play").click();
    assert!(h.try_run().is_err(), "playing");
    h.get_by_role_and_label(Role::Button, "Pause").click();
    assert!(h.try_run().is_ok(), "paused again");
    pretend.log().state = VideoState::Ended;
    h.step();
    assert!(h.try_run().is_ok(), "ended");
}

#[test]
fn only_the_last_frame_is_uploaded() {
    let clip = Clip::new("frames");
    let pretend = Pretend::default();
    let mut h = shown(&pretend, &clip.target());
    assert_eq!(video(&h)["size"], Value::Null);
    pretend.frame(1);
    pretend.frame(2);
    pretend.frame(3);
    h.run();
    assert_eq!(pretend.log().frames_taken, 1);
    assert_eq!(video(&h)["size"], json!([4, 2]));
}

#[test]
fn leaving_the_page_stops_the_player() {
    let clip = Clip::new("leave");
    let pretend = Pretend::default();
    let mut h = shown(&pretend, &clip.target());
    assert_eq!(pretend.log().open, 1);
    let app = handle(&h);
    links::open(&app, Request::new("docs:other"));
    h.run();
    assert_eq!(pretend.log().open, 0);
    assert_eq!(viewer_of(&app).state()["video"], Value::Null);
    // Back on it: a new player, from the start.
    viewer_of(&app).back(&app);
    h.run();
    assert_eq!(pretend.log().open, 1);
    assert_eq!(pretend.log().opened.len(), 2);
}

#[test]
fn hiding_the_panel_stops_the_player_and_showing_it_comes_back_paused() {
    let clip = Clip::new("hide");
    let pretend = Pretend::default();
    let mut h = shown(&pretend, &clip.target());
    h.get_by_role_and_label(Role::Button, "Play").click();
    h.step();
    h.step();
    assert_eq!(pretend.log().state, VideoState::Playing);
    pretend.log().position = Duration::from_secs(12);
    h.step();

    let app = handle(&h);
    app.set_panel_visible(&PANEL, false);
    // No repaint is needed from anyone else for it to stop.
    let _ = h.try_run();
    assert_eq!(pretend.log().open, 0, "stopped");
    assert_eq!(video(&h)["stopped"], true);
    assert_eq!(video(&h)["position_ms"], 12000);
    assert!(viewer_of(&app).video_toggle(&app).is_err(), "says why");

    app.set_panel_visible(&PANEL, true);
    h.run();
    assert_eq!(pretend.log().open, 1);
    assert_eq!(pretend.log().position, Duration::from_secs(12));
    assert_eq!(pretend.log().state, VideoState::Paused);
    h.get_by_role_and_label(Role::Button, "Play");
}

#[test]
fn a_player_that_cannot_open_gives_a_page_with_open_outside() {
    let clip = Clip::new("refused");
    let pretend = Pretend::default();
    pretend.log().refuse = Some("a plugin is missing".into());
    let mut h = shown(&pretend, &clip.target());
    let s = viewer_of(&handle(&h)).state();
    assert_eq!(s["status"], "failed", "{s}");
    assert_eq!(s["error"]["failure"], "playback");
    assert_eq!(s["error"]["detail"], "a plugin is missing");
    assert!(
        h.query_all_by_value("This video cannot be played here")
            .next()
            .is_some(),
        "This video cannot be played here"
    );
    assert!(
        h.query_all_by_value("a plugin is missing").next().is_some(),
        "a plugin is missing"
    );
    h.get_by_role_and_label(Role::Button, "Retry");
    let rec = RecordingOpener::default();
    links::links(&handle(&h))
        .unwrap()
        .set_opener(Arc::new(rec.clone()));
    pretend.log().refuse = None;
    h.get_by_role_and_label(Role::Button, "Retry").click();
    h.run();
    assert_eq!(viewer_of(&handle(&h)).state()["kind"], "video");
}

#[test]
fn a_player_failing_while_it_plays_gives_the_same_page() {
    let clip = Clip::new("fails");
    let pretend = Pretend::default();
    let mut h = shown(&pretend, &clip.target());
    pretend.log().fail = Some("the stream broke".into());
    h.step();
    h.run();
    let s = viewer_of(&handle(&h)).state();
    assert_eq!(s["status"], "failed", "{s}");
    assert_eq!(s["error"]["detail"], "the stream broke");
    assert_eq!(s["video"], Value::Null);
    assert_eq!(pretend.log().open, 0);
    assert!(
        h.query_all_by_value("This video cannot be played here")
            .next()
            .is_some(),
        "This video cannot be played here"
    );
}

#[test]
fn a_file_that_is_not_there_is_not_given_to_the_player() {
    let pretend = Pretend::default();
    let app = plugins(Some(&pretend))
        .build(&egui::Context::default())
        .unwrap();
    let h = app.handle();
    let target = "file:///nonexistent/egui-pop/clip.webm";
    viewer_of(h).navigate(h, target, true);
    let s = viewer_of(h).state();
    assert_eq!(s["status"], "failed", "{s}");
    assert_eq!(s["error"]["failure"], "not_found");
    assert!(pretend.log().opened.is_empty());
}
