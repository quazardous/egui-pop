//! The DJ page: two decks, each a timeline on bars and beats holding one
//! track (its song) drawn by a painter of the gallery's, a pretend waveform
//! under a beat grid. Hot cues are the timeline's cue points, loops its
//! range; the pitch fader scales the pretend clock, and Sync matches a
//! deck's tempo and beat to the other's. Nothing plays: the clock is the
//! motion clock, frozen in the tests.
//!
//! The deck's keys are commands bound on the decks' surface (`djDeck`):
//! 1 to 8 set or jump to a hot cue, Shift with them clears it, `[` and `]`
//! halve and double the loop, S syncs. What the timeline already has
//! (J/K/L, I and O, Ctrl+L) stays the timeline's.
//!
//! A deck's timeline keeps its playhead in the middle (`Follow::CENTRE`):
//! the waveform scrolls under it. Dragged, the waveform moves the playhead
//! with it while paused, and nudges the deck while it plays; the wheel
//! steps it by beats. The song's clip is locked to the pointer
//! (`scrub_on_drag`).

use std::cell::RefCell;
use std::rc::Rc;

use egui::{Color32, Painter, RichText, Stroke, Ui, pos2};
use egui_pop::api::stories::Story;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::core::motion;
use egui_pop::core::slider::Slider;
use egui_pop::model::commands::CommandSpec;
use egui_pop::model::ids::{CommandId, PluginId, StoryId};
use egui_pop::model::keymap::BindOptions;
use egui_pop::theme::Tokens;
use egui_pop::views::timeline::{
    Clip, ClipPaint, ClipPainter, Follow, Marker, MarkerKind, PlayState, Tempo, TimeRange,
    TimeScale, TimeView, Timeline, TimelineState, Track, Transport, TransportButton,
};
use serde_json::json;

use super::section;
use crate::shell::SHELL;

pub const DJ_PAGE: PluginId = PluginId::from_static("gallery.dj");

/// The surface both decks' timelines open: the deck's keys are bound on it.
pub const DECK_SURFACE: &str = "djDeck";

/// Sets hot cue N at the playhead, or jumps to it when it is set.
pub const CMD_HOT_CUE: [CommandId; 8] = [
    CommandId::from_static("dj.hot_cue_1"),
    CommandId::from_static("dj.hot_cue_2"),
    CommandId::from_static("dj.hot_cue_3"),
    CommandId::from_static("dj.hot_cue_4"),
    CommandId::from_static("dj.hot_cue_5"),
    CommandId::from_static("dj.hot_cue_6"),
    CommandId::from_static("dj.hot_cue_7"),
    CommandId::from_static("dj.hot_cue_8"),
];
/// Clears hot cue N.
pub const CMD_CLEAR_HOT_CUE: [CommandId; 8] = [
    CommandId::from_static("dj.clear_hot_cue_1"),
    CommandId::from_static("dj.clear_hot_cue_2"),
    CommandId::from_static("dj.clear_hot_cue_3"),
    CommandId::from_static("dj.clear_hot_cue_4"),
    CommandId::from_static("dj.clear_hot_cue_5"),
    CommandId::from_static("dj.clear_hot_cue_6"),
    CommandId::from_static("dj.clear_hot_cue_7"),
    CommandId::from_static("dj.clear_hot_cue_8"),
];
pub const CMD_LOOP_HALVE: CommandId = CommandId::from_static("dj.loop_halve");
pub const CMD_LOOP_DOUBLE: CommandId = CommandId::from_static("dj.loop_double");
pub const CMD_SYNC: CommandId = CommandId::from_static("dj.sync");

/// The hot cues' colours, pad 1 to 8.
pub const CUE_COLORS: [Color32; 8] = [
    Color32::from_rgb(0xe5, 0x3e, 0x3e),
    Color32::from_rgb(0xf0, 0x8c, 0x2a),
    Color32::from_rgb(0xe8, 0xc5, 0x2c),
    Color32::from_rgb(0x4c, 0xb8, 0x4c),
    Color32::from_rgb(0x2c, 0xb8, 0xc8),
    Color32::from_rgb(0x3d, 0x6f, 0xe0),
    Color32::from_rgb(0x9a, 0x5c, 0xe0),
    Color32::from_rgb(0xe0, 0x5c, 0xb0),
];

