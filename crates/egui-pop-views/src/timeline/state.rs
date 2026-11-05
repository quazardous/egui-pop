//! [`TimelineState`]: what a timeline shows and holds, serializable, and the
//! edits made on it, without egui.

use egui::Color32;
use serde::{Deserialize, Serialize};

use super::scale::TimeScale;
use super::tracks::{Clip, ClipPlace, ClipSpan, Track};

/// The fastest J or L shuttles, in either direction.
pub const MAX_SPEED: f64 = 32.0;

/// The part of time in view: from `start`, `duration` long (seconds).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TimeView {
    pub start: f64,
    pub duration: f64,
}

impl Default for TimeView {
    fn default() -> Self {
        Self {
            start: 0.0,
            duration: 60.0,
        }
    }
}

impl TimeView {
    pub fn new(start: f64, duration: f64) -> Self {
        Self { start, duration }
    }

    pub fn end(&self) -> f64 {
        self.start + self.duration
    }

    /// Where `time` is across `width` points (0 at the start).
    pub fn x_of(&self, time: f64, width: f32) -> f32 {
        ((time - self.start) / self.duration.max(f64::MIN_POSITIVE) * f64::from(width)) as f32
    }

    /// The time at `x` points across `width`.
    pub fn time_at(&self, x: f32, width: f32) -> f64 {
        self.start + f64::from(x) / f64::from(width.max(1.0)) * self.duration
    }

    /// Zoomed by `factor` (above 1: closer) keeping `at` where it is.
    pub fn zoomed(&self, factor: f64, at: f64) -> TimeView {
        let factor = if factor.is_finite() && factor > 0.0 {
            factor
        } else {
            1.0
        };
        let duration = self.duration / factor;
        let keep = (at - self.start) / self.duration.max(f64::MIN_POSITIVE);
        TimeView {
            start: at - keep * duration,
            duration,
        }
    }

    /// Whether `time` is in view.
    pub fn contains(&self, time: f64) -> bool {
        time >= self.start && time <= self.end()
    }
}

/// A span of time, `start` before `end` (seconds).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TimeRange {
    pub start: f64,
    pub end: f64,
}

impl TimeRange {
    /// From the earlier of `a` and `b` to the later.
    pub fn new(a: f64, b: f64) -> Self {
        Self {
            start: a.min(b),
            end: a.max(b),
        }
    }

    pub fn duration(&self) -> f64 {
        self.end - self.start
    }

    pub fn contains(&self, time: f64) -> bool {
        time >= self.start && time <= self.end
    }

    pub fn clamp(&self, time: f64) -> f64 {
        time.clamp(self.start, self.end)
    }
}

/// A marker, or a cue point (a DJ's hot cue, drawn as a pin).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkerKind {
    #[default]
    Marker,
    Cue,
}

/// A named point in time.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Marker {
    /// Unique in its timeline.
    pub id: u64,
    pub time: f64,
    pub label: String,
    #[serde(default)]
    pub kind: MarkerKind,
    /// Its own colour; the theme's (`timeline.marker`) when `None`.
    #[serde(default)]
    pub color: Option<Color32>,
}

impl Marker {
    pub fn new(id: u64, time: f64, label: impl Into<String>) -> Self {
        Self {
            id,
            time,
            label: label.into(),
            kind: MarkerKind::Marker,
            color: None,
        }
    }

    pub fn kind(mut self, kind: MarkerKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn color(mut self, color: Color32) -> Self {
        self.color = Some(color);
        self
    }
}

/// Whether the timeline asks to play.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayState {
    #[default]
    Paused,
    Playing,
}

/// The document's part of a timeline: what its history keeps, one step per
/// change (the markers, the range, the loop, the tracks and their clips).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TimelineDoc {
    pub markers: Vec<Marker>,
    pub range: Option<TimeRange>,
    pub looping: bool,
    pub tracks: Vec<Track>,
}

