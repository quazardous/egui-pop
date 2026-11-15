---
egui-pop-winit: patch
---

`egui-pop-winit` builds on its own on Linux: it turns on winit's X11 and Wayland backends itself (libwayland loaded at run time), where it used to rely on eframe turning them on beside it, so `cargo check -p egui-pop-winit` and docs.rs no longer fail with winit's "no backend" error.