/// How far the pitch fader goes, in percent.
pub const PITCH_RANGE: f64 = 8.0;

/// A deck: a song on a timeline, and its pitch.
#[derive(Debug)]
pub struct Deck {
    pub name: &'static str,
    pub tl: TimelineState,
    /// The pitch fader, in percent: the song plays `1 + pitch / 100` as
    /// fast.
    pub pitch: f64,
    /// The main cue point (CUE), apart from the hot cues.
    pub cue: f64,
    /// The song's clip, put back if a key moved or trimmed it.
    song: Clip,
}

impl Deck {
    fn new(name: &'static str, title: &str, source: &str, tempo: Tempo, bars: f64) -> Self {
        let length = tempo.offset + bars * tempo.bar();
        let mut tl = TimelineState::new(TimeScale::Beats(tempo), length);
        let track = tl.add_track(Track::new(0, name).kind("deck").height(72.0));
        let song = Clip::new(0, 0.0, length, title)
            .source(source)
            .source_length(length)
            .color(Color32::from_rgb(0x3d, 0x7b, 0xc9));
        let id = tl.add_clip(track, song.clone()).expect("the song fits");
        let song = tl.clip(id).cloned().expect("just added");
        Self {
            name,
            tl,
            pitch: 0.0,
            cue: 0.0,
            song,
        }
    }

    /// CUE, as on a CD player: playing, back to the cue point and paused;
    /// paused, the cue point set at the playhead (on the nearest beat).
    pub fn press_main_cue(&mut self) {
        if self.tl.is_playing() {
            self.tl.pause();
            self.tl.set_playhead(self.cue);
        } else {
            self.cue = self.nearest_beat(self.tl.playhead);
            self.tl.set_playhead(self.cue);
        }
    }

    pub fn tempo(&self) -> Tempo {
        match self.tl.scale {
            TimeScale::Beats(t) => t,
            _ => Tempo::default(),
        }
    }

    /// How fast the song plays: the pitch fader's ratio.
    pub fn rate(&self) -> f64 {
        1.0 + self.pitch / 100.0
    }

    /// The tempo heard: the song's, times the pitch.
    pub fn bpm(&self) -> f64 {
        self.tempo().bpm * self.rate()
    }

    /// Where the playhead is in beats from the first downbeat.
    pub fn beats(&self) -> f64 {
        let t = self.tempo();
        (self.tl.playhead - t.offset) / t.beat()
    }

    /// The beat nearest `time`.
    fn nearest_beat(&self, time: f64) -> f64 {
        let t = self.tempo();
        let at = t.offset + ((time - t.offset) / t.beat()).round() * t.beat();
        self.tl.clamp(at)
    }

    /// The hot cue on pad `pad` (0 to 7).
    pub fn hot_cue(&self, pad: usize) -> Option<&Marker> {
        let label = (pad + 1).to_string();
        self.tl
            .markers
            .iter()
            .find(|m| m.kind == MarkerKind::Cue && m.label == label)
    }

    /// Sets hot cue `pad` at the playhead (on the nearest beat, as DJ
    /// software quantizes), or jumps to it when it is set.
    pub fn press_cue(&mut self, pad: usize) -> serde_json::Value {
        if let Some(at) = self.hot_cue(pad).map(|m| m.time) {
            self.tl.set_playhead(at);
            return json!({ "jumped": at });
        }
        let at = self.nearest_beat(self.tl.playhead);
        self.tl.add_marker(
            Marker::new(0, at, (pad + 1).to_string())
                .kind(MarkerKind::Cue)
                .color(CUE_COLORS[pad]),
        );
        json!({ "set": at })
    }

    /// Clears hot cue `pad`.
    pub fn clear_cue(&mut self, pad: usize) -> bool {
        match self.hot_cue(pad).map(|m| m.id) {
            Some(id) => self.tl.remove_marker(id).is_some(),
            None => false,
        }
    }

    /// The loop's length in beats.
    pub fn loop_beats(&self) -> Option<f64> {
        self.tl.range.map(|r| r.duration() / self.tempo().beat())
    }