/// A change of a timeline's document, as its output reports it.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Edit {
    MarkerAdded(Marker),
    MarkerMoved {
        id: u64,
        from: f64,
        to: f64,
    },
    MarkerRemoved(Marker),
    RangeSet {
        from: Option<TimeRange>,
        to: Option<TimeRange>,
    },
    LoopSet(bool),
    ClipAdded {
        track: u64,
        clip: Clip,
    },
    /// A clip moved, along its track or to another.
    ClipMoved {
        id: u64,
        from: ClipPlace,
        to: ClipPlace,
    },
    /// A clip's start or end moved, its offset into its source following
    /// its start.
    ClipTrimmed {
        id: u64,
        from: ClipSpan,
        to: ClipSpan,
    },
    ClipRemoved {
        track: u64,
        clip: Clip,
    },
    /// A clip split in two at `at`: clip `id` ends there, clip `new`
    /// (added) starts there.
    ClipSplit {
        id: u64,
        at: f64,
        new: u64,
    },
    /// The document was replaced from the history (an undo, a redo).
    Restored,
}

/// Everything a timeline shows: its scale, view, playhead, range, markers,
/// tracks and clips, and the play state and speed it asks for. The application keeps it,
/// draws it with [`Timeline`](super::Timeline) and moves the playhead while
/// it plays ([`advance`](Self::advance)).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TimelineState {
    pub scale: TimeScale,
    pub view: TimeView,
    /// The document's span: the playhead stays inside, the view near it.
    pub bounds: Option<TimeRange>,
    pub playhead: f64,
    pub range: Option<TimeRange>,
    /// Playback goes round the range.
    pub looping: bool,
    pub markers: Vec<Marker>,
    /// The marker the keys act on.
    pub selected: Option<u64>,
    /// Rows of clips under the ruler, from the top.
    pub tracks: Vec<Track>,
    /// The clips selected, the one selected last at the end (the keys act
    /// on it).
    pub selected_clips: Vec<u64>,
    pub play: PlayState,
    /// Asked speed, negative backwards (J), 1 by default.
    pub speed: f64,
}

impl Default for TimelineState {
    fn default() -> Self {
        Self {
            scale: TimeScale::Seconds,
            view: TimeView::default(),
            bounds: None,
            playhead: 0.0,
            range: None,
            looping: false,
            markers: Vec::new(),
            selected: None,
            tracks: Vec::new(),
            selected_clips: Vec::new(),
            play: PlayState::Paused,
            speed: 1.0,
        }
    }
}

impl TimelineState {
    /// A timeline on `scale` over `0..length` seconds, all of it in view.
    pub fn new(scale: TimeScale, length: f64) -> Self {
        Self {
            scale,
            view: TimeView::new(0.0, length.max(scale.unit())),
            bounds: Some(TimeRange::new(0.0, length)),
            ..Self::default()
        }
    }

    pub fn is_playing(&self) -> bool {
        self.play == PlayState::Playing
    }

    /// The document's part.
    pub fn doc(&self) -> TimelineDoc {
        TimelineDoc {
            markers: self.markers.clone(),
            range: self.range,
            looping: self.looping,
            tracks: self.tracks.clone(),
        }
    }

    /// Replaces the document's part (an undo, a load).
    pub fn set_doc(&mut self, doc: TimelineDoc) {
        self.markers = doc.markers;
        self.range = doc.range;
        self.looping = doc.looping;
        self.tracks = doc.tracks;
        self.prune_selection();
        if self
            .selected
            .is_some_and(|id| !self.markers.iter().any(|m| m.id == id))
        {
            self.selected = None;
        }
    }

    /// `time` kept inside the bounds and on the scale (whole frames).
    pub fn clamp(&self, time: f64) -> f64 {
        let t = match self.bounds {
            Some(b) => b.clamp(time),
            None => time,
        };
        self.scale.quantize(t)
    }

    /// Moves the playhead (kept inside the bounds): whether it moved.
    pub fn set_playhead(&mut self, time: f64) -> bool {
        let t = self.clamp(time);
        let moved = t != self.playhead;
        self.playhead = t;
        moved
    }

    /// Plays at `speed` (negative: backwards).
    pub fn play_at(&mut self, speed: f64) {
        self.play = PlayState::Playing;
        self.speed = if speed.is_finite() && speed != 0.0 {
            speed.clamp(-MAX_SPEED, MAX_SPEED)
        } else {
            1.0
        };
    }

