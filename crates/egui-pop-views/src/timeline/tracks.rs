//! Tracks and clips: rows under a timeline's ruler, each holding clips (a
//! start, a length, an offset into a source), and how an application draws
//! them ([`ClipPainter`]). The model is plain data, without egui's input:
//! adding, moving, trimming and removing clips, never overlapping on a
//! track and kept inside the timeline's bounds.

use egui::{Color32, FontId, Painter, Rect, pos2};
use serde::{Deserialize, Serialize};

use super::Lanes;
use super::scale::TimeScale;
use super::state::{Edit, TimelineState};
use super::tokens::TimelineTokens;

/// Two times this close are the same (seconds): clips touching end to start
/// do not overlap.
const EPSILON: f64 = 1e-9;

/// A piece of a source placed on a track: from `start`, `length` long, the
/// source read from `offset` (all in seconds). A video's shot, a sample, a
/// span of recorded data.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Clip {
    /// Unique among all the clips of its timeline.
    pub id: u64,
    pub start: f64,
    pub length: f64,
    /// Where in its source it starts (seconds into the source).
    #[serde(default)]
    pub offset: f64,
    #[serde(default)]
    pub label: String,
    /// What it plays, for the application and its painter: a file, a
    /// sample's name, a stream. The timeline does not read it.
    #[serde(default)]
    pub source: String,
    /// How long its source is: trimming stops at its ends. `None`: a source
    /// without an end (a still, a generator).
    #[serde(default)]
    pub source_length: Option<f64>,
    /// Its own colour; the theme's (`timeline.clip.fill`) when `None`.
    #[serde(default)]
    pub color: Option<Color32>,
}

impl Clip {
    pub fn new(id: u64, start: f64, length: f64, label: impl Into<String>) -> Self {
        Self {
            id,
            start,
            length,
            offset: 0.0,
            label: label.into(),
            source: String::new(),
            source_length: None,
            color: None,
        }
    }

    /// Reads its source from `offset` seconds in.
    pub fn offset(mut self, offset: f64) -> Self {
        self.offset = offset;
        self
    }

    pub fn source(mut self, source: impl Into<String>) -> Self {
        self.source = source.into();
        self
    }

    /// Its source lasts `length` seconds: trimming stops at its ends.
    pub fn source_length(mut self, length: f64) -> Self {
        self.source_length = Some(length);
        self
    }

    pub fn color(mut self, color: Color32) -> Self {
        self.color = Some(color);
        self
    }

    pub fn end(&self) -> f64 {
        self.start + self.length
    }

    /// Whether `time` is on it (its start included, its end not).
    pub fn contains(&self, time: f64) -> bool {
        time >= self.start && time < self.end()
    }

    /// The source's time playing at the timeline's `time`.
    pub fn source_at(&self, time: f64) -> f64 {
        self.offset + (time - self.start)
    }

    /// The timeline's time at which the source's `source_time` plays.
    pub fn time_of(&self, source_time: f64) -> f64 {
        self.start + (source_time - self.offset)
    }

    /// Its span: start, length, offset.
    pub fn span(&self) -> ClipSpan {
        ClipSpan {
            start: self.start,
            length: self.length,
            offset: self.offset,
        }
    }
}

/// A row of clips under the ruler.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Track {
    /// Unique among its timeline's tracks.
    pub id: u64,
    pub name: String,
    /// What its clips are, for the application's painter (`video`, `audio`,
    /// `data`...). The timeline does not read it.
    #[serde(default)]
    pub kind: String,
    /// Its height in points; the timeline's (`Timeline::track_height`) when
    /// `None`.
    #[serde(default)]
    pub height: Option<f32>,
    /// In time order, never overlapping.
    #[serde(default)]
    pub clips: Vec<Clip>,
}

impl Track {
    pub fn new(id: u64, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            ..Self::default()
        }
    }

    pub fn kind(mut self, kind: impl Into<String>) -> Self {
        self.kind = kind.into();
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    fn sort(&mut self) {
        self.clips
            .sort_by(|a, b| a.start.total_cmp(&b.start).then(a.id.cmp(&b.id)));
    }
}

/// Which end of a clip a trim moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClipEdge {
    Start,
    End,
}

