//! The `Video` widget's GStreamer player on a clip written at test time
//! (`write_test_clip`): loops, and several widgets at once. Feature `video`:
//! GStreamer's development packages and plugins are needed; the sound goes to
//! a `fakesink`, so no sound device is. Everything waits with a deadline.

#![cfg(all(feature = "video", not(target_arch = "wasm32")))]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_pop_views::video::{
    Controls, GStreamer, Video, VideoBackend, VideoPlayer, VideoRequest, VideoState, Videos,
    write_test_clip,
};
use serde_json::Value;

const DEADLINE: Duration = Duration::from_secs(30);

/// A file removed with the test.
struct Temp(PathBuf);

impl Temp {
    fn clip(test: &str, seconds: u32) -> Self {
        let t = Self(std::env::temp_dir().join(format!(
            "egui-pop-video-gst-{}-{test}.avi",
            std::process::id()
        )));
        write_test_clip(&t.0, seconds).unwrap();
        t
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn silent() -> Arc<dyn VideoBackend> {
    Arc::new(GStreamer::default().audio_sink("fakesink"))
}

fn open(clip: &Temp) -> Box<dyn VideoPlayer> {
    silent()
        .open(&VideoRequest {
            path: clip.0.clone(),
            egui: egui::Context::default(),
            volume: 0.0,
        })
        .unwrap()
}

/// Plays `player` for `run`, looping the widget's way when `by_hand`: the
/// longest wait between two pictures, and the loops made.
fn play_and_time(player: &mut dyn VideoPlayer, run: Duration, by_hand: bool) -> (Duration, u64) {
    let start = Instant::now();
    let mut ready = false;
    while !ready {
        assert!(start.elapsed() < DEADLINE, "never ready");
        ready = player.poll().unwrap().state != VideoState::Loading;
        std::thread::sleep(Duration::from_millis(5));
    }
    let _ = player.frame();
    player.play().unwrap();
    let start = Instant::now();
    let (mut last, mut longest, mut loops) = (None::<Instant>, Duration::ZERO, 0);
    while start.elapsed() < run {
        let status = player.poll().unwrap();
        if by_hand && status.state == VideoState::Ended {
            loops += 1;
            player.seek(Duration::ZERO).unwrap();
            player.play().unwrap();
        }
        if player.frame().is_some() {
            let now = Instant::now();
            if let Some(l) = last {
                longest = longest.max(now - l);
            }
            last = Some(now);
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    (longest, loops + player.loops())
}

#[test]
fn a_segment_loop_goes_on_without_a_gap() {
    // Ten pictures a second: 100 ms between two.
    let clip = Temp::clip("loop", 1);
    let mut player = open(&clip);
    assert!(
        player.set_looping(true),
        "GStreamer's player loops by itself"
    );
    let (longest, loops) = play_and_time(player.as_mut(), Duration::from_millis(3500), false);
    eprintln!("segment loop: {loops} loops, longest wait between pictures {longest:?}");
    assert!(loops >= 2, "{loops} loops");
    assert!(longest < Duration::from_millis(250), "{longest:?}");
    assert_ne!(player.poll().unwrap().state, VideoState::Ended);

    // Turned off, the next end is an end.
    player.set_looping(false);
    let start = Instant::now();
    while player.poll().unwrap().state != VideoState::Ended {
        assert!(start.elapsed() < DEADLINE, "never ended");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_loop_by_seeking_back_is_measured_for_comparison() {
    let clip = Temp::clip("seek-loop", 1);
    let mut player = open(&clip);
    let (longest, loops) = play_and_time(player.as_mut(), Duration::from_millis(3500), true);
    eprintln!("seek-to-start loop: {loops} loops, longest wait between pictures {longest:?}");
    assert!(loops >= 2, "{loops} loops");
}

#[test]
fn several_widgets_play_each_on_its_own_pipeline() {
    let clip = Temp::clip("several", 2);
    let path = clip.0.clone();
    let mut h = Harness::builder()
        .with_size(egui::vec2(800.0, 600.0))
        .build_ui(move |ui| {
            for (id, controls) in [
                ("full", Controls::Full),
                ("compact", Controls::Compact),
                ("bare", Controls::None),
            ] {
                let bare = controls == Controls::None;
                let _ = Video::new(&path)
                    .id(id)
                    .controls(controls)
                    .autoplay(bare)
                    .looping(bare)
                    .muted(bare)
                    .backend(Some(silent()))
                    .size(egui::vec2(300.0, 160.0))
                    .show(ui);
            }
        });
    let state = |h: &Harness<'_>, id: &str| -> Value { Videos::of(&h.ctx).state(id).unwrap() };
    let start = Instant::now();
    loop {
        h.step();
        let ready = ["full", "compact", "bare"]
            .iter()
            .all(|id| !state(&h, id)["size"].is_null());
        if ready && state(&h, "bare")["position_ms"].as_u64() > Some(300) {
            break;
        }
        assert!(
            start.elapsed() < DEADLINE,
            "{}",
            Videos::of(&h.ctx).describe()
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(state(&h, "full")["state"], "paused");
    assert_eq!(state(&h, "compact")["state"], "paused");
    assert_eq!(state(&h, "bare")["state"], "playing");
    assert_eq!(state(&h, "full")["position_ms"], 0);
    assert_eq!(state(&h, "bare")["size"], serde_json::json!([160, 120]));
}
