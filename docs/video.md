# Video

`egui_pop::views::video::Video` plays a local video file in place: a
tutorial with full controls, a clip in a library with compact ones, a
preview that loops without sound and without controls. The in-app viewer's
video page ([Links](links.md#the-video-page)) is this widget. The choices
behind it are in the [video survey](video-survey.md).

```rust,ignore
use egui_pop::views::video::{Controls, Fit, Video};

// A tutorial: every control, an id for agents.
Video::new(&tour).id("tour").app(app).show(ui);

// A clip in a list: Play or Pause, the position and the time on one row.
Video::new(&clip).controls(Controls::Compact).size(egui::vec2(320.0, 220.0)).show(ui);

// A preview: no controls, playing at once, again and again, silent.
Video::new(&gesture)
    .controls(Controls::None)
    .autoplay(true)
    .looping(true)
    .muted(true)
    .fit(Fit::Cover)
    .size(egui::vec2(240.0, 135.0))
    .show(ui);
```

| Builder | Default | What |
|---|---|---|
| `controls(Full \| Compact \| None)` | `Full` | `Full`: the position on its own row, then Play or Pause, the time and the volume. `Compact`: Play or Pause, the position and the time on one row. `None`: the picture only, not a Tab stop |
| `autoplay(bool)` | off | plays as soon as it opens, and again when drawn after being out of view |
| `looping(bool)` | off | starts again at the end; read every pass |
| `muted(bool)` | off | starts without sound; the volume slider or `video.mute` gives it back |
| `volume(f32)` | 1 | the volume it opens with, 0 to 1 |
| `fit(Contain \| Cover)` | `Contain` | all of the picture with bars, or all of the room with the picture cropped |
| `poster(texture)` | its first frame | a picture shown until it first plays or moves |
| `size(Vec2)` | the available width, the picture as high as its shape asks | its whole size, controls included |
| `id(name)` | none | its name for agents, and its key in the context |
| `label(text)` | "Video: " and the file's name | its accessible name |
| `backend(Option<Arc<dyn VideoBackend>>)` | the build's | another player; `None` draws the placeholder |
| `app(&AppHandle)` | none | **Open outside** goes through the links service |
| `surface(name)`, `take_focus(bool)` | `video`, off | the focus surface around the picture, for key bindings; the picture takes the focus |

`show` returns a `VideoOutput`: the picture's response, the state, the
position, the length, the volume, whether it is muted, an error, whether
there is no player, and whether **Open outside** was clicked.

## How it plays

- **One player per widget**, opened when it is first drawn and paused on its
  first picture (unless `autoplay`). Several videos on one page each have
  their own pipeline.
- **Off the interface's thread**: GStreamer's threads decode and convert;
  each pass takes only the last picture and uploads it. A playing video asks
  for a repaint a few times a second, for the time; paused, ended or loading,
  nothing keeps the application awake.
- **Out of view**: a widget no longer drawn (a page left, a panel closed, a
  tab behind another) has its player stopped after a few passes. Drawn again,
  it opens the file at the same place, paused (playing with `autoplay`).
- **Keys**, while the picture has the focus: Space plays or pauses, Left and
  Right move by five seconds, Up and Down change the volume. A click on the
  picture plays or pauses and gives it the focus. A key the application binds
  on the picture's surface runs its command instead (the viewer binds its
  own).
- **Accessibility**: the picture is a `Video` node named after the file (or
  `label`); Play or Pause is a button named for what it does; the position
  and the volume are sliders.

### Loops

GStreamer's player loops by itself, without a gap: with `looping` on, it
seeks with GStreamer's `SEGMENT` flag, so the end of the file posts a
"segment done" message instead of ending the stream, and from there a
second segment seek back to the start, without a flush, carries the decoders
and the sinks straight on. Measured headless on a generated clip at ten
pictures a second (100 ms apart), the longest wait between two pictures
across loops was 102 to 112 ms: no gap
(`crates/egui-pop-views/tests/video_gstreamer.rs`).

A host's own player can do the same (`VideoPlayer::set_looping` answers
whether it does). One that cannot (the default) is looped by the widget: at
the end it seeks back to the start and plays, a flushing seek that leaves a
short gap, the time the pipeline takes to pre-roll again: about 150 ms
between two pictures on the same clip, half a frame more than playing on.

### The picture's path, and its cost

`playbin` opens the file and picks the decoders; a `videoconvert` turns each
picture into RGBA on every core (`n-threads` 0), and the player makes egui's
picture of it in one pass. With a hardware decoder (VA-API here), the decoded
picture is copied back to memory before that conversion, which on one core
(`playbin`'s default) cannot keep up with 4K.

Measured headless on 2026-11-04 (Fedora 44, GStreamer 1.28, an AMD Radeon
680M with Mesa's VA-API, twelve threads, a machine shared with other work), on
10 s of 4K VP9 at 30 pictures a second:

| What | Converting on one core | On every core |
|---|---|---|
| `gst-launch-1.0`, decode and convert only, unsynchronised | 24 a second (12.3 s) | 62 a second (4.9 s) |
| The player, unsynchronised, pictures as egui's | 17 to 20 a second | 38 to 41 a second |
| The player, playing in time | 17 to 23 a second, the 10 s taking 13 to 18 s (before this change) | 30 a second, in time (4 runs out of 4) |

Converting on every core with the old two-pass copy was no better (12 to 15
a second): the copy, then 15 to 55 ms a picture, was the other half; in one
pass it takes about 4 ms. Uploading the YUV planes and converting on the GPU
is a later step ([survey](video-survey.md#follow-ups-ranked)).

## Without a player

Without the `video` feature, on the web, or with `backend(None)`, the widget
draws a placeholder saying that the application has no video player, with
**Open outside**. A player that fails (a missing plugin, a file it cannot
read) is replaced by the failure and **Open outside**; the viewer turns it
into its error page.

**What it needs** with the feature: to build, GStreamer's development
packages (`libgstreamer1.0-dev` and `libgstreamer-plugins-base1.0-dev` on
Debian and Ubuntu); to run, GStreamer's libraries, its base plugins, and the
plugins for the files to play. The binary links GStreamer's libraries, so an
application declares them as a dependency of its package.

## Agents

`VideoPlugin` declares the commands and the `videos` collector; the widgets
play without it.

| Command | Arguments | Does |
|---|---|---|
| `video.toggle` | `video` | plays or pauses; from the end, plays from the start |
| `video.play`, `video.pause` | `video` | plays, pauses |
| `video.seek` | `video`, `seconds` or `by` | moves to a position, or by an amount, kept within the video |
| `video.volume` | `video`, `volume` (0 to 1) or `by` | sets or changes the volume, and gives the sound back |
| `video.mute` | `video`, `muted` (else switched) | mutes or gives the sound back |

`video` is a widget's id; without it, the one whose picture last had the
focus, else the only one with an id. The `videos` collector shows each video
with an id: its file, backend, controls, `state` (`loading`, `paused`,
`playing`, `ended`), `playing`, `position_ms`, `duration_ms`, `volume`,
`muted`, `looping`, `loops`, the picture's `size`, a `note` or an `error`,
and whether it is `stopped` out of view. `Videos::of(ctx)` gives the same
from code.

## Tests

Tests give a pretend `VideoBackend` (`crates/egui-pop-views/tests/video.rs`);
`write_test_clip` writes a small clip from GStreamer's test sources for the
tests that run the real player (`viewer_gstreamer.rs`, CI's `video` job) and
for the gallery's Video page (`cargo run -p egui-pop-gallery --features
video`).