/// Where a clip is: its track and its start.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClipPlace {
    pub track: u64,
    pub start: f64,
}

/// What a trim changes: a clip's start, length and offset.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClipSpan {
    pub start: f64,
    pub length: f64,
    pub offset: f64,
}

/// A clip to move: to `track`, starting at `start`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipMove {
    pub id: u64,
    pub track: u64,
    pub start: f64,
}

/// Why a change of the clips was refused.
#[derive(Clone, Debug, PartialEq)]
pub enum ClipError {
    NoTrack(u64),
    NoClip(u64),
    /// It would overlap this clip.
    Overlaps(u64),
    /// Outside the timeline's bounds.
    OutOfBounds,
    /// Not a length a clip can have.
    BadLength,
}

impl std::fmt::Display for ClipError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClipError::NoTrack(id) => write!(f, "no track {id}"),
            ClipError::NoClip(id) => write!(f, "no clip {id}"),
            ClipError::Overlaps(id) => write!(f, "it would overlap clip {id}"),
            ClipError::OutOfBounds => write!(f, "outside the timeline's bounds"),
            ClipError::BadLength => write!(f, "a clip's length is a positive number"),
        }
    }
}

impl std::error::Error for ClipError {}

/// A clip being moved, as it was when the move began.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Origin {
    pub id: u64,
    /// Its track's index.
    pub row: usize,
    pub start: f64,
    pub length: f64,
}

impl TimelineState {
    pub fn track(&self, id: u64) -> Option<&Track> {
        self.tracks.iter().find(|t| t.id == id)
    }

    pub fn track_mut(&mut self, id: u64) -> Option<&mut Track> {
        self.tracks.iter_mut().find(|t| t.id == id)
    }

    /// Adds a track below the others (its id taken if free, else the next
    /// one); returns the id.
    pub fn add_track(&mut self, mut track: Track) -> u64 {
        if track.id == 0 || self.track(track.id).is_some() {
            track.id = self.tracks.iter().map(|t| t.id).max().map_or(1, |m| m + 1);
        }
        track.sort();
        let id = track.id;
        self.tracks.push(track);
        id
    }

    /// Where clip `id` is: its track's index and its own in the track.
    pub(super) fn locate(&self, id: u64) -> Option<(usize, usize)> {
        self.tracks
            .iter()
            .enumerate()
            .find_map(|(row, t)| t.clips.iter().position(|c| c.id == id).map(|at| (row, at)))
    }

    pub fn clip(&self, id: u64) -> Option<&Clip> {
        let (row, at) = self.locate(id)?;
        Some(&self.tracks[row].clips[at])
    }

    /// The track clip `id` is on.
    pub fn clip_track(&self, id: u64) -> Option<&Track> {
        self.locate(id).map(|(row, _)| &self.tracks[row])
    }

    /// Every clip with its track, track by track.
    pub fn clips(&self) -> impl Iterator<Item = (&Track, &Clip)> {
        self.tracks
            .iter()
            .flat_map(|t| t.clips.iter().map(move |c| (t, c)))
    }

    /// The id the next clip takes.
    pub fn next_clip_id(&self) -> u64 {
        self.clips().map(|(_, c)| c.id).max().map_or(1, |m| m + 1)
    }

    /// The shortest a clip can be: a frame, else the ruler's finest step.
    pub fn min_clip_length(&self) -> f64 {
        match self.scale {
            TimeScale::Frames(r) => r.frame(),
            s => s.steps().first().copied().unwrap_or(0.001),
        }
    }

    /// The clip on track `row` that `start..end` would overlap, besides the
    /// clips of `skip`.
    fn overlap(&self, row: usize, start: f64, end: f64, skip: &[u64]) -> Option<u64> {
        self.tracks[row]
            .clips
            .iter()
            .filter(|c| !skip.contains(&c.id))
            .find(|c| start < c.end() - EPSILON && c.start < end - EPSILON)
            .map(|c| c.id)
    }

    fn in_bounds(&self, start: f64, end: f64) -> bool {
        self.bounds
            .is_none_or(|b| start >= b.start - EPSILON && end <= b.end + EPSILON)
    }

