//! The gallery's video story (feature `video`, GStreamer needed): the clip is
//! written on request and opens in the viewer as a video page, or, on a machine
//! that cannot play it, on the error page. Never a panic.

#![cfg(feature = "video")]

use std::time::{Duration, Instant};

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop::views::viewer::VIEWER;
use egui_pop_gallery::{Gallery, Options};
use egui_pop_test::SharedGpu as _;

const DEADLINE: Duration = Duration::from_secs(30);

#[test]
fn the_story_writes_a_clip_and_the_viewer_opens_it() {
    let options = Options {
        page: "viewer-video".to_string(),
        ..Options::default()
    };
    let mut h: Harness<'static, Gallery> = Harness::builder()
        .with_size(egui::vec2(1100.0, 720.0))
        .shared_wgpu()
        .build_eframe(move |cc| {
            Gallery::from_creation_context(cc, options).expect("the gallery builds")
        });
    h.run();
    h.get_by_role_and_label(Role::Button, "Write the clip")
        .click();
    let start = Instant::now();
    while h.query_by_role_and_label(Role::Link, "The clip").is_none() {
        assert!(start.elapsed() < DEADLINE, "the clip was not written");
        std::thread::sleep(Duration::from_millis(20));
        h.step();
    }
    h.get_by_role_and_label(Role::Link, "The clip").click();
    let app = h.state().app.handle().clone();
    let v = app.service(&VIEWER).unwrap();
    let start = Instant::now();
    loop {
        h.step();
        let s = v.state();
        if s["status"] == "failed" {
            assert_eq!(s["error"]["failure"], "playback", "{s}");
            break;
        }
        if s["kind"] == "video" && s["video"]["state"] == "paused" {
            break;
        }
        assert!(start.elapsed() < DEADLINE, "{s}");
        std::thread::sleep(Duration::from_millis(20));
    }
}
