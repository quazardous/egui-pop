//! The tracks page: a short edit at 25 frames per second on a timeline with
//! tracks of clips (titles, two video tracks, two audio tracks), drawn by a
//! painter of the gallery's: pretend thumbnails for video, a waveform for
//! audio, the plain painter's labels for titles. Each move, trim or removal
//! of a clip is a step of the application's history.

use std::cell::RefCell;
use std::rc::Rc;

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, Ui, pos2};
use egui_pop::api::stories::Story;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::model::ids::{PluginId, StoryId};
use egui_pop::theme::Tokens;
use egui_pop::views::timeline::{
    Clip, ClipPaint, ClipPainter, Marker, PlainClips, TimeScale, Timeline, TimelineState, Track,
};

use super::section;
use crate::shell::SHELL;

pub const TRACKS_PAGE: PluginId = PluginId::from_static("gallery.tracks");

/// The page's edit.
#[derive(Debug)]
pub struct State {
    pub edit: TimelineState,
}

impl Default for State {
    fn default() -> Self {
        // Forty seconds of video at 25 frames per second.
        let mut edit = TimelineState::new(TimeScale::frames(25), 40.0);
        edit.playhead = 9.0;
        edit.add_marker(Marker::new(0, 24.0, "Chorus"));
        let titles = edit.add_track(Track::new(0, "T1").kind("titles").height(28.0));
        let v2 = edit.add_track(Track::new(0, "V2").kind("video"));
        let v1 = edit.add_track(Track::new(0, "V1").kind("video"));
        let a1 = edit.add_track(Track::new(0, "A1").kind("audio"));
        let a2 = edit.add_track(Track::new(0, "A2").kind("audio"));
        let blue = Color32::from_rgb(0x3d, 0x7b, 0xc9);
        let violet = Color32::from_rgb(0x8a, 0x5c, 0xd6);
        let green = Color32::from_rgb(0x3f, 0x9e, 0x6a);
        let clips = [
            (titles, Clip::new(0, 1.0, 5.0, "Opening title")),
            (titles, Clip::new(0, 30.0, 6.0, "Credits")),
            (
                v1,
                Clip::new(0, 0.0, 12.0, "Wide shot")
                    .source("wide.mov")
                    .offset(3.0)
                    .source_length(20.0)
                    .color(blue),
            ),
            (
                v1,
                Clip::new(0, 12.0, 9.0, "Close-up")
                    .source("close.mov")
                    .source_length(15.0)
                    .color(blue),
            ),
            (
                v1,
                Clip::new(0, 23.0, 14.0, "Street")
                    .source("street.mov")
                    .offset(1.0)
                    .source_length(30.0)
                    .color(blue),
            ),
            (
                v2,
                Clip::new(0, 15.0, 6.0, "Overlay")
                    .source("overlay.mov")
                    .source_length(8.0)
                    .color(violet),
            ),
            (
                a1,
                Clip::new(0, 0.0, 21.0, "Dialogue")
                    .source("dialogue.wav")
                    .source_length(25.0)
                    .color(green),
            ),
            (
                a2,
                Clip::new(0, 4.0, 34.0, "Music")
                    .source("music.wav")
                    .offset(10.0)
                    .source_length(180.0)
                    .color(green),
            ),
        ];
        for (track, clip) in clips {
            edit.add_clip(track, clip).expect("the edit's clips fit");
        }
        Self { edit }
    }
}

/// The tracks page.
#[derive(Debug, Default)]
pub struct TracksPage;

impl Plugin for TracksPage {
    fn id(&self) -> PluginId {
        TRACKS_PAGE
    }

    fn title(&self) -> String {
        "Tracks and clips".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let state = Rc::new(RefCell::new(State::default()));
        ctx.story(
            Story::new(StoryId::from_static("tracks"), "Tracks and clips", move |ui, app| {
                show(ui, app, &mut state.borrow_mut());
            })
            .description("A timeline with tracks of clips: a painter of the application's draws their content (thumbnails, a waveform), a click selects, a drag moves them along a track or to another, their edges trim them, each change one step of the history.")
            .group("Components", 82),
        )?;
        Ok(())
    }
}

