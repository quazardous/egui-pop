//! The `Video` widget over a pretend player, so these run in every build: its
//! three sets of controls, the placeholder without a player, the keys, its
//! accessible node, its state through the debug bridge and the `video.*`
//! commands, loops, several videos at once, and a video going out of view.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::App;
use egui_pop_views::video::{
    Controls, Video, VideoBackend, VideoFrame, VideoPlayer, VideoPlugin, VideoRequest, VideoState,
    VideoStatus, Videos,
};
use serde_json::{Value, json};

const LENGTH: Duration = Duration::from_secs(60);

/// What one pretend player was asked, and what it answers.
#[derive(Default)]
struct Log {
    opened: usize,
    open: bool,
    volume_at_open: f32,
    state: VideoState,
    position: Duration,
    volume: f32,
    seeks: Vec<Duration>,
    frame: bool,
    /// It loops by itself when asked.
    native_loop: bool,
    looping: bool,
}

/// Players by file name.
#[derive(Clone, Default)]
struct Pretend(Arc<Mutex<BTreeMap<String, Log>>>);

impl Pretend {
    fn with<R>(&self, file: &str, f: impl FnOnce(&mut Log) -> R) -> R {
        f(self.0.lock().unwrap().entry(file.to_string()).or_default())
    }

    fn state(&self, file: &str) -> VideoState {
        self.with(file, |l| l.state)
    }

    fn open(&self) -> usize {
        self.0.lock().unwrap().values().filter(|l| l.open).count()
    }
}

struct Player(Pretend, String);

impl VideoPlayer for Player {
    fn poll(&mut self) -> Result<VideoStatus, String> {
        Ok(self.0.with(&self.1, |l| VideoStatus {
            state: l.state,
            position: Some(l.position),
            duration: Some(LENGTH),
            note: None,
        }))
    }

    fn frame(&mut self) -> Option<VideoFrame> {
        self.0.with(&self.1, |l| {
            std::mem::take(&mut l.frame).then(|| VideoFrame {
                image: Arc::new(egui::ColorImage::filled([4, 2], egui::Color32::GRAY)),
                aspect: 2.0,
            })
        })
    }

    fn play(&mut self) -> Result<(), String> {
        self.0.with(&self.1, |l| l.state = VideoState::Playing);
        Ok(())
    }

    fn pause(&mut self) -> Result<(), String> {
        self.0.with(&self.1, |l| l.state = VideoState::Paused);
        Ok(())
    }

    fn seek(&mut self, to: Duration) -> Result<(), String> {
        self.0.with(&self.1, |l| {
            l.position = to;
            l.seeks.push(to);
        });
        Ok(())
    }

    fn set_volume(&mut self, volume: f32) {
        self.0.with(&self.1, |l| l.volume = volume);
    }

    fn set_looping(&mut self, on: bool) -> bool {
        self.0.with(&self.1, |l| {
            l.looping = on;
            l.native_loop
        })
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.0.with(&self.1, |l| l.open = false);
    }
}

impl VideoBackend for Pretend {
    fn name(&self) -> &str {
        "pretend"
    }

    fn open(&self, request: &VideoRequest) -> Result<Box<dyn VideoPlayer>, String> {
        let file = request
            .path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        self.with(&file, |l| {
            l.opened += 1;
            l.open = true;
            l.volume_at_open = request.volume;
            l.volume = request.volume;
            l.state = VideoState::Paused;
            l.frame = true;
        });
        Ok(Box::new(Player(self.clone(), file)))
    }
}

fn backend(p: &Pretend) -> Option<Arc<dyn VideoBackend>> {
    Some(Arc::new(p.clone()))
}

/// One video, drawn as `draw` says.
fn harness<'a>(draw: impl FnMut(&mut egui::Ui) + 'a) -> Harness<'a> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(640.0, 520.0))
        .build_ui(draw);
    h.run();
    h
}

fn videos(h: &Harness<'_>) -> Videos {
    Videos::of(&h.ctx)
}

