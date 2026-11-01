//! The viewer's video page over the real GStreamer, on a clip written at test
//! time (`write_test_clip`). Feature `video`: GStreamer's development packages
//! and plugins are needed; the sound goes to a `fakesink`, so no sound device
//! is. Everything waits with a deadline.

#![cfg(all(feature = "video", not(target_arch = "wasm32")))]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_pop_api::dialogs::DialogsPlugin;
use egui_pop_api::links::{self, LinksPlugin, Request};
use egui_pop_api::{App, AppHandle};
use egui_pop_views::viewer::{Docs, GStreamer, VIEWER, Viewer, ViewerPlugin, write_test_clip};
use serde_json::Value;

const DEADLINE: Duration = Duration::from_secs(30);

/// A file removed with the test.
struct Temp(PathBuf);

impl Temp {
    fn new(test: &str, extension: &str) -> Self {
        Self(std::env::temp_dir().join(format!(
            "egui-pop-viewer-gst-{}-{test}.{extension}",
            std::process::id()
        )))
    }

    fn target(&self) -> String {
        format!("file://{}", self.0.display())
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

type Shown<'a> = Harness<'a, Option<App>>;

fn shown<'a>(backend: GStreamer, target: &str) -> Shown<'a> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1000.0, 640.0))
        .build_ui_state(
            move |ui, app: &mut Option<App>| {
                let app = app.get_or_insert_with(|| {
                    App::builder()
                        .plugin(DialogsPlugin)
                        .plugin(LinksPlugin::default())
                        .plugin(
                            ViewerPlugin::default()
                                .docs(Docs::new().page("other", "# Other"))
                                .video(Arc::new(backend.clone())),
                        )
                        .build(ui.ctx())
                        .unwrap()
                });
                app.show(ui);
            },
            None,
        );
    h.step();
    links::open(&handle(&h), Request::new(target));
    h
}

fn handle(h: &Shown<'_>) -> AppHandle {
    h.state().as_ref().unwrap().handle().clone()
}

fn viewer(h: &Shown<'_>) -> Arc<Viewer> {
    handle(h).service(&VIEWER).unwrap()
}

/// Draws until the viewer's state says so; fails with that state at the deadline.
fn until(h: &mut Shown<'_>, what: &str, done: impl Fn(&Value) -> bool) -> Value {
    let start = Instant::now();
    loop {
        h.step();
        let state = viewer(h).state();
        if done(&state) {
            return state;
        }
        assert!(start.elapsed() < DEADLINE, "waiting for {what}: {state}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn millis(state: &Value, key: &str) -> i64 {
    state["video"][key].as_i64().unwrap_or(-1)
}

fn silent() -> GStreamer {
    GStreamer::default().audio_sink("fakesink")
}

#[test]
fn a_generated_clip_plays_pauses_seeks_ends_and_stops() {
    let clip = Temp::new("plays", "avi");
    write_test_clip(&clip.0, 2).unwrap();
    assert!(std::fs::metadata(&clip.0).unwrap().len() > 0);
    let mut h = shown(silent(), &clip.target());

    // Paused on its first picture, knowing its length and its size.
    let s = until(&mut h, "the first picture", |s| {
        s["video"]["state"] == "paused" && !s["video"]["size"].is_null()
    });
    assert_eq!(s["kind"], "video", "{s}");
    assert_eq!(s["video"]["backend"], "gstreamer");
    assert_eq!(s["video"]["size"], serde_json::json!([160, 120]), "{s}");
    let s = until(&mut h, "the length", |s| millis(s, "duration_ms") > 0);
    assert!((millis(&s, "duration_ms") - 2000).abs() < 300, "{s}");

    let app = handle(&h);
    let v = viewer(&h);

    // Plays: the position moves.
    v.video_toggle(&app).unwrap();
    let s = until(&mut h, "the position to move", |s| {
        millis(s, "position_ms") > 200
    });
    assert_eq!(s["video"]["state"], "playing", "{s}");

    // Paused: it no longer does.
    v.video_toggle(&app).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    let s = until(&mut h, "the pause", |s| s["video"]["state"] == "paused");
    let at = millis(&s, "position_ms");
    std::thread::sleep(Duration::from_millis(300));
    h.step();
    let later = millis(&viewer(&h).state(), "position_ms");
    assert!((later - at).abs() < 150, "{at} then {later}");

    // Seeks while paused.
    v.video_seek_to(&app, 1.0).unwrap();
    let s = until(&mut h, "the seek", |s| {
        (millis(s, "position_ms") - 1000).abs() < 150
    });
    assert_eq!(s["video"]["state"], "paused", "{s}");

    v.video_volume(&app, 0.5).unwrap();
    assert_eq!(v.state()["video"]["volume"], 0.5);

    // Plays to its end, where it stops by itself, and starts again from there.
    v.video_toggle(&app).unwrap();
    until(&mut h, "the end", |s| s["video"]["state"] == "ended");
    v.video_toggle(&app).unwrap();
    until(&mut h, "playing again", |s| {
        s["video"]["state"] == "playing" && millis(s, "position_ms") < 1500
    });

    // Leaving the page ends it.
    links::open(&app, Request::new("docs:other"));
    let s = until(&mut h, "the other page", |s| s["status"] == "shown");
    assert_eq!(s["video"], Value::Null, "{s}");
}

#[test]
fn a_file_that_is_no_video_gives_the_error_page() {
    let file = Temp::new("garbage", "mp4");
    std::fs::write(&file.0, vec![0x5a_u8; 64 * 1024]).unwrap();
    let mut h = shown(silent(), &file.target());
    let s = until(&mut h, "the failure", |s| s["status"] == "failed");
    assert_eq!(s["error"]["failure"], "playback", "{s}");
    assert_eq!(s["video"], Value::Null, "{s}");
}

#[test]
fn an_element_gstreamer_does_not_have_gives_the_error_page() {
    let clip = Temp::new("no-element", "avi");
    write_test_clip(&clip.0, 1).unwrap();
    let mut h = shown(
        GStreamer::default().audio_sink("no-such-element"),
        &clip.target(),
    );
    let s = until(&mut h, "the failure", |s| s["status"] == "failed");
    assert_eq!(s["error"]["failure"], "playback", "{s}");
    assert!(
        s["error"]["detail"]
            .as_str()
            .unwrap()
            .contains("no-such-element"),
        "{s}"
    );
}

#[test]
fn a_file_that_is_not_there_gives_the_not_found_page() {
    let file = Temp::new("missing", "mkv");
    let mut h = shown(silent(), &file.target());
    let s = until(&mut h, "the failure", |s| s["status"] == "failed");
    assert_eq!(s["error"]["failure"], "not_found", "{s}");
}
