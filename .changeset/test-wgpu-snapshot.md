---
egui-pop-test: patch
---

The `wgpu` feature also turns on `egui_kittest`'s `snapshot` feature, which
`Scenario`'s shots need for `try_snapshot`. Before, a build with `wgpu` but
without another crate enabling `snapshot` (for example `cargo doc
--all-features` without the gallery) failed to compile.