#[test]
fn full_controls_name_and_drive_the_player() {
    let p = Pretend::default();
    let b = backend(&p);
    let mut h = harness(|ui| {
        let _ = Video::new("/clips/tour.mp4")
            .id("tour")
            .backend(b.clone())
            .show(ui);
    });
    h.get_by_role_and_label(Role::Video, "Video: tour.mp4");
    h.get_by_label("Position");
    h.get_by_label("Volume");
    assert!(h.query_all_by_value("0:00 / 1:00").next().is_some());
    h.get_by_role_and_label(Role::Button, "Play").click();
    h.run();
    assert_eq!(p.state("tour.mp4"), VideoState::Playing);
    h.get_by_role_and_label(Role::Button, "Pause").click();
    h.run();
    assert_eq!(p.state("tour.mp4"), VideoState::Paused);
    // A click on the picture plays too.
    h.get_by_role_and_label(Role::Video, "Video: tour.mp4")
        .click();
    h.run();
    assert_eq!(p.state("tour.mp4"), VideoState::Playing);
    let s = videos(&h).state("tour").unwrap();
    assert_eq!(s["size"], json!([4, 2]), "{s}");
    assert_eq!(s["controls"], "full");
}

#[test]
fn compact_controls_have_no_volume_and_bare_ones_nothing() {
    let p = Pretend::default();
    let b = backend(&p);
    let h = harness(|ui| {
        let _ = Video::new("/clips/compact.mp4")
            .controls(Controls::Compact)
            .backend(b.clone())
            .show(ui);
    });
    h.get_by_role_and_label(Role::Button, "Play");
    h.get_by_label("Position");
    assert!(h.query_by_label("Volume").is_none());
    assert!(h.query_all_by_value("0:00 / 1:00").next().is_some());
    egui_pop_test::assert_all_named(&h);

    let b = backend(&p);
    let mut h = harness(|ui| {
        let _ = Video::new("/clips/bare.mp4")
            .controls(Controls::None)
            .backend(b.clone())
            .show(ui);
    });
    assert!(h.query_by_role(Role::Button).is_none());
    assert!(h.query_by_label("Position").is_none());
    // Not a Tab stop, and a click does nothing.
    h.key_press(egui::Key::Tab);
    h.run();
    assert_eq!(h.ctx.memory(|m| m.focused()), None);
    h.get_by_role_and_label(Role::Video, "Video: bare.mp4")
        .click();
    h.run();
    assert_eq!(p.state("bare.mp4"), VideoState::Paused);
}

#[test]
fn a_bare_preview_plays_at_once_silently_and_loops() {
    let p = Pretend::default();
    let b = backend(&p);
    let mut h = harness(|ui| {
        let _ = Video::new("/clips/loop.mp4")
            .id("loop")
            .controls(Controls::None)
            .autoplay(true)
            .looping(true)
            .muted(true)
            .backend(b.clone())
            .show(ui);
    });
    assert_eq!(p.state("loop.mp4"), VideoState::Playing);
    assert_eq!(p.with("loop.mp4", |l| l.volume_at_open), 0.0);
    assert!(p.with("loop.mp4", |l| l.looping), "the player was asked");
    // This player cannot loop by itself: at the end, the widget goes back.
    p.with("loop.mp4", |l| {
        l.state = VideoState::Ended;
        l.position = LENGTH;
    });
    h.step();
    h.step();
    assert_eq!(p.state("loop.mp4"), VideoState::Playing);
    assert_eq!(p.with("loop.mp4", |l| l.position), Duration::ZERO);
    let s = videos(&h).state("loop").unwrap();
    assert_eq!(
        (s["loops"].as_u64(), s["muted"].as_bool()),
        (Some(1), Some(true))
    );
    // Unmuted from the commands, at its volume.
    videos(&h).set_muted(Some("loop"), Some(false)).unwrap();
    assert_eq!(p.with("loop.mp4", |l| l.volume), 1.0);
}

#[test]
fn a_player_that_loops_by_itself_is_left_to_it() {
    let p = Pretend::default();
    p.with("native.mp4", |l| l.native_loop = true);
    let b = backend(&p);
    let mut h = harness(|ui| {
        let _ = Video::new("/clips/native.mp4")
            .id("native")
            .autoplay(true)
            .looping(true)
            .backend(b.clone())
            .show(ui);
    });
    p.with("native.mp4", |l| l.state = VideoState::Ended);
    h.step();
    h.step();
    assert!(p.with("native.mp4", |l| l.seeks.is_empty()), "no seek");
}