    /// Halves the loop (down to a sixteenth of a beat), or doubles it (up to
    /// the song's end); without a loop, starts one of four beats on the
    /// nearest beat.
    pub fn resize_loop(&mut self, double: bool) {
        let beat = self.tempo().beat();
        let end = self.tl.end();
        let range = match self.tl.range {
            Some(r) => {
                let len = if double {
                    r.duration() * 2.0
                } else {
                    (r.duration() / 2.0).max(beat / 16.0)
                };
                TimeRange::new(r.start, (r.start + len).min(end))
            }
            None => {
                let start = self.nearest_beat(self.tl.playhead);
                TimeRange::new(start, (start + 4.0 * beat).min(end))
            }
        };
        self.tl.range = Some(range);
        self.tl.looping = true;
    }

    /// Plays at the other deck's tempo, on its beat: the pitch that makes
    /// the tempos equal, then the playhead moved by less than half a beat
    /// so that both are as far into their beat.
    pub fn sync_to(&mut self, master: &Deck) {
        let pitch = (master.bpm() / self.tempo().bpm - 1.0) * 100.0;
        self.pitch = pitch.clamp(-PITCH_RANGE, PITCH_RANGE);
        let phase = master.beats().rem_euclid(1.0);
        let here = self.beats();
        let mut beats = here.floor() + phase;
        if beats - here > 0.5 {
            beats -= 1.0;
        } else if here - beats > 0.5 {
            beats += 1.0;
        }
        let t = self.tempo();
        self.tl.playhead = self.tl.clamp(t.offset + beats * t.beat());
    }

    /// Puts the song back where it was if a key moved, trimmed, split or
    /// removed it: a deck's track is not an edit (the pointer cannot, the
    /// clip being locked to it).
    fn keep_song(&mut self) {
        let track = &mut self.tl.tracks[0];
        if track.clips.len() != 1 || track.clips[0] != self.song {
            track.clips = vec![self.song.clone()];
        }
        self.tl.selected_clips.clear();
    }
}

/// The page's two decks, which one the keys act on, and the clock's last
/// reading.
#[derive(Debug)]
pub struct State {
    pub decks: [Deck; 2],
    /// The deck whose timeline had the focus last.
    pub active: usize,
    last: Option<f64>,
}

impl Default for State {
    fn default() -> Self {
        // Deck A: 96 bars at 128 BPM, playing round a loop of four bars
        // in the drop.
        let mut a = Deck::new(
            "Deck A",
            "Night Drive (Extended Mix)",
            "night-drive.wav",
            Tempo::new(128.0, 4, 4),
            96.0,
        );
        let bar = a.tempo().bar();
        for (pad, at) in [(0, 0.0), (1, 16.0), (2, 32.0), (4, 64.0)] {
            a.tl.playhead = at * bar;
            a.press_cue(pad);
        }
        a.tl.range = Some(TimeRange::new(32.0 * bar, 36.0 * bar));
        a.tl.looping = true;
        a.tl.playhead = 33.0 * bar + 2.5 * a.tempo().beat();
        a.tl.play_at(1.0);

        // Deck B: 124 BPM, its first downbeat 0.12 s in, cued at the start.
        let mut b = Deck::new(
            "Deck B",
            "Sunrise Theory",
            "sunrise-theory.wav",
            Tempo::new(124.0, 4, 4).offset(0.12),
            80.0,
        );
        let tb = b.tempo();
        for (pad, at) in [(0, 0.0), (3, 24.0)] {
            b.tl.playhead = tb.offset + at * tb.bar();
            b.press_cue(pad);
        }
        b.tl.playhead = tb.offset + 1.25 * tb.beat();
        for deck in [&mut a, &mut b] {
            let around = 8.0 * deck.tempo().bar();
            deck.tl.view = TimeView::new(deck.tl.playhead - around / 2.0, around);
        }
        Self {
            decks: [a, b],
            active: 0,
            last: None,
        }
    }
}

