//! The video editor page: a sequence at 25 frames per second (a titles
//! track, two video tracks, two audio tracks) on the timeline, a pretend
//! program monitor showing the frame under the playhead, and the editing
//! commands of video editors: split at the playhead, ripple delete, in and
//! out marks, J/K/L. Nothing plays: the clock is the motion clock, frozen
//! in the tests. Each change is a step of the application's history.

use std::cell::RefCell;
use std::rc::Rc;

use egui::{Align2, Color32, FontId, Rect, Sense, Stroke, StrokeKind, Ui, vec2};
use egui_pop::api::stories::Story;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::core::motion;
use egui_pop::model::ids::{PluginId, StoryId};
use egui_pop::theme::Tokens;
use egui_pop::views::timeline::{
    CMD_RIPPLE_DELETE, CMD_SET_RANGE, CMD_SPLIT_CLIP, Clip, Marker, PlayState, TimeRange,
    TimeScale, Timeline, TimelineState, Track,
};
use serde_json::json;

use super::section;
use super::tracks::{EditPainter, noise};
use crate::shell::SHELL;

pub const EDITOR_PAGE: PluginId = PluginId::from_static("gallery.editor");

/// The timeline's id, for agents and the page's buttons.
pub const SEQUENCE: &str = "sequence";

/// The page's sequence and the clock's last reading.
#[derive(Debug)]
pub struct State {
    pub seq: TimelineState,
    last: Option<f64>,
}

impl Default for State {
    fn default() -> Self {
        // A minute at 25 frames per second.
        let mut seq = TimelineState::new(TimeScale::frames(25), 60.0);
        seq.playhead = 14.0;
        seq.range = Some(TimeRange::new(8.0, 20.0));
        seq.add_marker(Marker::new(0, 34.0, "Music cue"));
        let titles = seq.add_track(Track::new(0, "T1").kind("titles").height(28.0));
        let v2 = seq.add_track(Track::new(0, "V2").kind("video"));
        let v1 = seq.add_track(Track::new(0, "V1").kind("video"));
        let a1 = seq.add_track(Track::new(0, "A1").kind("audio"));
        let a2 = seq.add_track(Track::new(0, "A2").kind("audio"));
        let blue = Color32::from_rgb(0x3d, 0x7b, 0xc9);
        let teal = Color32::from_rgb(0x2f, 0x9a, 0x9e);
        let violet = Color32::from_rgb(0x8a, 0x5c, 0xd6);
        let green = Color32::from_rgb(0x3f, 0x9e, 0x6a);
        let clips = [
            (titles, Clip::new(0, 2.0, 6.0, "Lower third: Ana")),
            (
                v1,
                Clip::new(0, 0.0, 18.0, "Interview")
                    .source("interview.mov")
                    .offset(12.0)
                    .source_length(300.0)
                    .color(blue),
            ),
            (
                v1,
                Clip::new(0, 18.0, 10.0, "Workshop")
                    .source("workshop.mov")
                    .offset(4.0)
                    .source_length(40.0)
                    .color(teal),
            ),
            (
                v1,
                Clip::new(0, 28.0, 22.0, "Interview")
                    .source("interview.mov")
                    .offset(95.0)
                    .source_length(300.0)
                    .color(blue),
            ),
            (
                v2,
                Clip::new(0, 10.0, 6.0, "Hands")
                    .source("hands.mov")
                    .offset(2.0)
                    .source_length(12.0)
                    .color(violet),
            ),
            (
                a1,
                Clip::new(0, 0.0, 18.0, "Interview")
                    .source("interview.wav")
                    .offset(12.0)
                    .source_length(300.0)
                    .color(green),
            ),
            (
                a1,
                Clip::new(0, 28.0, 22.0, "Interview")
                    .source("interview.wav")
                    .offset(95.0)
                    .source_length(300.0)
                    .color(green),
            ),
            (
                a2,
                Clip::new(0, 16.0, 40.0, "Score")
                    .source("score.wav")
                    .source_length(120.0)
                    .color(green),
            ),
        ];
        for (track, clip) in clips {
            seq.add_clip(track, clip).expect("the sequence's clips fit");
        }
        Self { seq, last: None }
    }
}

/// The video editor page.
#[derive(Debug, Default)]
pub struct EditorPage;

impl Plugin for EditorPage {
    fn id(&self) -> PluginId {
        EDITOR_PAGE
    }