    /// Adds `clip` to track `track` (its id taken if free, else the next
    /// one), on the scale's units: refused where it would overlap a clip or
    /// leave the bounds. Returns the id.
    pub fn add_clip(&mut self, track: u64, mut clip: Clip) -> Result<u64, ClipError> {
        let row = self
            .tracks
            .iter()
            .position(|t| t.id == track)
            .ok_or(ClipError::NoTrack(track))?;
        if !clip.length.is_finite() || clip.length <= 0.0 || !clip.start.is_finite() {
            return Err(ClipError::BadLength);
        }
        clip.start = self.scale.quantize(clip.start);
        clip.length = self.scale.quantize(clip.length).max(self.min_clip_length());
        clip.offset = self.scale.quantize(clip.offset.max(0.0));
        if !self.in_bounds(clip.start, clip.end()) {
            return Err(ClipError::OutOfBounds);
        }
        if let Some(other) = self.overlap(row, clip.start, clip.end(), &[]) {
            return Err(ClipError::Overlaps(other));
        }
        if clip.id == 0 || self.clip(clip.id).is_some() {
            clip.id = self.next_clip_id();
        }
        let id = clip.id;
        let t = &mut self.tracks[row];
        t.clips.push(clip);
        t.sort();
        Ok(id)
    }

    /// Moves clips at once, each to its track and start (on the scale's
    /// units): refused, nothing moved, where one would overlap a clip or
    /// leave the bounds. The edits, one per clip that moved.
    pub fn move_clips(&mut self, moves: &[ClipMove]) -> Result<Vec<Edit>, ClipError> {
        let mut placed: Vec<(usize, u64, f64, f64)> = Vec::new();
        let ids: Vec<u64> = moves.iter().map(|m| m.id).collect();
        for m in moves {
            let (row, at) = self.locate(m.id).ok_or(ClipError::NoClip(m.id))?;
            let length = self.tracks[row].clips[at].length;
            let to = self
                .tracks
                .iter()
                .position(|t| t.id == m.track)
                .ok_or(ClipError::NoTrack(m.track))?;
            let start = self.scale.quantize(m.start);
            if !start.is_finite() || !self.in_bounds(start, start + length) {
                return Err(ClipError::OutOfBounds);
            }
            if let Some(other) = self.overlap(to, start, start + length, &ids) {
                return Err(ClipError::Overlaps(other));
            }
            // Nor over one another.
            if let Some(&(_, other, ..)) = placed.iter().find(|(r, _, s, l)| {
                *r == to && start < s + l - EPSILON && *s < start + length - EPSILON
            }) {
                return Err(ClipError::Overlaps(other));
            }
            placed.push((to, m.id, start, length));
        }
        let mut edits = Vec::new();
        for (to, id, start, _) in placed {
            if let Some((from, place)) = self.place(id, to, start)
                && from != place
            {
                edits.push(Edit::ClipMoved {
                    id,
                    from,
                    to: place,
                });
            }
        }
        Ok(edits)
    }

    /// Puts clip `id` on track `row` at `start`, unchecked: where it was
    /// and where it is.
    pub(super) fn place(
        &mut self,
        id: u64,
        row: usize,
        start: f64,
    ) -> Option<(ClipPlace, ClipPlace)> {
        let (from_row, at) = self.locate(id)?;
        let from = ClipPlace {
            track: self.tracks[from_row].id,
            start: self.tracks[from_row].clips[at].start,
        };
        let mut clip = self.tracks[from_row].clips.remove(at);
        clip.start = start;
        let track = self.tracks.get_mut(row)?;
        let to = ClipPlace {
            track: track.id,
            start,
        };
        track.clips.push(clip);
        track.sort();
        Some((from, to))
    }