/// Which deck a command acts on: its `deck` argument (`a`, `b`), else the
/// one the keys last reached.
fn deck_of(state: &State, args: &serde_json::Value) -> Result<usize, String> {
    match args.get("deck").and_then(|d| d.as_str()) {
        None => Ok(state.active),
        Some(d) if d.eq_ignore_ascii_case("a") => Ok(0),
        Some(d) if d.eq_ignore_ascii_case("b") => Ok(1),
        Some(d) => Err(format!("no deck {d}: a or b")),
    }
}

/// The DJ page.
#[derive(Debug, Default)]
pub struct DjPage;

impl Plugin for DjPage {
    fn id(&self) -> PluginId {
        DJ_PAGE
    }

    fn title(&self) -> String {
        "DJ decks".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let state = Rc::new(RefCell::new(State::default()));
        let deck_arg = json!({ "deck": "a or b; else the deck the keys last reached" });
        for pad in 0..8 {
            let s = state.clone();
            ctx.command(
                CommandSpec::new(
                    CMD_HOT_CUE[pad].clone(),
                    &format!("Hot cue {}", pad + 1),
                    "DJ",
                )
                .description("Sets the hot cue at the playhead, on the nearest beat, or jumps to it when it is set.")
                .args(deck_arg.clone())
                .key_with(&(pad + 1).to_string(), BindOptions::surface(DECK_SURFACE))
                .hidden(),
                move |call| {
                    let mut s = s.try_borrow_mut().map_err(|_| "the decks are busy")?;
                    let d = deck_of(&s, call.args)?;
                    Ok(s.decks[d].press_cue(pad))
                },
            )?;
            let s = state.clone();
            ctx.command(
                CommandSpec::new(
                    CMD_CLEAR_HOT_CUE[pad].clone(),
                    &format!("Clear hot cue {}", pad + 1),
                    "DJ",
                )
                .description("Clears the hot cue.")
                .args(deck_arg.clone())
                .key_with(
                    &format!("Shift+{}", pad + 1),
                    BindOptions::surface(DECK_SURFACE),
                )
                .hidden(),
                move |call| {
                    let mut s = s.try_borrow_mut().map_err(|_| "the decks are busy")?;
                    let d = deck_of(&s, call.args)?;
                    Ok(json!({ "cleared": s.decks[d].clear_cue(pad) }))
                },
            )?;
        }
        for (id, title, key, double) in [
            (CMD_LOOP_HALVE, "Halve the loop", "OpenBracket", false),
            (CMD_LOOP_DOUBLE, "Double the loop", "CloseBracket", true),
        ] {
            let s = state.clone();
            ctx.command(
                CommandSpec::new(id, title, "DJ")
                    .description(
                        "Halves or doubles the loop from its start; without one, loops four beats.",
                    )
                    .args(deck_arg.clone())
                    .key_with(key, BindOptions::surface(DECK_SURFACE))
                    .hidden(),
                move |call| {
                    let mut s = s.try_borrow_mut().map_err(|_| "the decks are busy")?;
                    let d = deck_of(&s, call.args)?;
                    s.decks[d].resize_loop(double);
                    Ok(json!({ "beats": s.decks[d].loop_beats() }))
                },
            )?;
        }
        let s = state.clone();
        ctx.command(
            CommandSpec::new(CMD_SYNC, "Sync to the other deck", "DJ")
                .description("Matches the deck's tempo and beat to the other deck's.")
                .args(deck_arg)
                .key_with("S", BindOptions::surface(DECK_SURFACE))
                .hidden(),
            move |call| {
                let mut s = s.try_borrow_mut().map_err(|_| "the decks are busy")?;
                let d = deck_of(&s, call.args)?;
                sync(&mut s, d);
                Ok(json!({ "pitch": s.decks[d].pitch }))
            },
        )?;
        ctx.story(
            Story::new(StoryId::from_static("dj"), "DJ decks", move |ui, app| {
                show(ui, app, &mut state.borrow_mut());
            })
            .description("Two decks on bars and beats: a beat grid over a pretend waveform drawn by a clip painter, hot cues 1 to 8, loops halved and doubled, tempo and pitch, J/K/L and a sync of one deck to the other's beat, on the timeline and its tracks.")
            .group("Components", 83),
        )?;
        Ok(())
    }
}

/// Deck `d` synced to the other.
fn sync(state: &mut State, d: usize) {
    let [a, b] = &mut state.decks;
    if d == 0 { a.sync_to(b) } else { b.sync_to(a) }
}