    fn title(&self) -> String {
        "Video editor".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let state = Rc::new(RefCell::new(State::default()));
        ctx.story(
            Story::new(StoryId::from_static("editor"), "Video editor", move |ui, app| {
                show(ui, app, &mut state.borrow_mut());
            })
            .description("A sequence at 25 fps on the timeline: a pretend program monitor, video and audio tracks, clips moved and trimmed, split at the playhead (Ctrl+K), ripple deleted (Shift+Delete), in and out marks and J/K/L, each edit one step of the history.")
            .group("Components", 84),
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
    state.seq.advance(dt);
    if state.seq.is_playing() && !motion::controlled(&ctx) {
        ctx.request_repaint();
    }

    ui.label("Click the timeline, then: J, K and L shuttle, Space plays or pauses, Left and Right step a frame; I and O mark the in and out points, Alt+X clears them; Ctrl+K splits the selected clips at the playhead (none selected: every clip there); Delete removes the selected clips, Shift+Delete removes them and closes the gap; [ and ] trim to the playhead. Drag clips to move them, their edges to trim them. Each edit is a step of the history (Ctrl+Z).");

    let t = Tokens::of(ui.ctx());
    ui.add_space(t.space.s);
    ui.horizontal(|ui| {
        monitor(ui, &state.seq);
        ui.add_space(t.space.group);
        ui.vertical(|ui| {
            let seq = &state.seq;
            ui.label(
                egui::RichText::new(seq.scale.format(seq.playhead))
                    .monospace()
                    .size(t.font.scale.xl),
            );
            let asked = match seq.play {
                PlayState::Playing if seq.speed != 1.0 => format!("playing at {}×", seq.speed),
                PlayState::Playing => "playing".into(),
                PlayState::Paused => "paused".into(),
            };
            ui.colored_label(t.text.muted, asked);
            let (mark_in, mark_out, dur) = match seq.range {
                Some(r) => (
                    seq.scale.format(r.start),
                    seq.scale.format(r.end),
                    seq.scale.format(r.duration()),
                ),
                None => ("--".into(), "--".into(), "--".into()),
            };
            egui::Grid::new("editor.marks").show(ui, |ui| {
                for (k, v) in [("In", mark_in), ("Out", mark_out), ("Duration", dur)] {
                    ui.colored_label(t.text.muted, k);
                    ui.monospace(v);
                    ui.end_row();
                }
            });
            let clips = seq.clips().count();
            ui.colored_label(
                t.text.muted,
                format!("{clips} clips, {} selected", seq.selected_clips.len()),
            );
        });
    });

    section(ui, "Sequence at 25 frames per second");
    ui.horizontal(|ui| {
        if ui.button("Split").on_hover_text("Ctrl+K").clicked() {
            let ids = split_targets(&state.seq);
            if !ids.is_empty() {
                app.run(CMD_SPLIT_CLIP, json!({ "timeline": SEQUENCE, "ids": ids }));
            }
        }
        let any = !state.seq.selected_clips.is_empty();
        if ui
            .add_enabled(any, egui::Button::new("Ripple delete"))
            .on_hover_text("Shift+Delete")
            .clicked()
        {
            app.run(
                CMD_RIPPLE_DELETE,
                json!({ "timeline": SEQUENCE, "ids": state.seq.selected_clips }),
            );
        }
        if ui.button("Mark in").on_hover_text("I").clicked() {
            let r = state.seq.in_at(state.seq.playhead);
            app.run(
                CMD_SET_RANGE,
                json!({ "timeline": SEQUENCE, "start": r.start, "end": r.end }),
            );
        }
        if ui.button("Mark out").on_hover_text("O").clicked() {
            let r = state.seq.out_at(state.seq.playhead);
            app.run(
                CMD_SET_RANGE,
                json!({ "timeline": SEQUENCE, "start": r.start, "end": r.end }),
            );
        }
        if ui
            .add_enabled(state.seq.range.is_some(), egui::Button::new("Clear marks"))
            .on_hover_text("Alt+X")
            .clicked()
        {
            app.run(
                CMD_SET_RANGE,
                json!({ "timeline": SEQUENCE, "clear": true }),
            );
        }
    });
    let mut painter = EditPainter;
    Timeline::new(&mut state.seq)
        .id(SEQUENCE)
        .label("Sequence")
        .app(app)
        .history("gallery/editor/sequence")
        .clips(&mut painter)
        .show(ui);
}

/// What Ctrl+K splits: the selected clips under the playhead, else every
/// clip there.
fn split_targets(seq: &TimelineState) -> Vec<u64> {
    let t = seq.playhead;
    let at = |c: &Clip| c.start < t && t < c.end();
    let selected: Vec<u64> = seq
        .selected_clips
        .iter()
        .copied()
        .filter(|id| seq.clip(*id).is_some_and(at))
        .collect();
    if selected.is_empty() {
        seq.clips()
            .filter(|(_, c)| at(c))
            .map(|(_, c)| c.id)
            .collect()
    } else {
        selected
    }
}

/// The program monitor: the frame of the topmost video clip under the
/// playhead (a pretend picture, as the timeline's thumbnails draw it), the
/// titles over it, and the source's timecode.
fn monitor(ui: &mut Ui, seq: &TimelineState) {
    let t = Tokens::of(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(256.0, 144.0), Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 0.0, Color32::BLACK);
    let at = seq.playhead;
    let under = |kind: &str| {
        seq.tracks
            .iter()
            .filter(|tr| tr.kind == kind)
            .find_map(|tr| tr.clips.iter().find(|c| c.contains(at)))
    };
    let font = FontId::monospace(t.font.scale.xs);
    if let Some(c) = under("video") {
        let base = c.color.unwrap_or(t.surface.raised);
        let s = c.source_at(at);
        let n = noise(&c.source, s.floor() as i64);
        let horizon = rect.top() + rect.height() * (0.35 + 0.3 * n);
        p.rect_filled(
            Rect::from_x_y_ranges(rect.x_range(), rect.top()..=horizon),
            0.0,
            base.gamma_multiply(0.55 + 0.35 * n),
        );
        p.rect_filled(
            Rect::from_x_y_ranges(rect.x_range(), horizon..=rect.bottom()),
            0.0,
            base.gamma_multiply(0.95),
        );
        p.text(
            rect.left_top() + vec2(6.0, 4.0),
            Align2::LEFT_TOP,
            format!("{}  {}", c.source, seq.scale.format(s)),
            font.clone(),
            Color32::WHITE,
        );
    } else {
        p.text(
            rect.center(),
            Align2::CENTER_CENTER,
            "No video",
            font.clone(),
            Color32::GRAY,
        );
    }
    if let Some(c) = under("titles") {
        p.text(
            rect.left_bottom() + vec2(12.0, -14.0),
            Align2::LEFT_BOTTOM,
            &c.label,
            FontId::proportional(t.font.scale.m),
            Color32::WHITE,
        );
    }
    p.rect_stroke(
        rect,
        0.0,
        Stroke::new(1.0, t.border.default),
        StrokeKind::Inside,
    );
}