    /// Moves clip `id`'s `edge` to `time`, kept on its source (never before
    /// its start nor past its end), off its neighbours, inside the bounds,
    /// and at least [`min_clip_length`](Self::min_clip_length) long. Its
    /// span before.
    pub fn trim_clip(&mut self, id: u64, edge: ClipEdge, time: f64) -> Result<ClipSpan, ClipError> {
        let (row, at) = self.locate(id).ok_or(ClipError::NoClip(id))?;
        if !time.is_finite() {
            return Err(ClipError::OutOfBounds);
        }
        let min = self.min_clip_length();
        let clips = &self.tracks[row].clips;
        let c = &clips[at];
        let before = c.span();
        let (prev_end, next_start) = (
            at.checked_sub(1).map(|i| clips[i].end()),
            clips.get(at + 1).map(|n| n.start),
        );
        let t = self.scale.quantize(time);
        let bounds = self.bounds;
        let c = &mut self.tracks[row].clips[at];
        match edge {
            ClipEdge::Start => {
                let end = c.end();
                let mut lo = c.start - c.offset;
                if let Some(p) = prev_end {
                    lo = lo.max(p);
                }
                if let Some(b) = bounds {
                    lo = lo.max(b.start);
                }
                let start = t.max(lo).min(end - min);
                c.offset += start - c.start;
                c.start = start;
                c.length = end - start;
            }
            ClipEdge::End => {
                let mut hi = f64::INFINITY;
                if let Some(n) = next_start {
                    hi = hi.min(n);
                }
                if let Some(b) = bounds {
                    hi = hi.min(b.end);
                }
                if let Some(s) = c.source_length {
                    hi = hi.min(c.start + (s - c.offset));
                }
                let end = t.min(hi).max(c.start + min);
                c.length = end - c.start;
            }
        }
        Ok(before)
    }

    /// Splits clip `id` at `time` (on the scale's units) into two clips
    /// touching there: the clip keeps its id and ends at `time`, the new
    /// one (the next id) starts there, reading its source on from where
    /// the first stops. Refused unless both parts are at least
    /// [`min_clip_length`](Self::min_clip_length) long. The new clip's id.
    pub fn split_clip(&mut self, id: u64, time: f64) -> Result<u64, ClipError> {
        let (row, at) = self.locate(id).ok_or(ClipError::NoClip(id))?;
        let t = self.scale.quantize(time);
        let min = self.min_clip_length();
        let next = self.next_clip_id();
        let c = &mut self.tracks[row].clips[at];
        if !t.is_finite() || t < c.start + min - EPSILON || t > c.end() - min + EPSILON {
            return Err(ClipError::BadLength);
        }
        let mut second = c.clone();
        second.id = next;
        second.start = t;
        second.length = c.end() - t;
        second.offset = c.offset + (t - c.start);
        c.length = t - c.start;
        let track = &mut self.tracks[row];
        track.clips.push(second);
        track.sort();
        Ok(next)
    }

    /// Removes clips and closes the gaps they leave: on each track, the
    /// clips after a removed one move back by its length. The clips
    /// removed (each with its track's id), and an edit per clip moved.
    pub fn ripple_remove_clips(&mut self, ids: &[u64]) -> (Vec<(u64, Clip)>, Vec<Edit>) {
        let removed = self.remove_clips(ids);
        let before: Vec<(u64, ClipPlace)> = self
            .tracks
            .iter()
            .flat_map(|t| {
                t.clips.iter().map(|c| {
                    let place = ClipPlace {
                        track: t.id,
                        start: c.start,
                    };
                    (c.id, place)
                })
            })
            .collect();
        // The last gap first, so that the ones before still see the clips
        // after them where they were.
        let mut gaps: Vec<&(u64, Clip)> = removed.iter().collect();
        gaps.sort_by(|a, b| b.1.start.total_cmp(&a.1.start));
        for (track, gone) in gaps {
            if let Some(t) = self.tracks.iter_mut().find(|t| t.id == *track) {
                for c in &mut t.clips {
                    if c.start >= gone.end() - EPSILON {
                        c.start = self.scale.quantize((c.start - gone.length).max(gone.start));
                    }
                }
                t.sort();
            }
        }
        let mut moved = Vec::new();
        for (id, from) in before {
            if let Some(c) = self.clip(id)
                && c.start != from.start
            {
                let to = ClipPlace {
                    track: from.track,
                    start: c.start,
                };
                moved.push(Edit::ClipMoved { id, from, to });
            }
        }
        (removed, moved)
    }

    /// Removes clips: those removed, each with its track's id.
    pub fn remove_clips(&mut self, ids: &[u64]) -> Vec<(u64, Clip)> {
        let mut removed = Vec::new();
        for id in ids {
            if let Some((row, at)) = self.locate(*id) {
                removed.push((self.tracks[row].id, self.tracks[row].clips.remove(at)));
            }
        }
        self.selected_clips.retain(|id| !ids.contains(id));
        removed
    }