pub fn show(ui: &mut Ui, app: &AppHandle, state: &mut State) {
    let ctx = ui.ctx().clone();
    // The application moves the time: a pretend player on the motion clock,
    // each deck as fast as its pitch.
    let now = motion::now(&ctx);
    let dt = state.last.map_or(0.0, |last| (now - last).max(0.0));
    state.last = Some(now);
    let mut playing = false;
    for deck in &mut state.decks {
        let rate = deck.rate();
        deck.tl.advance(dt * rate);
        playing |= deck.tl.is_playing();
    }
    if playing && !motion::controlled(&ctx) {
        ctx.request_repaint();
    }

    ui.label("Drag a deck's waveform to move it under the playhead, or to nudge the deck while it plays; the wheel steps by beats. Click it, then: 1 to 8 set a hot cue on the nearest beat or jump to it, Shift+1 to 8 clear it; I and O set a loop, Ctrl+L turns it on or off, [ and ] halve and double it; J, K and L shuttle, Space plays or pauses; S syncs the deck to the other's tempo and beat. The pads, the loop buttons and the pitch fader do the same with the pointer.");

    for d in 0..2 {
        deck_ui(ui, app, state, d);
    }
}

fn deck_ui(ui: &mut Ui, app: &AppHandle, state: &mut State, d: usize) {
    let t = Tokens::of(ui.ctx());
    let id = if d == 0 { "deck-a" } else { "deck-b" };
    let deck = &state.decks[d];
    section(ui, &format!("{}: {}", deck.name, deck.song.label));
    // The position, the tempo heard and the transport.
    ui.horizontal(|ui| {
        ui.monospace(deck.tl.scale.format(deck.tl.playhead));
        let left = (deck.tl.end() - deck.tl.playhead) / deck.rate();
        ui.monospace(format!(
            "-{}:{:04.1}",
            (left / 60.0).floor(),
            left.rem_euclid(60.0)
        ));
        ui.monospace(format!("{:.2} BPM", deck.bpm()));
        let asked = match deck.tl.play {
            PlayState::Playing if deck.tl.speed != 1.0 => format!("playing at {}×", deck.tl.speed),
            PlayState::Playing => "playing".into(),
            PlayState::Paused => "paused".into(),
        };
        ui.colored_label(t.text.muted, asked);
    });
    let mut sync_now = false;
    let deck = &mut state.decks[d];
    ui.horizontal(|ui| {
        // Play runs `timeline.toggle_play` on the deck, as Space does.
        Transport::new(&mut deck.tl)
            .app(app)
            .timeline(id)
            .buttons(&[TransportButton::PlayPause])
            .timecode(false)
            .show(ui);
        if ui
            .button("Cue")
            .on_hover_text("Playing: back to the cue point, paused; paused: sets it here")
            .clicked()
        {
            deck.press_main_cue();
        }
        if ui.button("Sync").clicked() {
            sync_now = true;
        }
        ui.add_space(t.space.group);
        ui.scope(|ui| {
            ui.set_width(220.0);
            Slider::new(&mut deck.pitch, "Pitch", -PITCH_RANGE..=PITCH_RANGE)
                .unit("%")
                .precision(1)
                .signed()
                .default(0.0)
                .show(ui);
        });
    });
    // The pads and the loop.
    ui.horizontal(|ui| {
        for (pad, color) in CUE_COLORS.into_iter().enumerate() {
            let label = RichText::new((pad + 1).to_string()).monospace();
            let set = deck.hot_cue(pad).is_some();
            let button = if set {
                egui::Button::new(label.color(Color32::BLACK)).fill(color)
            } else {
                egui::Button::new(label)
            };
            let r = ui
                .add_sized([28.0, 22.0], button)
                .on_hover_text("Click: set or jump; Shift+click: clear");
            if r.clicked() {
                if ui.input(|i| i.modifiers.shift) {
                    deck.clear_cue(pad);
                } else {
                    deck.press_cue(pad);
                }
            }
        }
        ui.add_space(t.space.group);
        if ui.button("Loop in").clicked() {
            deck.tl.range = Some(deck.tl.in_at(deck.tl.playhead));
        }
        if ui.button("Loop out").clicked() {
            deck.tl.range = Some(deck.tl.out_at(deck.tl.playhead));
            deck.tl.looping = true;
        }
        if ui.button("½").on_hover_text("Halve the loop").clicked() {
            deck.resize_loop(false);
        }
        if ui.button("×2").on_hover_text("Double the loop").clicked() {
            deck.resize_loop(true);
        }
        ui.checkbox(&mut deck.tl.looping, "Loop");
        let beats = match deck.loop_beats() {
            Some(b) if b >= 1.0 => format!("{} beats", trim_number(b)),
            Some(b) => format!("1/{} beat", trim_number(1.0 / b)),
            None => "no loop".into(),
        };
        ui.colored_label(t.text.muted, beats);
    });
    let tempo = deck.tempo();
    let mut painter = DeckPainter { tempo };
    let out = Timeline::new(&mut deck.tl)
        .id(id)
        .label(deck.name)
        .surface(DECK_SURFACE)
        .follow_mode(Follow::CENTRE)
        .scrub_on_drag(true)
        .nudge_on_drag(true)
        .lanes(0.0)
        .clips(&mut painter)
        .show(ui);
    deck.keep_song();
    if out.response.has_focus() {
        state.active = d;
    }
    if sync_now {
        sync(state, d);
    }
}

