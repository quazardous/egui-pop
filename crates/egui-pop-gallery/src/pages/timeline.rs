//! The timeline page: one timeline per time scale (seconds, a timecode at 25
//! frames per second, bars and beats at 128 BPM), each with markers and a
//! range, and a pretend clock that moves the playhead while one plays (the
//! motion clock, frozen in the tests). Each change of a document is a step
//! of the application's history.

use std::cell::RefCell;
use std::rc::Rc;

use egui::{Color32, Stroke, Ui, pos2};
use egui_pop::api::stories::Story;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::core::motion;
use egui_pop::model::ids::{PluginId, StoryId};
use egui_pop::theme::Tokens;
use egui_pop::views::timeline::{
    Lanes, Marker, MarkerKind, PlayState, Snap, Tempo, TimeRange, TimeScale, TimeView, Timeline,
    TimelineState,
};

use super::section;
use crate::shell::SHELL;

pub const TIMELINE_PAGE: PluginId = PluginId::from_static("gallery.timeline");

/// The page's three timelines and the clock's last reading.
#[derive(Debug)]
pub struct State {
    pub seconds: TimelineState,
    pub timecode: TimelineState,
    pub beats: TimelineState,
    last: Option<f64>,
}

impl Default for State {
    fn default() -> Self {
        // A recording of 90 seconds, as a data viewer shows it.
        let mut seconds = TimelineState::new(TimeScale::Seconds, 90.0);
        seconds.playhead = 12.0;
        seconds.add_marker(Marker::new(0, 5.0, "Start"));
        seconds.add_marker(Marker::new(0, 42.5, "Peak"));
        seconds.range = Some(TimeRange::new(20.0, 35.0));

        // Two minutes of video at 25 frames per second.
        let mut timecode = TimelineState::new(TimeScale::frames(25), 120.0);
        timecode.playhead = 50.0;
        timecode.add_marker(Marker::new(0, 31.2, "Scene 2"));
        timecode.add_marker(Marker::new(0, 64.0, "Cut"));
        timecode.range = Some(TimeRange::new(48.0, 56.0));
        timecode.looping = true;

        // A track at 128 BPM: 64 bars, cue points and a loop of four bars.
        let tempo = Tempo::new(128.0, 4, 4);
        let bar = tempo.bar();
        let mut beats = TimelineState::new(TimeScale::Beats(tempo), 64.0 * bar);
        beats.view = TimeView::new(0.0, 48.0 * bar);
        beats.playhead = 33.5 * bar;
        for (at, label, color) in [
            (0.0, "Intro", Color32::from_rgb(0x3d, 0x9b, 0xe9)),
            (16.0, "Build", Color32::from_rgb(0xe9, 0xa2, 0x3d)),
            (32.0, "Drop", Color32::from_rgb(0xe9, 0x4b, 0x3d)),
        ] {
            beats.add_marker(
                Marker::new(0, at * bar, label)
                    .kind(MarkerKind::Cue)
                    .color(color),
            );
        }
        beats.range = Some(TimeRange::new(32.0 * bar, 36.0 * bar));
        beats.looping = true;
        Self {
            seconds,
            timecode,
            beats,
            last: None,
        }
    }
}

/// The timeline page.
#[derive(Debug, Default)]
pub struct TimelinePage;

impl Plugin for TimelinePage {
    fn id(&self) -> PluginId {
        TIMELINE_PAGE
    }

    fn title(&self) -> String {
        "Timeline".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let state = Rc::new(RefCell::new(State::default()));
        ctx.story(
            Story::new(StoryId::from_static("timeline"), "Timeline", move |ui, app| {
                show(ui, app, &mut state.borrow_mut());
            })
            .description("Timelines over seconds, a 25 fps timecode and bars and beats at 128 BPM: an adaptive ruler, a playhead to scrub, a range that loops, markers and cue points, snapping, the J/K/L transport keys, each change of a document one step of the history.")
            .group("Components", 81),
        )?;
        Ok(())
    }
}

