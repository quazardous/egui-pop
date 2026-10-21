# egui-pop-platform

[![crates.io](https://img.shields.io/crates/v/egui-pop-platform.svg)](https://crates.io/crates/egui-pop-platform)
[![docs.rs](https://img.shields.io/docsrs/egui-pop-platform)](https://docs.rs/egui-pop-platform)

Platform services for egui-pop applications, as opt-in plugins: a single
instance with deep links, desktop notifications with actions, a tray
icon with a declared menu, global shortcuts, logs and crash reports, the
focus mode and the text scaling read in-process.

Part of [egui-pop](https://github.com/quazardous/egui-pop), an
application framework and component library for
[egui](https://github.com/emilk/egui). Most applications depend on
[`egui-pop`](https://crates.io/crates/egui-pop), which re-exports this
crate as `egui_pop::platform` (features `platform` or
`platform-<service>`); depend on `egui-pop-platform` directly only to
use this layer alone.

Guide: [the platform
services](https://github.com/quazardous/egui-pop/blob/main/docs/README.md).
All of egui-pop's documentation: [the docs
index](https://github.com/quazardous/egui-pop/blob/main/docs/README.md).

License: MIT OR Apache-2.0.