    /// The clips selected that still exist, the selection kept in order.
    pub(super) fn prune_selection(&mut self) {
        let alive: Vec<u64> = self
            .selected_clips
            .iter()
            .copied()
            .filter(|id| self.locate(*id).is_some())
            .collect();
        self.selected_clips = alive;
    }

    /// The shift nearest `dt` (seconds) by which the clips of `origin`,
    /// `drow` tracks down, fit: off the other clips and inside the bounds.
    /// `None` when no shift fits.
    pub(super) fn fit_shift(&self, origin: &[Origin], dt: f64, drow: isize) -> Option<f64> {
        let ids: Vec<u64> = origin.iter().map(|o| o.id).collect();
        let rows = self.tracks.len() as isize;
        let target = |o: &Origin| {
            let r = o.row as isize + drow;
            (0..rows).contains(&r).then_some(r as usize)
        };
        if origin.iter().any(|o| target(o).is_none()) {
            return None;
        }
        let fits = |d: f64| {
            origin.iter().all(|o| {
                let s = o.start + d;
                let row = target(o).unwrap_or(0);
                self.in_bounds(s, s + o.length)
                    && self.overlap(row, s, s + o.length, &ids).is_none()
            })
        };
        if fits(dt) {
            return Some(dt);
        }
        // Against an edge: of another clip, or of the bounds.
        let mut candidates = Vec::new();
        for o in origin {
            let row = target(o).unwrap_or(0);
            for c in self.tracks[row]
                .clips
                .iter()
                .filter(|c| !ids.contains(&c.id))
            {
                candidates.push(c.end() - o.start);
                candidates.push(c.start - (o.start + o.length));
            }
            if let Some(b) = self.bounds {
                candidates.push(b.start - o.start);
                candidates.push(b.end - (o.start + o.length));
            }
        }
        candidates
            .into_iter()
            .filter(|d| d.is_finite() && fits(*d))
            .min_by(|a, b| (a - dt).abs().total_cmp(&(b - dt).abs()))
    }

    /// The clip after (`forward`) or before the one selected last, on its
    /// track; with none selected, the first after the playhead (or the last
    /// before it) on the first track holding clips.
    pub fn neighbour_clip(&self, forward: bool) -> Option<u64> {
        if let Some((row, at)) = self.selected_clips.last().and_then(|id| self.locate(*id)) {
            let clips = &self.tracks[row].clips;
            return if forward {
                clips.get(at + 1).map(|c| c.id)
            } else {
                at.checked_sub(1).map(|i| clips[i].id)
            };
        }
        let t = self.tracks.iter().find(|t| !t.clips.is_empty())?;
        let p = self.playhead;
        if forward {
            t.clips
                .iter()
                .find(|c| c.start >= p - EPSILON)
                .map(|c| c.id)
        } else {
            t.clips
                .iter()
                .rev()
                .find(|c| c.end() <= p + EPSILON)
                .map(|c| c.id)
        }
    }

    /// The clip under the playhead (else the nearest to it) on the next
    /// track holding clips above (`up`) or below the selected clip's track
    /// (with none selected, from the last or the first track).
    pub fn clip_across(&self, up: bool) -> Option<u64> {
        let rows = self.tracks.len();
        let from = self
            .selected_clips
            .last()
            .and_then(|id| self.locate(*id))
            .map(|(row, _)| row);
        let order: Vec<usize> = match (from, up) {
            (Some(r), true) => (0..r).rev().collect(),
            (Some(r), false) => (r + 1..rows).collect(),
            (None, true) => (0..rows).rev().collect(),
            (None, false) => (0..rows).collect(),
        };
        let p = self.playhead;
        let distance = |c: &Clip| {
            if c.contains(p) {
                0.0
            } else {
                (c.start - p).abs().min((c.end() - p).abs())
            }
        };
        order.into_iter().find_map(|row| {
            self.tracks[row]
                .clips
                .iter()
                .min_by(|a, b| distance(a).total_cmp(&distance(b)))
                .map(|c| c.id)
        })
    }
}

