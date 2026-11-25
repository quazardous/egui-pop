---
egui-pop-api: major
egui-pop: major
---

`workbench::Change` is `#[non_exhaustive]`: a new kind of workbench
change is no longer a breaking change. To move: a `match` on `Change`
outside egui-pop keeps a `_` arm.