/// `4`, `0.5`, without trailing zeros.
fn trim_number(x: f64) -> String {
    let s = format!("{x:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Draws a deck's song: a pretend waveform, louder on the beats and in the
/// drop, under the beat grid (each beat, the bars brighter).
#[derive(Debug)]
pub struct DeckPainter {
    pub tempo: Tempo,
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

/// How loud the song is at bar `bar`: an intro, a build, the drop, a
/// break and the outro.
fn energy(bar: f64) -> f32 {
    match bar {
        b if b < 16.0 => 0.45,
        b if b < 32.0 => 0.5 + 0.35 * ((b - 16.0) / 16.0) as f32,
        b if b < 64.0 => 0.95,
        b if b < 72.0 => 0.35,
        _ => 0.6,
    }
}

impl ClipPainter for DeckPainter {
    fn paint(&mut self, painter: &Painter, clip: &ClipPaint<'_>) {
        let base = clip.clip.color.unwrap_or(clip.tokens.clip_text);
        let r = clip.rect;
        let mid = r.center().y;
        let half = r.height() / 2.0 - 2.0;
        let beat = self.tempo.beat();
        let per_bar = f64::from(self.tempo.beats_per_bar.max(1));
        // The waveform: a line every point across.
        let mut x = clip.visible.left().floor();
        while x <= clip.visible.right() {
            let s = clip.source_at(x);
            let b = (s - self.tempo.offset) / beat;
            if b >= 0.0 {
                let phase = b.fract() as f32;
                let kick = (1.0 - phase).powi(6);
                let level = energy(b / per_bar)
                    * (0.35
                        + 0.4 * kick
                        + 0.25 * noise(&clip.clip.source, (b * 8.0).floor() as i64));
                let h = half * level.min(1.0);
                painter.line_segment(
                    [pos2(x, mid - h), pos2(x, mid + h)],
                    Stroke::new(1.0, base.gamma_multiply(0.6 + 0.4 * kick)),
                );
            }
            x += 1.0;
        }
        // The beat grid.
        let (from, to) = clip.source_in_view();
        let first = ((from - self.tempo.offset) / beat).floor().max(0.0) as i64;
        let last = ((to - self.tempo.offset) / beat).ceil() as i64;
        if last - first > 2000 {
            return;
        }
        let grid = clip.tokens.clip_text;
        for k in first..=last {
            let x = clip.x_of_source(self.tempo.offset + k as f64 * beat);
            let bar = k % self.tempo.beats_per_bar.max(1) as i64 == 0;
            let (alpha, width) = if bar { (0.9, 1.5) } else { (0.35, 1.0) };
            painter.line_segment(
                [pos2(x, r.top()), pos2(x, r.bottom())],
                Stroke::new(width, grid.gamma_multiply(alpha)),
            );
        }
        clip.paint_label(painter);
    }
}