/// What a [`ClipPainter`] draws a clip with: the clip, its track, where it is
/// on screen, and the mapping of time.
#[derive(Debug)]
pub struct ClipPaint<'a> {
    pub clip: &'a Clip,
    pub track: &'a Track,
    /// The whole clip, often wider than the lanes when zoomed in.
    pub rect: Rect,
    /// The part of it in view.
    pub visible: Rect,
    /// Where its content goes: under its label when the track is tall
    /// enough, else the whole clip.
    pub content: Rect,
    /// The lanes, for [`Lanes::x_of`] and [`Lanes::time_at`].
    pub lanes: &'a Lanes,
    pub scale: &'a TimeScale,
    pub selected: bool,
    pub hovered: bool,
    pub tokens: &'a TimelineTokens,
}

impl ClipPaint<'_> {
    /// The x of the source's `source_time`.
    pub fn x_of_source(&self, source_time: f64) -> f32 {
        self.lanes.x_of(self.clip.time_of(source_time))
    }

    /// The source's time at `x`.
    pub fn source_at(&self, x: f32) -> f64 {
        self.clip.source_at(self.lanes.time_at(x))
    }

    /// The source's times in view: from, to.
    pub fn source_in_view(&self) -> (f64, f64) {
        (
            self.source_at(self.visible.left()),
            self.source_at(self.visible.right()),
        )
    }

    /// The text its label is drawn in.
    pub fn text_color(&self) -> Color32 {
        self.tokens.clip_text
    }

    /// Its label at the top left of the part in view (so a long clip keeps
    /// it in sight), as [`PlainClips`] draws it.
    pub fn paint_label(&self, painter: &Painter) {
        if self.clip.label.is_empty() {
            return;
        }
        let font = FontId::proportional(self.tokens.font_size);
        let galley = painter.layout_no_wrap(self.clip.label.clone(), font, self.tokens.clip_text);
        let at = pos2(self.visible.left() + 4.0, self.rect.top() + 2.0);
        if at.x + 8.0 < self.visible.right() {
            painter.galley(at, galley, self.tokens.clip_text);
        }
    }
}