#[test]
fn without_a_player_a_placeholder_says_why_and_opens_outside() {
    let outside = Arc::new(Mutex::new(false));
    let o = outside.clone();
    let mut h = harness(move |ui| {
        let out = Video::new("/clips/none.mp4")
            .id("none")
            .backend(None)
            .show(ui);
        assert!(out.unavailable);
        if out.open_outside {
            *o.lock().unwrap() = true;
        }
    });
    h.get_by_role_and_label(Role::Video, "Video: none.mp4");
    h.get_by_label("This application has no video player");
    assert!(h.query_by_label("Play").is_none());
    h.get_by_role_and_label(Role::Button, "Open outside")
        .click();
    h.run();
    assert!(*outside.lock().unwrap());
    assert!(videos(&h).toggle(Some("none")).is_err());
}

#[cfg(not(feature = "video"))]
#[test]
fn a_build_without_the_feature_draws_the_placeholder() {
    let h = harness(|ui| {
        let _ = Video::new("/clips/default.mp4").show(ui);
    });
    h.get_by_label("This application has no video player");
}

#[test]
fn the_keys_work_while_the_picture_has_the_focus() {
    let p = Pretend::default();
    let b = backend(&p);
    let mut h = harness(|ui| {
        let _ = Video::new("/clips/keys.mp4")
            .id("keys")
            .backend(b.clone())
            .show(ui);
    });
    p.with("keys.mp4", |l| l.position = Duration::from_secs(20));
    h.run();
    h.get_by_role_and_label(Role::Video, "Video: keys.mp4")
        .focus();
    h.run();
    h.key_press(egui::Key::ArrowRight);
    h.run();
    assert_eq!(p.with("keys.mp4", |l| l.position), Duration::from_secs(25));
    h.key_press(egui::Key::ArrowLeft);
    h.run();
    assert_eq!(p.with("keys.mp4", |l| l.position), Duration::from_secs(20));
    h.key_press(egui::Key::ArrowDown);
    h.run();
    assert_eq!(videos(&h).state("keys").unwrap()["volume"], 0.95);
    h.key_press(egui::Key::ArrowUp);
    h.run();
    assert_eq!(videos(&h).state("keys").unwrap()["volume"], 1.0);
    h.key_press(egui::Key::Space);
    h.run();
    assert_eq!(p.state("keys.mp4"), VideoState::Playing);
    h.key_press(egui::Key::Space);
    h.run();
    assert_eq!(p.state("keys.mp4"), VideoState::Paused);
}

#[test]
fn a_label_names_the_video_for_screen_readers() {
    let p = Pretend::default();
    let b = backend(&p);
    let h = harness(|ui| {
        let _ = Video::new("/clips/a11y.mp4")
            .label("Product tour")
            .backend(b.clone())
            .show(ui);
    });
    h.get_by_role_and_label(Role::Video, "Product tour");
    egui_pop_test::assert_all_named(&h);
}

#[test]
fn several_videos_play_each_on_its_own_player() {
    let p = Pretend::default();
    let b = backend(&p);
    let mut h = harness(|ui| {
        for (id, file) in [("a", "a.mp4"), ("b", "b.mp4"), ("c", "c.mp4")] {
            let _ = Video::new(Path::new("/clips").join(file))
                .id(id)
                .controls(Controls::Compact)
                .size(egui::vec2(300.0, 140.0))
                .backend(b.clone())
                .show(ui);
        }
    });
    assert_eq!(p.open(), 3);
    let v = videos(&h);
    assert_eq!(v.ids(), ["a", "b", "c"]);
    v.toggle(Some("b")).unwrap();
    v.seek_to(Some("c"), 30.0).unwrap();
    h.run();
    assert_eq!(
        [p.state("a.mp4"), p.state("b.mp4"), p.state("c.mp4")],
        [VideoState::Paused, VideoState::Playing, VideoState::Paused]
    );
    assert_eq!(p.with("c.mp4", |l| l.position), Duration::from_secs(30));
    assert!(p.with("a.mp4", |l| l.seeks.is_empty()));
    // Without `video`, nobody had the focus: which one is unsaid.
    assert!(v.toggle(None).is_err());
}