    /// Pauses, the speed back to 1.
    pub fn pause(&mut self) {
        self.play = PlayState::Paused;
        self.speed = 1.0;
    }

    /// Space: plays forward at 1, or pauses.
    pub fn toggle(&mut self) {
        if self.is_playing() {
            self.pause();
        } else {
            self.play_at(1.0);
        }
    }

    /// J (`forward` false) or L: plays that way at 1, or twice as fast when
    /// it already does, up to [`MAX_SPEED`].
    pub fn shuttle(&mut self, forward: bool) {
        let sign = if forward { 1.0 } else { -1.0 };
        if self.is_playing() && self.speed * sign > 0.0 {
            self.play_at((self.speed * 2.0).clamp(-MAX_SPEED, MAX_SPEED));
        } else {
            self.play_at(sign);
        }
    }

    /// Moves the playhead by `by` seconds, pausing: whether it moved.
    pub fn step_by(&mut self, by: f64) -> bool {
        self.pause();
        self.set_playhead(self.playhead + by)
    }

    /// Moves the playhead as playing does for `dt` seconds of the clock:
    /// round the range when looping, else stopping at the bounds. Whether
    /// it moved.
    pub fn advance(&mut self, dt: f64) -> bool {
        if !self.is_playing() || dt.is_nan() || dt <= 0.0 {
            return false;
        }
        let before = self.playhead;
        let mut t = self.playhead + dt * self.speed;
        match self.range.filter(|r| self.looping && r.duration() > 0.0) {
            Some(r) if r.contains(before) || (self.speed > 0.0 && before < r.start) => {
                let span = r.duration();
                if t > r.end {
                    t = r.start + (t - r.start).rem_euclid(span);
                } else if t < r.start && before >= r.start {
                    t = r.end - (r.start - t).rem_euclid(span);
                }
            }
            _ => {}
        }
        if let Some(b) = self.bounds
            && (t >= b.end || t <= b.start)
        {
            t = b.clamp(t);
            self.pause();
        }
        // Not quantized while playing: the frames shown follow the clock.
        self.playhead = t;
        self.playhead != before
    }

    pub fn marker(&self, id: u64) -> Option<&Marker> {
        self.markers.iter().find(|m| m.id == id)
    }

    /// The id the next marker takes.
    pub fn next_id(&self) -> u64 {
        self.markers.iter().map(|m| m.id).max().map_or(1, |m| m + 1)
    }

    /// Adds a marker (its id taken if free, else the next one), in time
    /// order; returns the id.
    pub fn add_marker(&mut self, mut marker: Marker) -> u64 {
        if marker.id == 0 || self.marker(marker.id).is_some() {
            marker.id = self.next_id();
        }
        marker.time = self.clamp(marker.time);
        let id = marker.id;
        self.markers.push(marker);
        self.sort_markers();
        id
    }

    /// Moves a marker: its old time, `None` when there is no such marker.
    pub fn move_marker(&mut self, id: u64, time: f64) -> Option<f64> {
        let t = self.clamp(time);
        let m = self.markers.iter_mut().find(|m| m.id == id)?;
        let from = m.time;
        m.time = t;
        self.sort_markers();
        Some(from)
    }

    pub fn remove_marker(&mut self, id: u64) -> Option<Marker> {
        let at = self.markers.iter().position(|m| m.id == id)?;
        if self.selected == Some(id) {
            self.selected = None;
        }
        Some(self.markers.remove(at))
    }

    fn sort_markers(&mut self) {
        self.markers
            .sort_by(|a, b| a.time.total_cmp(&b.time).then(a.id.cmp(&b.id)));
    }

