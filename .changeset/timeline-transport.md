---
egui-pop-views: minor
egui-pop-i18n: minor
---

The timeline keeps its playhead in place and gets a transport row.
`Timeline::follow_mode(Follow::Fixed(at))` (`Follow::CENTRE` for the
middle) keeps the playhead at a fixed place, playing or not, and scrolls
the content under it: a drag moves the content with the playhead, a click
moves the playhead, the wheel steps it by a unit (a beat, a frame) while
paused. `scrub_on_drag` locks the clips to the pointer (a drag on a clip
scrubs), and `nudge_on_drag` makes a drag nudge a playing timeline by a
quarter of the drag's speed, as a jog wheel's edge does. The new
`Transport` widget is a row of buttons over a `TimelineState` (go to
start, step back, play backward, stop, play or pause, step forward, go to
end, loop) and the playhead's text, each button running its key's
command; the transport keys have their commands: `timeline.toggle_play`,
`shuttle_forward`, `shuttle_backward`, `step_forward`, `step_backward`,
`go_to_start`, `go_to_end` and `toggle_loop`. English and French strings
for the buttons and the commands.
