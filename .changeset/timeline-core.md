---
egui-pop-views: minor
---

A timeline, `views::timeline::Timeline`: a ruler of time over a pluggable
`TimeScale` (seconds; frames at a `FrameRate`, written as a timecode
`HH:MM:SS:FF`; bars and beats at a `Tempo`, written `bar.beat.tick`), with
an adaptive ruler whose labels never overlap, a view zoomed and panned
(Ctrl+wheel, pinch, the wheel, Ctrl+drag), a playhead to click or scrub, a
range set by Shift+drag or I and O that can loop, markers and cue points
(added, moved, removed, reached with Up and Down), snapping to the grid,
the markers and the playhead (Alt turns it off), and the transport keys
(Space, J/K/L shuttle, the arrows, Home and End). Time is an `f64` in
seconds. The application keeps a serializable `TimelineState` and moves
the time while it plays (`TimelineState::advance`); the widget reports the
play state and speed asked. With `Timeline::history`, each change of the
document (markers, range, loop) is one step of the history. The timeline
is an accessible slider over the time; `TimelinePlugin` adds the
`timeline` collector and the `timeline.set_time`, `set_range`,
`add_marker`, `remove_marker`, `play` and `pause` commands. Its look is
the theme's `timeline.*` tokens, falling back on system tokens.
