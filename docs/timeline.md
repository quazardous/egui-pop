# Timeline

`egui_pop::views::timeline::Timeline` is a ruler of time: a playhead to
scrub, a range that loops, markers and cue points, snapping, and the
transport keys of video editors and DJ software. One widget serves a data
viewer's time panel (seconds), a video editor (frames and timecode) and a
DJ application (bars and beats): only its [`TimeScale`](#time-scales)
changes.

Under the ruler, [tracks](#tracks-and-clips) hold clips (a start, a
length, an offset into a source) that an application's painter draws: a
video's thumbnails, an audio waveform, a data plot. They are selected,
moved within and across tracks and trimmed by their edges.

```rust,ignore
use egui_pop::views::timeline::{Marker, MarkerKind, TimeScale, Timeline, TimelineState};

// The application keeps the state (serializable)...
let mut song = TimelineState::new(TimeScale::beats(128.0), 240.0);
song.add_marker(Marker::new(0, 30.0, "Drop").kind(MarkerKind::Cue));

// ...moves the time while it plays...
song.advance(dt);

// ...and draws it each frame.
let out = Timeline::new(&mut song)
    .id("song")
    .app(app)
    .history("doc/song/timeline")
    .show(ui);
if out.transport_changed {
    player.set_speed(if song.is_playing() { song.speed } else { 0.0 });
}
```

## Time

Time is an `f64` in seconds, everywhere: the playhead, the range, the
markers, the view. A scale only reads it in its own units.

Why not integer ticks: no tick rate divides every scale. Frame rates such
as 29.97 (30000/1001) are not a whole number of any common tick, a tempo
of 128.5 BPM gives beats that are not either, and a timeline may change its
scale while its markers stay where they are. An `f64` keeps about 15
significant digits: at 24 hours it still tells apart times a hundred
nanoseconds apart, far finer than an audio sample at 192 kHz. Where whole
units matter the scale rounds: on frames, the playhead, markers and ranges
always land on a whole frame (`TimeScale::quantize`), and snapping lands
on the grid's exact lines.

## Time scales

| Scale | Text | Ruler | ←/→, Shift+←/→ |
|---|---|---|---|
| `Seconds` | `1:02.500`, `1:02:03.250` | 1 ms to days, labels as precise as their step (`0:05`, `0:05.5`) | 1 s, 10 s |
| `Frames(FrameRate)` | timecode `HH:MM:SS:FF` (counted without dropping frames: 29.97 counts 30 to a second) | 1, 2, 5, 10 frames, then timecode seconds and minutes | a frame, a timecode second |
| `Beats(Tempo)` | `bar.beat.tick` from `1.1.000` | sixteenths of a beat to bars, then 2 to 1024 bars | a beat, a bar |

`Tempo::new(bpm, beats_per_bar, beat_unit)` counts 960 ticks to a beat
(`ticks_per_beat`), and `offset` puts bar 1 where a track's first downbeat
is. A scale converts (`to_units`, `from_units`), writes (`format`, and
`label` at a ruler step's precision), reads (`parse`, which also takes the
shorter forms `1:02`, `12.5`, `17.2`), gives its ruler's steps (`steps`)
and the grid (`round_to`).

### The ruler

`ruler::ticks` picks, for the view and the width, the finest step whose
labels fit: each label is measured (its widest, at the view's ends and
middle) and needs its width plus `LABEL_GAP` (10 points) between two
ticks. Under it, the minor step is the finest one that divides it and
keeps `MIN_TICK_GAP` (6 points). Labelled ticks draw a grid line down the
lanes; the minor ticks are the grid snapping lands on. `ruler::marks`
lists the ticks in view, and the widget still skips a label that would
touch the one before it.

## State

`TimelineState` is everything the timeline shows, serializable with serde:

| Field | What |
|---|---|
| `scale` | the [time scale](#time-scales) |
| `view` | the part in view: `start` and `duration` |
| `bounds` | the document's span (`TimelineState::new(scale, length)` gives `0..length`): the playhead stays inside, the view near it |
| `playhead` | the current time |
| `range`, `looping` | the selected range, and whether playing goes round it |
| `markers` | `Marker { id, time, label, kind, color }`, in time order; `kind` is `Marker` (a flag) or `Cue` (a pin), `color` the theme's when `None` |
| `selected` | the marker the keys act on |
| `tracks` | rows of [clips](#tracks-and-clips), from the top |
| `selected_clips` | the clips selected, the last one the keys act on |
| `play`, `speed` | what the user asked: `Paused` or `Playing`, and the speed (negative backwards) |

Its document part, `TimelineDoc` (`doc`, `set_doc`), is the markers, the
range, the loop and the tracks with their clips: what the history keeps.

**Playing is the application's job.** The widget only says what was
asked: `play` and `speed` in the state and the output, and
`transport_changed` the pass it changes. The application moves the time,
from its player's clock or its own; `TimelineState::advance(dt)` does it
the usual way (round the range when looping, otherwise stopping and
pausing at the bounds), and the application repaints while it plays.
While it plays, the view follows the playhead a page at a time
(`Timeline::follow`).

## The widget

| Builder | Default | What |
|---|---|---|
| `id(name)` | none | its name for agents, and its key in the context |
| `label(text)` | "Timeline" | its accessible name |
| `lanes(height)` | a control's height, none with tracks | the lanes under the tracks |
| `track_height(points)` | twice a control's height | a track's height, unless the track says |
| `clips(&mut painter)` | `PlainClips` | what draws the clips' content ([below](#painting-clips)) |
| `width(points)` | the width available | |
| `snap(Snap)` | all on | what snapping goes to: `grid`, `markers` (and the range's ends), `playhead`, `clips` (their edges); `Snap::OFF` for none |
| `app(&AppHandle)`, `history(path)` | none | each change of the document a step of the history ([below](#history)) |
| `surface(name)` | `timeline` | its focus surface, for key bindings |
| `take_focus(bool)` | off | takes the keyboard focus this pass |
| `follow(bool)` | on | the view follows the playhead while it plays |

`show(ui)` draws it; `show_with(ui, |ui, lanes| ...)` draws what goes in
the lanes too (rows of data, a beat grid), before the clips, the markers
and the playhead are drawn over it: `Lanes::x_of` and `time_at` map time and
points. Rerun-like time panels draw their rows of data ticks this way.

`TimelineOutput` says what happened: `playhead_moved`, `scrubbing`,
`transport_changed`, `play`, `speed`, `view_changed`, and `edits`, the
document's changes (`Edit::MarkerAdded`, `MarkerMoved`, `MarkerRemoved`,
`RangeSet`, `LoopSet`, `ClipAdded`, `ClipMoved`, `ClipTrimmed`,
`ClipRemoved`, `ClipSplit`, and `Restored` when the history put the document back;
`Edit` is non-exhaustive), and `tracks`, each track's id and row on
screen, for what an application draws beside them.

### Pointer

| Gesture | Does |
|---|---|
| Press on the ruler or the lanes | moves the playhead there (snapped) and takes the focus |
| Drag | scrubs |
| Shift+drag | selects a range (snapped) |
| Ctrl+drag, middle drag | pans |
| Wheel (either axis) | pans |
| Ctrl+wheel, pinch | zooms around the pointer |
| Click on a marker | selects it, the playhead goes there |
| Drag a marker | moves it: one step of the history when dropped |
| Double click on the markers' lane | adds a marker there |
| Alt held | no snapping (frames still land on whole frames) |

Snapping pulls a time to a marker, the range's ends, the playhead or a
clip's edge within 8 points (`SNAP_REACH`), the nearest first; otherwise
onto the grid (the ruler's small ticks). A scrub does not snap to the
playhead it moves, a dragged marker or clip not to itself. A clip moved
snaps by whichever of its edges is nearer a point (`ruler::snap_span`),
else its start onto the grid.

### Keys

While the timeline has the focus (it is one Tab stop). A key an
application binds on the timeline's surface runs its command instead.

| Key | Does | Convention |
|---|---|---|
| Space | plays forward at 1, or pauses | everywhere |
| L | plays forward; again: twice as fast (up to 32×) | NLE J/K/L shuttle |
| J | plays backward; again: twice as fast | NLE |
| K | pauses (the speed back to 1) | NLE |
| K held + J or L | one unit back or forward | NLE |
| ← / → | one unit back or forward (pauses) | |
| Shift+← / Shift+→ | a bigger step (10 s, a timecode second, a bar) | |
| Home / End | the bounds' start / end | |
| I / O | the range's in / out point at the playhead | NLE |
| Shift+I / Shift+O | goes to the in / out point | Premiere |
| Alt+X | clears the range | Premiere, Resolve |
| Ctrl+L | loops the range, or no longer | Ableton |
| M | adds a marker at the playhead, "Marker 3" | Premiere, Resolve |
| ↑ / ↓ | selects the previous / next marker, the playhead goes there (announced) | Resolve's Shift+↑/↓ |
| Ctrl+← / Ctrl+→ (Shift: bigger) | moves the selected marker, else the selected clips, one unit | |
| Page Up / Page Down | selects the previous / next clip on the selected one's track (else the first track's, from the playhead), the playhead goes to its start (announced) | Premiere's edit points |
| Shift+↑ / Shift+↓ | selects the clip at the playhead (else the nearest) on the track above / below (announced) | egui-pop's |
| Ctrl+↑ / Ctrl+↓ | moves the selected clips a track up / down, where they fit | egui-pop's |
| `[` / `]` | trims the selected clip's start / end to the playhead | Resolve's Shift+[ / ] |
| Delete, Backspace | removes the selected marker, else the selected clips | |
| Shift+Delete, Shift+Backspace | removes the selected clips and closes their gaps (ripple delete) | Premiere |
| Ctrl+K | splits the selected clips at the playhead; none selected: every clip there | Premiere's Add Edit |
| Escape | lets the selected marker and clips go | |
| `+` / `-` | zooms in / out around the playhead | |

### Tracks and clips

```rust,ignore
use egui_pop::views::timeline::{Clip, TimeScale, Timeline, TimelineState, Track};

let mut edit = TimelineState::new(TimeScale::frames(25), 120.0);
let v1 = edit.add_track(Track::new(0, "V1").kind("video"));
let a1 = edit.add_track(Track::new(0, "A1").kind("audio").height(40.0));
edit.add_clip(v1, Clip::new(0, 0.0, 12.0, "Wide shot")
    .source("wide.mov")      // what it plays: the application's
    .offset(3.0)             // the source read from 3 s in
    .source_length(20.0))?;  // trimming stops at its ends

Timeline::new(&mut edit).clips(&mut my_painter).show(ui);
```

A `Track` (`id`, `name`, `kind`, `height`, `clips`) is a row under the
markers' lane; its `kind` is free text for the application's painter
(`video`, `audio`, `data`). A `Clip` is `id` (unique in the timeline),
`start` and `length` on the timeline, `offset` into its source, `label`,
`source` (what it plays, which the timeline never reads),
`source_length` (`None`: a source without ends, a still or a generator)
and `color`. Time is in seconds, as everywhere in the timeline; on frames
a clip's start, length and offset are whole frames. `source_at(time)`
and `time_of(source_time)` map the timeline and the source.

The rules, kept by `TimelineState` (each method usable without the
widget, for an application's own commands):

- A track's clips are in time order and **never overlap** (touching end
  to start is fine), and stay inside the bounds.
- `add_clip(track, clip)` and `move_clips(&[ClipMove])` (several at once,
  across tracks) are **refused** where they would overlap or leave the
  bounds (`ClipError`): nothing changes.
- `trim_clip(id, ClipEdge, time)` **stops** at the source's ends, the
  neighbours, the bounds and the shortest a clip can be
  (`min_clip_length`: a frame, else the ruler's finest step); trimming the
  start moves the offset with it, so the picture stays where it was.
- `split_clip(id, time)` cuts a clip in two at `time` (a whole frame on
  frames): it keeps its id and ends there, a new clip (the next id,
  returned) starts there and reads its source on; refused unless both
  parts are at least `min_clip_length` long.
- `ripple_remove_clips(&ids)` removes clips and closes the gaps they
  leave: the clips after each on its track move back by its length
  (other tracks stay).
- `remove_clips(&ids)`; `clip`, `clip_track`, `clips()` find them;
  `neighbour_clip` and `clip_across` are what the keys walk.

With the pointer:

| Gesture | Does |
|---|---|
| Click on a clip | selects it alone; Ctrl: in or out of the selection; Shift: added |
| Drag a clip | moves the selection along its track or to others, snapped; where it does not fit, it lands against the nearest edge that lets it (another clip's, the bounds'); one step when dropped |
| Drag a clip's edge | trims it (the cursor turns to a resize arrow over an edge) |
| Press on a track, off the clips | lets the clips go, and moves the playhead as anywhere |
| Middle drag on a clip | pans |
| Alt held | no snapping |

The playhead stays where it is when a clip is clicked or dragged. Each
clip is an accessible button, "Clip Wide shot on V1, 00:00:00:00 to
00:00:12:00", selected or not; a screen reader's click selects it.

#### Painting clips

The timeline draws each clip's box (its colour, else
`timeline.clip.fill`), its outline, its selection and the edge the
pointer would trim; a `ClipPainter` draws what is inside, clipped to the
clip in view:

```rust,ignore
impl ClipPainter for MyPainter {
    fn paint(&mut self, painter: &egui::Painter, clip: &ClipPaint<'_>) {
        if clip.track.kind == "audio" {
            let (from, to) = clip.source_in_view(); // seconds of the source
            // ... a waveform across clip.content, x by clip.x_of_source(t)
        }
        clip.paint_label(painter); // what PlainClips draws
    }
}
```

`ClipPaint` gives the clip and its track, `rect` (the whole clip, often
wider than the lanes when zoomed in), `visible` (the part in view),
`content` (under the label when the track is tall enough), the `lanes`
and the `scale` for mapping, `selected`, `hovered` and the `tokens`. A
closure `|painter, clip| ...` is a painter too. `PlainClips`, the
default, draws the labels only, each kept in sight at the left of the
part in view.

### History

With `app(app).history(path)`, the document (markers, range, loop, tracks
and clips) is kept
as text at `path` of the application's value store (declared there if it is
not), and each change is one step of egui-pop's history ([History](history.md)):
adding, moving (a drag is one step, from where it was to where it lands),
removing a marker, setting or clearing the range, looping, adding,
moving, trimming, splitting and removing clips (clips moved, split or
removed together are one step; a ripple delete, its removals and moves,
is one). Selecting is not a step. Ctrl+Z undoes it
and the timeline takes the document back at its next pass
(`Edit::Restored`). The playhead, the view and playing are not steps. An
agent's change is a step named as the agent's.

Without it, the edits are in the output for an application keeping its own
history.

### Accessibility

The timeline is a slider: its value is the playhead's text in the scale
(`00:01:02:12`), with its numeric value in seconds, the bounds as its range
and a unit as its step; a screen reader's Increment, Decrement and Set
Value move the playhead. Each marker is a named node, "Marker Drop at
1:02.500" ("Cue ..." for a cue point); ↑ and ↓ reach them from the
keyboard and announce the one reached.

### Look

Tokens only, each `timeline.*` path falling back on the system token it
aliases (`TimelineTokens`), so a theme restyles it with data:

| Token | Fallback | What |
|---|---|---|
| `timeline.background` | `surface.sunken` | behind the lanes |
| `timeline.ruler.fill`, `.border`, `.tick`, `.text` | `surface.raised`, `border.default`, `text.muted`, `text.muted` | the ruler |
| `timeline.grid` | `border.default` | lines down the lanes at the labels |
| `timeline.range.fill` | `accent.subtle` | the range down the lanes |
| `timeline.range.edge`, `timeline.range.idle` | `accent.default`, `text.muted` | its band and edges, looping or not |
| `timeline.playhead` | `accent.default` | the playhead |
| `timeline.marker` | `info` | a marker without a colour of its own |
| `timeline.marker.text`, `.selected` | `text.default`, `focus.ring` | its label, the selected one's outline |
| `timeline.track.border`, `.text` | `border.default`, `text.muted` | the line under a track, its name |
| `timeline.clip.fill`, `.border`, `.text` | `surface.raised`, `border.strong`, `text.default` | a clip without a colour of its own, its label |
| `timeline.clip.selected` | `accent.default` | a selected clip's outline, the edge being trimmed |

The ruler's height is `size.control.height`, its labels `font.scale.xs` in
the monospace font; a track is twice `size.control.height` high unless it
or the timeline says.

## For agents: `timeline.*`

`TimelinePlugin` declares the commands and the `timeline` collector; the
widgets work without it. A timeline is reached by its `id`.

| Command | Arguments | Does |
|---|---|---|
| `timeline.set_time` | `timeline`, `seconds`, `time` (text in its scale) or `by` | moves the playhead |
| `timeline.set_range` | `timeline`, `start` and `end`, or `clear`; `loop` | sets or clears the range, loops it |
| `timeline.add_marker` | `timeline`, `seconds` or `time` (else the playhead), `label`, `kind` (`marker`, `cue`), `color` (`#rrggbb`) | adds a marker, answers its id in `added` |
| `timeline.remove_marker` | `timeline`, `id` | removes a marker |
| `timeline.play` | `timeline`, `speed` | asks to play |
| `timeline.pause` | `timeline` | asks to pause |
| `timeline.add_clip` | `timeline`, `track`, `seconds` or `time` (else the playhead), `length`, `offset`, `label`, `source`, `source_length`, `color` | adds a clip, answers its id in `added`; refused where it would overlap |
| `timeline.move_clip` | `timeline`, `id`, `seconds` or `time`, `track` | moves a clip (what is not given stays); refused where it would overlap or leave the bounds |
| `timeline.trim_clip` | `timeline`, `id`, `start`, `end` | moves a clip's start or end (seconds on the timeline), stopping as a drag does |
| `timeline.remove_clip` | `timeline`, `id` or `ids` | removes clips, one step together |
| `timeline.split_clip` | `timeline`, `id` or `ids`, `seconds` or `time` (else the playhead) | splits clips there, one step together; answers the new clips' ids in `added`; refused unless the time is inside each |
| `timeline.ripple_delete` | `timeline`, `id` or `ids` | removes clips and closes their gaps, one step |
| `timeline.select_clips` | `timeline`, `ids` | selects clips (`[]`: none); not a step |

Without `timeline`, the one that last had the focus, else the only one
drawn. A command is checked at once (a refusal says why: `it would overlap clip
4`, `no clip 9`) and answered with the timeline as the collector shows it; the widget applies it at its next pass, as a step of
the history made by the agent.

The `timeline` collector lists each timeline drawn with an id: its scale,
view, bounds, playhead (seconds and text), range (and its text), loop,
markers (id, seconds, text, label, kind, colour), the selected marker, the
tracks (id, name, kind) with their clips (id, seconds, end, length,
offset, text, label, source, source length, colour), the clips selected,
the play state and speed; and the one that last had the focus.

## The gallery

The **Timeline** page shows one timeline per scale: a 90-second recording
in seconds (its lanes a data viewer's rows of events, no grid snapping),
two minutes of video at 25 frames per second with a looping range, and a
track at 128 BPM with coloured cue points, a loop of four bars and a kick
drawn on every beat. A pretend player moves each playhead while it plays,
on the motion clock (frozen in the tests, so the snapshots stand still).

The **Tracks and clips** page is a forty-second edit at 25 frames per
second: a titles track, two video tracks and two audio tracks, drawn by
a painter of the gallery's (pretend thumbnails, one per second of the
source, and a waveform, both from the source's time, so a clip trimmed
or moved keeps its picture and its sound in place; the titles with
`PlainClips`).

The **DJ decks** page puts two decks on bars and beats (128 and 124 BPM),
each a timeline with one track holding its song, drawn by a painter of
the gallery's: a pretend waveform, louder on the beats and in the drop,
under a beat grid with the bars brighter. Hot cues 1 to 8 are cue points
coloured per pad (set on the nearest beat, or jumped to), the loop is the
range (I and O, halved and doubled from its start), the pitch fader
scales the pretend clock, and Sync gives a deck the other's tempo and
moves it by less than half a beat onto the other's phase. The deck's keys
(1 to 8, Shift with them, `[`, `]`, S) are commands of the page bound on
the decks' surface (`Timeline::surface`); J/K/L, I, O and Ctrl+L stay the
timeline's.

## What comes next

A video editor demo on top (timecode, J/K/L, trimming, splitting).

What the DJ decks needed that the timeline does not give, worked around
by the page:

- **A view centred on the playhead**: `follow` turns pages; a deck keeps
  its playhead in the middle and scrolls under it, which the page does by
  moving `view` each frame.
- **Locked tracks or clips**: a deck's song can be dragged, trimmed or
  deleted like any clip; the page puts it back after each pass.
- **Which timeline a surface key is for**: a command bound on a surface
  two timelines share is not told which one has the focus; the page
  remembers the deck whose timeline had it last.
- **An application's data on a marker**: a hot cue's pad lives in its
  label; a marker has no field of the application's (a slot, a type).
- **An application's own edits as steps**: changing `TimelineState`
  directly is not a step of the history; only the widget's gestures and
  the `timeline.*` commands are (the decks keep no history, as DJ
  software does not).

Not in the timeline yet: drop-frame timecode, tempo changes along a
track, a range's edges dragged directly (Shift+drag draws a new one);
for clips, splitting at the playhead, ripple and roll edits, overwriting
a clip by dropping another on it (a drop lands beside it instead), a
rubber band selection, track headers (the output's `tracks` rows leave
room for the application's), and adding or removing tracks by agents.