    /// The marker after the selected one (else after the playhead), or
    /// before it (`forward` false).
    pub fn neighbour_marker(&self, forward: bool) -> Option<u64> {
        let (at, id) = match self.selected.and_then(|id| self.marker(id)) {
            Some(m) => (m.time, Some(m.id)),
            None => (self.playhead, None),
        };
        let key = |m: &Marker| (m.time, m.id);
        let here = (at, id.unwrap_or(if forward { u64::MAX } else { 0 }));
        let later = |m: &&Marker| {
            let k = key(m);
            k.0 > here.0 || (k.0 == here.0 && k.1 > here.1)
        };
        if forward {
            self.markers.iter().find(later).map(|m| m.id)
        } else {
            self.markers
                .iter()
                .rev()
                .find(|m| !later(m) && Some(m.id) != id)
                .map(|m| m.id)
        }
    }

    /// The in point at `time`: the range starts there (ends at its end, else
    /// the bounds' end, else `time`).
    pub fn in_at(&self, time: f64) -> TimeRange {
        let t = self.clamp(time);
        let end = match self.range {
            Some(r) if r.end > t => r.end,
            _ => self.bounds.map_or(t, |b| b.end).max(t),
        };
        TimeRange { start: t, end }
    }

    /// The out point at `time`.
    pub fn out_at(&self, time: f64) -> TimeRange {
        let t = self.clamp(time);
        let start = match self.range {
            Some(r) if r.start < t => r.start,
            _ => self.bounds.map_or(t, |b| b.start).min(t),
        };
        TimeRange { start, end: t }
    }

    /// Where Home goes: the bounds' start, else 0.
    pub fn home(&self) -> f64 {
        self.bounds.map_or(0.0, |b| b.start)
    }

    /// Where End goes: the bounds' end, else the last marker or the range's
    /// end, whichever is later.
    pub fn end(&self) -> f64 {
        match self.bounds {
            Some(b) => b.end,
            None => self
                .markers
                .iter()
                .map(|m| m.time)
                .chain(self.range.map(|r| r.end))
                .fold(self.playhead, f64::max),
        }
    }

    /// The shortest and longest the view may be.
    pub fn zoom_limits(&self) -> (f64, f64) {
        let least = match self.scale {
            TimeScale::Seconds => 0.05,
            TimeScale::Frames(r) => 5.0 / r.fps(),
            TimeScale::Beats(t) => t.beat() / 2.0,
        };
        let most = self
            .bounds
            .map_or(86_400.0, |b| (b.duration() * 1.5).max(least * 2.0));
        (least, most)
    }

