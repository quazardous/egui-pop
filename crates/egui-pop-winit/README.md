# egui-pop-winit

[![crates.io](https://img.shields.io/crates/v/egui-pop-winit.svg)](https://crates.io/crates/egui-pop-winit)
[![docs.rs](https://img.shields.io/docsrs/egui-pop-winit)](https://docs.rs/egui-pop-winit)

The winit layer of egui-pop's runner: a winit 0.30 window id as the
window's raw handles (Win32, X11, Wayland), so every window of an event
loop is reachable, with no egui dependency.

Part of [egui-pop](https://github.com/quazardous/egui-pop), an
application framework and component library for
[egui](https://github.com/emilk/egui). An internal layer of
[`egui-pop`](https://crates.io/crates/egui-pop), used by its runner;
applications do not depend on it directly.

Why it exists: [eframe and egui-pop](https://github.com/quazardous/egui-pop/blob/main/docs/eframe-comparison.md).
All of egui-pop's documentation: [the docs
index](https://github.com/quazardous/egui-pop/blob/main/docs/README.md).

License: MIT OR Apache-2.0.
