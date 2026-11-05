# Timeline

`egui_pop::views::timeline::Timeline` is a ruler of time: a playhead to
scrub, a range that loops, markers and cue points, snapping, and the
transport keys of video editors and DJ software. One widget serves a data
viewer's time panel (seconds), a video editor (frames and timecode) and a
DJ application (bars and beats): only its [`TimeScale`](#time-scales)
changes.

This is the timeline's core. Tracks and clips, drawn through a painter the
application gives, come next, on top of it ([below](#what-comes-next)).

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
| `play`, `speed` | what the user asked: `Paused` or `Playing`, and the speed (negative backwards) |

Its document part, `TimelineDoc` (`doc`, `set_doc`), is the markers, the
range and the loop: what the history keeps.

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
| `lanes(height)` | a control's height | the lanes under the ruler and the markers |
| `width(points)` | the width available | |
| `snap(Snap)` | all on | what snapping goes to: `grid`, `markers` (and the range's ends), `playhead`; `Snap::OFF` for none |
| `app(&AppHandle)`, `history(path)` | none | each change of the document a step of the history ([below](#history)) |
| `surface(name)` | `timeline` | its focus surface, for key bindings |
| `take_focus(bool)` | off | takes the keyboard focus this pass |
| `follow(bool)` | on | the view follows the playhead while it plays |

`show(ui)` draws it; `show_with(ui, |ui, lanes| ...)` draws what goes in
the lanes too (rows of data, a waveform), before the markers and the
playhead are drawn over it: `Lanes::x_of` and `time_at` map time and
points. Rerun-like time panels draw their rows of data ticks this way.

`TimelineOutput` says what happened: `playhead_moved`, `scrubbing`,
`transport_changed`, `play`, `speed`, `view_changed`, and `edits`, the
document's changes (`Edit::MarkerAdded`, `MarkerMoved`, `MarkerRemoved`,
`RangeSet`, `LoopSet`, and `Restored` when the history put the document
back).

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

Snapping pulls a time to a marker, the range's ends or the playhead within
8 points (`SNAP_REACH`), the nearest first; otherwise onto the grid (the
ruler's small ticks). A scrub does not snap to the playhead it moves, a
dragged marker not to itself.

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
| Ctrl+← / Ctrl+→ (Shift: bigger) | moves the selected marker one unit | |
| Delete, Backspace | removes the selected marker | |
| Escape | lets the selected marker go | |
| `+` / `-` | zooms in / out around the playhead | |

### History

With `app(app).history(path)`, the document (markers, range, loop) is kept
as text at `path` of the application's value store (declared there if it is
not), and each change is one step of egui-pop's history ([History](history.md)):
adding, moving (a drag is one step, from where it was to where it lands),
removing a marker, setting or clearing the range, looping. Ctrl+Z undoes it
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

The ruler's height is `size.control.height`, its labels `font.scale.xs` in
the monospace font.

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

Without `timeline`, the one that last had the focus, else the only one
drawn. A command is checked at once and answered with the timeline as the
collector shows it; the widget applies it at its next pass, as a step of
the history made by the agent.

The `timeline` collector lists each timeline drawn with an id: its scale,
view, bounds, playhead (seconds and text), range (and its text), loop,
markers (id, seconds, text, label, kind, colour), the selected marker, the
play state and speed; and the one that last had the focus.

## The gallery

The **Timeline** page shows one timeline per scale: a 90-second recording
in seconds (its lanes a data viewer's rows of events, no grid snapping),
two minutes of video at 25 frames per second with a looping range, and a
track at 128 BPM with coloured cue points, a loop of four bars and a kick
drawn on every beat. A pretend player moves each playhead while it plays,
on the motion clock (frozen in the tests, so the snapshots stand still).

## What comes next

Tracks and clips: rows under the ruler, each holding clips (a start, a
length, a source offset) drawn by a painter trait the application gives
(a video's thumbnails, an audio waveform, a data plot), with selection,
trimming and moving clips as steps of the history. They build on this
core: `Lanes` for the geometry, `TimeView` and `Ticks` for the mapping
and the grid, `ruler::snap` with clip edges as more targets, the
`Timeline`'s pointer and keys, and `TimelineDoc` grown with the tracks.
Then two demos: a DJ deck (beat grid, hot cues, loops) and a video editor
(timecode, J/K/L, trimming).

Not in this core: drop-frame timecode, tempo changes along a track, and
a range's edges dragged directly (Shift+drag draws a new one).
