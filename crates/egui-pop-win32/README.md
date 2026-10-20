# egui-pop-win32

[![crates.io](https://img.shields.io/crates/v/egui-pop-win32.svg)](https://crates.io/crates/egui-pop-win32)
[![docs.rs](https://img.shields.io/docsrs/egui-pop-win32)](https://docs.rs/egui-pop-win32)

The Windows layer of egui-pop's window frame: subclasses a window to
answer WM_NCHITTEST from the frame's hit regions (Snap Layouts, Aero
Snap) and asks Windows 11 for rounded corners, with no egui dependency.

Part of [egui-pop](https://github.com/quazardous/egui-pop), an
application framework and component library for
[egui](https://github.com/emilk/egui). An internal layer of
[`egui-pop`](https://crates.io/crates/egui-pop), used by it on its
platform; applications do not depend on it directly.

Guide: [The
workbench](https://github.com/quazardous/egui-pop/blob/main/docs/workbench.md).
All of egui-pop's documentation: [the docs
index](https://github.com/quazardous/egui-pop/blob/main/docs/README.md).

License: MIT OR Apache-2.0.
