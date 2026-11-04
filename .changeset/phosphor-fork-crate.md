---
egui-pop-core: patch
---

The Phosphor icon font comes from `egui-pop-phosphor` 0.15 on crates.io,
a fork of egui-phosphor carrying its pending update to egui 0.37, in
place of a git patch: egui-pop no longer needs a `[patch.crates-io]`
entry, in its workspace or in an application depending on it. The
library is still `egui_phosphor`, and the icons are unchanged. egui-pop
returns to egui-phosphor once upstream releases for egui 0.37 (#146).
