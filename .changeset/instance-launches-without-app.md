---
egui-pop-platform: minor
---

A single instance without `App`: `Launches::take` (the launches handed
over since the last call) and `Launches::set_waker` (the egui context a
launch repaints) are public. An application takes the `Launches` from
`Single::First(instance)` with `instance.launches()`, sets its context
as the waker and takes the launches in its frame.
