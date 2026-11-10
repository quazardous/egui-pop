---
egui-pop-test: patch
---

A new `snapshot` feature compares a scenario's shots as images (with
`wgpu`); without it a shot records the accessibility tree. `wgpu` alone no
longer needs `egui_kittest`'s `snapshot`, so `cargo doc --all-features`
without the gallery builds again, and the image-diffing tools stay out of
the crate's normal dependencies.