pub fn show(ui: &mut Ui, app: &AppHandle, state: &mut State) {
    ui.label("Click a clip to select it (Ctrl: one more or one less, Shift: one more), drag it along its track or to another, drag its edges to trim it; clips snap to one another's edges, the markers and the playhead (Alt: no snapping). With the timeline focused: Page Up and Page Down walk a track's clips, Shift+Up and Shift+Down reach the clip at the playhead on the track above or below, Ctrl+Left and Ctrl+Right move the selection, Ctrl+Up and Ctrl+Down to another track, [ and ] trim it to the playhead, Delete removes it. Each change is a step of the history (Ctrl+Z).");

    section(ui, "An edit at 25 frames per second");
    let tl = &state.edit;
    let t = Tokens::of(ui.ctx());
    ui.horizontal(|ui| {
        ui.monospace(tl.scale.format(tl.playhead));
        let selected: Vec<&str> = tl
            .selected_clips
            .iter()
            .filter_map(|id| tl.clip(*id).map(|c| c.label.as_str()))
            .collect();
        let said = if selected.is_empty() {
            "no clip selected".to_string()
        } else {
            format!("selected: {}", selected.join(", "))
        };
        ui.colored_label(t.text.muted, said);
    });
    let mut painter = EditPainter;
    Timeline::new(&mut state.edit)
        .id("edit")
        .label("Edit")
        .app(app)
        .history("gallery/timeline/edit")
        .clips(&mut painter)
        .show(ui);
}

/// Draws video clips as thumbnails, audio clips as a waveform, anything
/// else as the plain painter does. Both come from the source's time, so a
/// clip trimmed or moved keeps its picture and its sound in place.
#[derive(Debug, Default)]
pub struct EditPainter;

impl ClipPainter for EditPainter {
    fn paint(&mut self, painter: &Painter, clip: &ClipPaint<'_>) {
        match clip.track.kind.as_str() {
            "video" => thumbnails(painter, clip),
            "audio" => waveform(painter, clip),
            _ => {}
        }
        PlainClips.paint(painter, clip);
    }
}

/// A number from 0 to 1 for a source and a step in it.
fn noise(source: &str, step: i64) -> f32 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in source.bytes().chain(step.to_le_bytes()) {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    (h >> 40) as f32 / (1u64 << 24) as f32
}

/// A frame every second of the source: a sky over a ground, both shades of
/// the clip's colour.
fn thumbnails(painter: &Painter, clip: &ClipPaint<'_>) {
    let base = clip.clip.color.unwrap_or(clip.tokens.clip_fill);
    let r = clip.content;
    let (from, to) = clip.source_in_view();
    let first = from.floor() as i64;
    let last = to.ceil() as i64;
    if last - first > 400 {
        return;
    }
    for s in first..last {
        let a = clip.x_of_source(s as f64);
        let b = clip.x_of_source((s + 1) as f64);
        let frame = Rect::from_x_y_ranges(a + 1.0..=b - 1.0, r.top()..=r.bottom());
        if frame.width() < 2.0 {
            continue;
        }
        let n = noise(&clip.clip.source, s);
        let horizon = frame.top() + frame.height() * (0.35 + 0.3 * n);
        painter.rect_filled(
            Rect::from_x_y_ranges(frame.x_range(), frame.top()..=horizon),
            1.0,
            base.gamma_multiply(0.55 + 0.35 * n),
        );
        painter.rect_filled(
            Rect::from_x_y_ranges(frame.x_range(), horizon..=frame.bottom()),
            1.0,
            base.gamma_multiply(0.95),
        );
    }
}

/// An envelope of the source, a point every two points across.
fn waveform(painter: &Painter, clip: &ClipPaint<'_>) {
    let r = clip.content;
    let color = clip.clip.color.unwrap_or(clip.tokens.clip_text);
    let mid = r.center().y;
    let half = r.height() / 2.0 - 1.0;
    let mut top: Vec<Pos2> = Vec::new();
    let mut x = clip.visible.left();
    while x <= clip.visible.right() {
        let s = clip.source_at(x);
        let beat = (s * 2.0).fract() as f32;
        let level = 0.25
            + 0.45 * noise(&clip.clip.source, (s * 4.0).floor() as i64)
            + 0.3 * (1.0 - beat).powi(3);
        top.push(pos2(x, mid - half * level.min(1.0)));
        x += 2.0;
    }
    if top.len() < 2 {
        return;
    }
    let mut outline = top.clone();
    outline.extend(top.iter().rev().map(|p| pos2(p.x, 2.0 * mid - p.y)));
    painter.add(Shape::line(outline, Stroke::new(1.0, color)));
    painter.line_segment(
        [
            pos2(clip.visible.left(), mid),
            pos2(clip.visible.right(), mid),
        ],
        Stroke::new(1.0, color.gamma_multiply(0.6)),
    );
}
