---
egui-pop-views: minor
egui-pop-i18n: minor
---

The timeline splits clips and ripple deletes: `TimelineState::split_clip`
cuts a clip in two at a time (the new part reads its source on), and
`ripple_remove_clips` removes clips and closes the gaps they leave on
their tracks. With the timeline focused, Ctrl+K splits the selected clips
at the playhead (none selected: every clip there) and Shift+Delete ripple
deletes the selection, each one step of the history; agents have
`timeline.split_clip` and `timeline.ripple_delete`. The output reports a
split as `Edit::ClipSplit`. English and French strings for both.