#[test]
fn a_video_out_of_view_stops_and_comes_back_where_it_was() {
    let p = Pretend::default();
    let b = backend(&p);
    let shown = Arc::new(Mutex::new(true));
    let s = shown.clone();
    let mut h = harness(move |ui| {
        if *s.lock().unwrap() {
            let _ = Video::new("/clips/hide.mp4")
                .id("hide")
                .backend(b.clone())
                .show(ui);
        }
    });
    videos(&h).toggle(Some("hide")).unwrap();
    p.with("hide.mp4", |l| l.position = Duration::from_secs(12));
    h.step();
    *shown.lock().unwrap() = false;
    // Nothing else asks for passes: the sweep keeps them coming.
    let _ = h.try_run();
    assert_eq!(p.open(), 0, "stopped");
    let s = videos(&h).state("hide").unwrap();
    assert_eq!(
        (s["stopped"].as_bool(), s["position_ms"].as_u64()),
        (Some(true), Some(12000))
    );
    assert!(videos(&h).toggle(Some("hide")).is_err(), "says why");

    *shown.lock().unwrap() = true;
    h.run();
    assert_eq!(p.open(), 1);
    assert_eq!(p.with("hide.mp4", |l| l.opened), 2);
    assert_eq!(p.with("hide.mp4", |l| l.position), Duration::from_secs(12));
    assert_eq!(p.state("hide.mp4"), VideoState::Paused);
}

#[test]
fn agents_read_videos_through_the_bridge_and_drive_them_with_commands() {
    let p = Pretend::default();
    let b = backend(&p);
    let mut h = Harness::builder()
        .with_size(egui::vec2(640.0, 520.0))
        .build_ui_state(
            move |ui, app: &mut Option<App>| {
                let app = app.get_or_insert_with(|| {
                    App::builder().plugin(VideoPlugin).build(ui.ctx()).unwrap()
                });
                app.show(ui);
                let _ = Video::new("/clips/agent.mp4")
                    .id("intro")
                    .backend(b.clone())
                    .app(app.handle())
                    .show(ui);
            },
            None,
        );
    h.run();
    let app = h.state().as_ref().unwrap().handle().clone();
    let snap = app.debug_snapshot();
    let intro = &snap["videos"]["videos"]["intro"];
    assert_eq!(intro["state"], "paused", "{snap}");
    assert_eq!(intro["duration_ms"], 60_000);
    assert_eq!(intro["backend"], "pretend");

    // An agent's command runs at the application's next pass.
    let mut run = |id: &str, args: Value| {
        let f = app.debug_call("commands", "run", &json!({ "id": id, "args": args }));
        assert!(f.ok, "{id}: {f:?}");
        h.run();
    };
    run("video.play", json!({ "video": "intro" }));
    assert_eq!(p.state("agent.mp4"), VideoState::Playing);
    run("video.seek", json!({ "seconds": 42 }));
    assert_eq!(p.with("agent.mp4", |l| l.position), Duration::from_secs(42));
    run("video.seek", json!({ "by": -2 }));
    assert_eq!(p.with("agent.mp4", |l| l.position), Duration::from_secs(40));
    run("video.volume", json!({ "volume": 0.4 }));
    assert_eq!(p.with("agent.mp4", |l| l.volume), 0.4);
    run("video.mute", json!({}));
    assert_eq!(p.with("agent.mp4", |l| l.volume), 0.0);
    run("video.pause", json!({ "video": "intro" }));
    assert_eq!(p.state("agent.mp4"), VideoState::Paused);
    let intro = app.debug_snapshot()["videos"]["videos"]["intro"].clone();
    assert_eq!(
        (intro["position_ms"].as_u64(), intro["muted"].as_bool()),
        (Some(40_000), Some(true)),
        "{intro}"
    );
    // A video nobody drew: refused, and said so.
    assert!(Videos::of(&h.ctx).play(Some("nope")).is_err());
}
