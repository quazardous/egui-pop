---
egui-pop-views: minor
---

A `Video` widget (`egui_pop_views::video`) plays a local video file in place,
each on its own player: `Video::new(path)` with `controls(Full | Compact |
None)`, `autoplay`, `looping`, `muted`, `volume`, `fit(Contain | Cover)`, a
`poster`, a `size` and an `id`. Its keys follow the viewer's (Space, Left and
Right, Up and Down), its picture is an accessible video node, and
`VideoPlugin` adds the `video.toggle`, `video.play`, `video.pause`,
`video.seek`, `video.volume` and `video.mute` commands and the `videos`
collector for agents. Without the `video` feature (or on the web) it draws a
placeholder saying so, with Open outside. The viewer's video page is now this
widget. GStreamer's player converts pictures on every core and makes egui's picture
in one pass (4K VP9 decoded by the GPU now plays in real time, where it fell
behind at about 20 frames a second), and loops without a gap through segment
seeks (`VideoPlayer::set_looping`). The video types
moved to `egui_pop_views::video`; they are still named under `viewer::`.