pub fn show(ui: &mut Ui, app: &AppHandle, state: &mut State) {
    let ctx = ui.ctx().clone();
    // The application moves the time: a pretend player on the motion clock.
    let now = motion::now(&ctx);
    let dt = state.last.map_or(0.0, |last| (now - last).max(0.0));
    state.last = Some(now);
    let mut playing = false;
    for tl in [&mut state.seconds, &mut state.timecode, &mut state.beats] {
        tl.advance(dt);
        playing |= tl.is_playing();
    }
    // Playing is no decoration: it goes on under reduced motion, and only a
    // frozen clock (the tests) stops it.
    if playing && !motion::controlled(&ctx) {
        ctx.request_repaint();
    }

    ui.label("Click a timeline, then: Space plays or pauses; J, K and L shuttle (J or L again doubles the speed); Left and Right step a unit, with Shift a bigger one; Home and End; I and O set the range, Ctrl+L loops it, Alt+X clears it; M adds a marker, Up and Down go from one to the next, Ctrl+Left and Ctrl+Right move it, Delete removes it. Drag on the ruler to scrub, Shift+drag for a range, Ctrl+drag or the wheel to pan, Ctrl+wheel to zoom; Alt turns snapping off. Each change of the markers or the range is a step of the history (Ctrl+Z).");

    section(ui, "Seconds: a recording, as a data viewer shows it");
    row(ui, &state.seconds);
    Timeline::new(&mut state.seconds)
        .id("seconds")
        .label("Recording")
        .app(app)
        .history("gallery/timeline/seconds")
        .snap(Snap {
            grid: false,
            ..Snap::default()
        })
        .lanes(36.0)
        .show_with(ui, events);

    section(ui, "Timecode at 25 frames per second");
    row(ui, &state.timecode);
    Timeline::new(&mut state.timecode)
        .id("timecode")
        .label("Video")
        .app(app)
        .history("gallery/timeline/timecode")
        .show(ui);

    section(ui, "Bars and beats at 128 BPM");
    row(ui, &state.beats);
    let tempo = match state.beats.scale {
        TimeScale::Beats(t) => t,
        _ => Tempo::default(),
    };
    Timeline::new(&mut state.beats)
        .id("beats")
        .label("Track")
        .app(app)
        .history("gallery/timeline/beats")
        .lanes(36.0)
        .show_with(ui, |ui, lanes| kicks(ui, lanes, &tempo));
}

/// The time and what the timeline asks for, above it.
fn row(ui: &mut Ui, tl: &TimelineState) {
    let t = Tokens::of(ui.ctx());
    ui.horizontal(|ui| {
        ui.monospace(tl.scale.format(tl.playhead));
        let asked = match tl.play {
            PlayState::Playing if tl.speed != 1.0 => format!("playing at {}×", tl.speed),
            PlayState::Playing => "playing".into(),
            PlayState::Paused => "paused".into(),
        };
        ui.colored_label(t.text.muted, asked);
        if let Some(r) = tl.range {
            let looping = if tl.looping { ", looping" } else { "" };
            ui.colored_label(
                t.text.muted,
                format!(
                    "range {} to {}{looping}",
                    tl.scale.format(r.start),
                    tl.scale.format(r.end)
                ),
            );
        }
    });
}

/// A recording's events: a tick where each stream logged something.
fn events(ui: &mut Ui, lanes: &Lanes) {
    let t = Tokens::of(ui.ctx());
    let p = ui.painter();
    let rows = 3;
    let h = lanes.rect.height() / rows as f32;
    for row in 0..rows {
        let top = lanes.rect.top() + h * row as f32;
        let every = [0.7, 1.3, 2.9][row];
        let mut at = 0.35 * row as f64;
        while at <= 90.0 {
            if lanes.view.contains(at) {
                let x = lanes.x_of(at);
                p.line_segment(
                    [pos2(x, top + 3.0), pos2(x, top + h - 3.0)],
                    Stroke::new(1.0, t.text.muted),
                );
            }
            at += every;
        }
    }
}

/// A kick on every beat, louder on the bar's first.
fn kicks(ui: &mut Ui, lanes: &Lanes, tempo: &Tempo) {
    let t = Tokens::of(ui.ctx());
    let p = ui.painter();
    let beat = tempo.beat();
    let first = ((lanes.view.start - tempo.offset) / beat).floor().max(0.0) as i64;
    let last = ((lanes.view.end() - tempo.offset) / beat).ceil() as i64;
    let mid = lanes.rect.center().y;
    for k in first..=last {
        let at = tempo.offset + k as f64 * beat;
        let x = lanes.x_of(at);
        let loud = k % i64::from(tempo.beats_per_bar.max(1)) == 0;
        let half = lanes.rect.height() * if loud { 0.4 } else { 0.22 };
        p.line_segment(
            [pos2(x, mid - half), pos2(x, mid + half)],
            Stroke::new(2.0, t.text.muted),
        );
    }
}