    /// `view` within the zoom limits and near the bounds.
    pub fn fit_view(&self, view: TimeView) -> TimeView {
        let (least, most) = self.zoom_limits();
        let duration = if view.duration.is_finite() {
            view.duration.clamp(least, most)
        } else {
            most
        };
        // Zoomed past a limit: around the middle of what was asked.
        let mut start = view.start + (view.duration - duration) / 2.0;
        if !start.is_finite() {
            start = 0.0;
        }
        if let Some(b) = self.bounds {
            let margin = duration * 0.05;
            let lo = b.start - margin;
            let hi = (b.end + margin - duration).max(lo);
            start = start.clamp(lo, hi);
        }
        TimeView { start, duration }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> TimelineState {
        TimelineState::new(TimeScale::frames(25), 60.0)
    }

    #[test]
    fn j_k_and_l_shuttle_and_double() {
        let mut s = state();
        s.shuttle(true);
        assert_eq!((s.play, s.speed), (PlayState::Playing, 1.0));
        s.shuttle(true);
        s.shuttle(true);
        assert_eq!(s.speed, 4.0);
        s.shuttle(false);
        assert_eq!(s.speed, -1.0);
        s.shuttle(false);
        assert_eq!(s.speed, -2.0);
        for _ in 0..10 {
            s.shuttle(false);
        }
        assert_eq!(s.speed, -MAX_SPEED);
        s.pause();
        assert_eq!((s.play, s.speed), (PlayState::Paused, 1.0));
        s.toggle();
        assert!(s.is_playing());
    }

    #[test]
    fn playing_goes_round_a_looping_range_and_stops_at_the_end() {
        let mut s = state();
        s.range = Some(TimeRange::new(10.0, 12.0));
        s.looping = true;
        s.playhead = 11.5;
        s.play_at(1.0);
        s.advance(1.0);
        assert!((s.playhead - 10.5).abs() < 1e-9, "{}", s.playhead);
        // Backwards, round from its start to its end.
        s.play_at(-1.0);
        s.advance(1.0);
        assert!((s.playhead - 11.5).abs() < 1e-9, "{}", s.playhead);
        // Not looping: on to the end, where it pauses.
        s.looping = false;
        s.play_at(8.0);
        s.advance(10.0);
        assert_eq!((s.playhead, s.play), (60.0, PlayState::Paused));
        // Paused, the clock moves nothing.
        assert!(!s.advance(1.0));
    }

    #[test]
    fn markers_keep_time_order_and_unique_ids() {
        let mut s = state();
        let a = s.add_marker(Marker::new(0, 20.0, "Chorus"));
        let b = s.add_marker(Marker::new(a, 10.0, "Verse"));
        assert_ne!(a, b);
        assert_eq!(s.markers[0].label, "Verse");
        // On whole frames, inside the bounds.
        let c = s.add_marker(Marker::new(0, 70.013, "Late"));
        assert_eq!(s.marker(c).unwrap().time, 60.0);
        assert_eq!(s.move_marker(b, 30.01), Some(10.0));
        assert_eq!(s.markers.last().unwrap().id, c);
        assert_eq!(s.marker(b).unwrap().time, 30.0);
        s.selected = Some(b);
        assert_eq!(s.remove_marker(b).map(|m| m.label), Some("Verse".into()));
        assert_eq!(s.selected, None);
        assert_eq!(s.move_marker(b, 1.0), None);
    }

    #[test]
    fn up_and_down_walk_the_markers() {
        let mut s = state();
        let a = s.add_marker(Marker::new(0, 10.0, "A"));
        let b = s.add_marker(Marker::new(0, 20.0, "B"));
        let c = s.add_marker(Marker::new(0, 20.0, "C"));
        s.playhead = 15.0;
        assert_eq!(s.neighbour_marker(true), Some(b));
        assert_eq!(s.neighbour_marker(false), Some(a));
        s.selected = Some(b);
        assert_eq!(s.neighbour_marker(true), Some(c));
        assert_eq!(s.neighbour_marker(false), Some(a));
        s.selected = Some(c);
        assert_eq!(s.neighbour_marker(true), None);
        assert_eq!(s.neighbour_marker(false), Some(b));
    }

    #[test]
    fn in_and_out_points_make_a_range() {
        let mut s = state();
        assert_eq!(s.in_at(10.0), TimeRange::new(10.0, 60.0));
        assert_eq!(s.out_at(20.0), TimeRange::new(0.0, 20.0));
        s.range = Some(s.in_at(10.0));
        s.range = Some(s.out_at(20.0));
        assert_eq!(s.range, Some(TimeRange::new(10.0, 20.0)));
        // An out point before the in point starts the range again.
        assert_eq!(s.out_at(5.0), TimeRange::new(0.0, 5.0));
    }

    #[test]
    fn the_view_keeps_within_its_limits() {
        let s = state();
        let v = s.fit_view(TimeView::new(30.0, 0.001));
        assert_eq!(v.duration, 0.2);
        let v = s.fit_view(TimeView::new(-500.0, 30.0));
        assert_eq!(v.start, -1.5);
        let v = s.fit_view(TimeView::new(0.0, 1e9));
        assert_eq!(v.duration, 90.0);
        // Zooming keeps the time under the pointer in place.
        let z = TimeView::new(0.0, 60.0).zoomed(2.0, 15.0);
        assert_eq!((z.start, z.duration), (7.5, 30.0));
    }

    #[test]
    fn the_state_round_trips_through_json() {
        let mut s = state();
        s.add_marker(
            Marker::new(0, 1.0, "Drop")
                .kind(MarkerKind::Cue)
                .color(Color32::RED),
        );
        s.range = Some(TimeRange::new(1.0, 2.0));
        let json = serde_json::to_string(&s).unwrap();
        let back: TimelineState = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
        let partial: TimelineState = serde_json::from_str(r#"{"playhead": 3.0}"#).unwrap();
        assert_eq!(partial.playhead, 3.0);
    }
}