/// How an application draws its clips' content: a video's thumbnails, an
/// audio waveform, a data plot. The timeline draws a clip's box, its
/// selection and its trimming handles; the painter what is inside, clipped
/// to the clip.
pub trait ClipPainter {
    /// Draws `clip` with `painter` (clipped to the clip in view).
    fn paint(&mut self, painter: &Painter, clip: &ClipPaint<'_>);
}

/// The plain painter: each clip's label, nothing else.
#[derive(Clone, Copy, Debug, Default)]
pub struct PlainClips;

impl ClipPainter for PlainClips {
    fn paint(&mut self, painter: &Painter, clip: &ClipPaint<'_>) {
        clip.paint_label(painter);
    }
}

impl<F: FnMut(&Painter, &ClipPaint<'_>)> ClipPainter for F {
    fn paint(&mut self, painter: &Painter, clip: &ClipPaint<'_>) {
        self(painter, clip)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::state::TimeRange;

    /// Two tracks at 25 fps over a minute: V1 holds clips at 0..10 and
    /// 20..30, A1 one at 5..15.
    fn edit() -> TimelineState {
        let mut s = TimelineState::new(TimeScale::frames(25), 60.0);
        let v1 = s.add_track(Track::new(0, "V1").kind("video"));
        let a1 = s.add_track(Track::new(0, "A1").kind("audio"));
        s.add_clip(v1, Clip::new(0, 0.0, 10.0, "One").source_length(12.0))
            .unwrap();
        s.add_clip(
            v1,
            Clip::new(0, 20.0, 10.0, "Two")
                .offset(2.0)
                .source_length(20.0),
        )
        .unwrap();
        s.add_clip(a1, Clip::new(0, 5.0, 10.0, "Sound")).unwrap();
        s
    }

    #[test]
    fn a_split_makes_two_clips_reading_the_source_on() {
        let mut s = edit();
        // Clip 2: 20..30 on the timeline, its source from 2 s in.
        let new = s.split_clip(2, 24.0).unwrap();
        assert_eq!(new, 4);
        let (a, b) = (s.clip(2).unwrap(), s.clip(4).unwrap());
        assert_eq!((a.start, a.length, a.offset), (20.0, 4.0, 2.0));
        assert_eq!((b.start, b.length, b.offset), (24.0, 6.0, 6.0));
        assert_eq!(b.source_length, Some(20.0));
        assert_eq!(s.clip_track(4).unwrap().name, "V1");
        // Not at an end, nor nearer it than a frame, nor off the clip.
        assert_eq!(s.split_clip(2, 20.0), Err(ClipError::BadLength));
        assert_eq!(s.split_clip(2, 23.99), Err(ClipError::BadLength));
        assert_eq!(s.split_clip(4, 40.0), Err(ClipError::BadLength));
        assert_eq!(s.split_clip(9, 1.0), Err(ClipError::NoClip(9)));
        // On frames, the cut lands on a whole frame.
        let new = s.split_clip(1, 3.013).unwrap();
        assert_eq!(s.clip(new).unwrap().start, 3.0);
    }

    #[test]
    fn a_ripple_delete_closes_the_gaps_on_their_tracks() {
        let mut s = edit();
        s.split_clip(2, 25.0).unwrap(); // V1: 0..10, 20..25, 25..30
        let (removed, moved) = s.ripple_remove_clips(&[1, 2]);
        assert_eq!(removed.len(), 2);
        // 25..30 moves back by both: 5 s for clip 2, then 10 for clip 1.
        assert_eq!(s.clip(4).unwrap().start, 10.0);
        assert_eq!(
            moved,
            [Edit::ClipMoved {
                id: 4,
                from: ClipPlace {
                    track: 1,
                    start: 25.0
                },
                to: ClipPlace {
                    track: 1,
                    start: 10.0
                },
            }]
        );
        // Other tracks stay.
        assert_eq!(s.clip(3).unwrap().start, 5.0);
    }

    #[test]
    fn clips_never_overlap_on_a_track_nor_leave_the_bounds() {
        let mut s = edit();
        assert_eq!(s.next_clip_id(), 4);
        assert_eq!(
            s.add_clip(1, Clip::new(0, 8.0, 5.0, "Over")),
            Err(ClipError::Overlaps(1))
        );
        assert_eq!(
            s.add_clip(1, Clip::new(0, 55.0, 10.0, "Late")),
            Err(ClipError::OutOfBounds)
        );
        assert_eq!(
            s.add_clip(9, Clip::new(0, 40.0, 1.0, "")),
            Err(ClipError::NoTrack(9))
        );
        // Touching end to start is no overlap; on whole frames.
        let id = s
            .add_clip(1, Clip::new(0, 10.013, 9.98, "Between"))
            .unwrap();
        assert_eq!(s.clip(id).map(|c| (c.start, c.end())), Some((10.0, 20.0)));
        assert_eq!(
            s.track(1)
                .unwrap()
                .clips
                .iter()
                .map(|c| c.id)
                .collect::<Vec<_>>(),
            [1, id, 2],
            "in time order"
        );
    }

    #[test]
    fn moves_are_all_or_nothing_and_cross_tracks() {
        let mut s = edit();
        // Onto the sound: refused, nothing moved.
        let refused = s.move_clips(&[ClipMove {
            id: 1,
            track: 2,
            start: 0.0,
        }]);
        assert_eq!(refused, Err(ClipError::Overlaps(3)));
        assert_eq!(s.clip_track(1).map(|t| t.id), Some(1));
        let edits = s
            .move_clips(&[ClipMove {
                id: 1,
                track: 2,
                start: 40.0,
            }])
            .unwrap();
        assert_eq!(
            edits,
            [Edit::ClipMoved {
                id: 1,
                from: ClipPlace {
                    track: 1,
                    start: 0.0
                },
                to: ClipPlace {
                    track: 2,
                    start: 40.0
                },
            }]
        );
        assert_eq!(s.track(2).unwrap().clips.len(), 2);
        // Two moved together may take each other's place.
        let swap = s.move_clips(&[
            ClipMove {
                id: 1,
                track: 2,
                start: 5.0,
            },
            ClipMove {
                id: 3,
                track: 2,
                start: 40.0,
            },
        ]);
        assert_eq!(swap.map(|e| e.len()), Ok(2));
        // Nor over one another.
        let both = s.move_clips(&[
            ClipMove {
                id: 1,
                track: 1,
                start: 40.0,
            },
            ClipMove {
                id: 3,
                track: 1,
                start: 45.0,
            },
        ]);
        assert_eq!(both, Err(ClipError::Overlaps(1)));
    }

    #[test]
    fn trimming_keeps_to_the_source_the_neighbours_and_a_frame() {
        let mut s = edit();
        // The end: no further than the source (12 s), nor the next clip.
        s.trim_clip(1, ClipEdge::End, 30.0).unwrap();
        assert_eq!(s.clip(1).unwrap().end(), 12.0);
        s.trim_clip(1, ClipEdge::End, 3.0).unwrap();
        assert_eq!(s.clip(1).unwrap().length, 3.0);
        // The start: the offset follows, never before the source's start.
        let before = s.trim_clip(2, ClipEdge::Start, 25.0).unwrap();
        assert_eq!(
            before,
            ClipSpan {
                start: 20.0,
                length: 10.0,
                offset: 2.0
            }
        );
        let c = s.clip(2).unwrap();
        assert_eq!((c.start, c.length, c.offset), (25.0, 5.0, 7.0));
        s.trim_clip(2, ClipEdge::Start, 0.0).unwrap();
        let c = s.clip(2).unwrap();
        assert_eq!(
            (c.start, c.offset),
            (18.0, 0.0),
            "the source starts at 18 s"
        );
        // Never shorter than a frame.
        s.trim_clip(2, ClipEdge::Start, 59.0).unwrap();
        assert!((s.clip(2).unwrap().length - 0.04).abs() < 1e-9);
        // What plays where.
        let c = s.clip(2).unwrap();
        assert!((c.source_at(c.start) - c.offset).abs() < 1e-9);
        assert!((c.time_of(c.offset + 0.02) - (c.start + 0.02)).abs() < 1e-9);
    }

    #[test]
    fn a_shift_that_does_not_fit_lands_against_the_nearest_edge() {
        let s = edit();
        let one = Origin {
            id: 1,
            row: 0,
            start: 0.0,
            length: 10.0,
        };
        assert_eq!(s.fit_shift(&[one], 5.0, 0), Some(5.0), "free");
        assert_eq!(s.fit_shift(&[one], 15.0, 0), Some(10.0), "against Two");
        assert_eq!(s.fit_shift(&[one], 22.0, 0), Some(30.0), "past Two");
        assert_eq!(s.fit_shift(&[one], -3.0, 0), Some(0.0), "the bounds");
        assert_eq!(s.fit_shift(&[one], 8.0, 1), Some(15.0), "after Sound");
        assert_eq!(s.fit_shift(&[one], 8.0, 2), None, "no third track");
    }

    #[test]
    fn walking_the_clips() {
        let mut s = edit();
        s.playhead = 12.0;
        assert_eq!(s.neighbour_clip(true), Some(2));
        assert_eq!(s.neighbour_clip(false), Some(1));
        s.selected_clips = vec![2];
        assert_eq!(s.neighbour_clip(false), Some(1));
        assert_eq!(s.neighbour_clip(true), None);
        // Across: the clip under the playhead on the next track down.
        assert_eq!(s.clip_across(false), Some(3));
        assert_eq!(s.clip_across(true), None);
        s.selected_clips.clear();
        assert_eq!(s.clip_across(false), Some(1), "nearest on the first");
        assert_eq!(s.clip_across(true), Some(3), "from the last");
    }

    #[test]
    fn removing_lets_go_of_the_selection_and_the_doc_keeps_the_tracks() {
        let mut s = edit();
        s.selected_clips = vec![1, 3];
        let removed = s.remove_clips(&[3]);
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].0, 2);
        assert_eq!(s.selected_clips, [1]);
        let doc = s.doc();
        assert_eq!(doc.tracks.len(), 2);
        s.remove_clips(&[1]);
        s.selected_clips = vec![1];
        s.set_doc(doc);
        assert!(s.clip(1).is_some());
        let json = serde_json::to_string(&s).unwrap();
        let back: TimelineState = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
        // No bounds: anywhere.
        s.bounds = None;
        assert!(s.in_bounds(-5.0, 1e6));
        s.bounds = Some(TimeRange::new(0.0, 60.0));
        assert!(!s.in_bounds(-5.0, 1.0));
    }
}
