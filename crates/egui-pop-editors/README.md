# egui-pop-editors

[![crates.io](https://img.shields.io/crates/v/egui-pop-editors.svg)](https://crates.io/crates/egui-pop-editors)
[![docs.rs](https://img.shields.io/docsrs/egui-pop-editors)](https://docs.rs/egui-pop-editors)

Text and code editors and a terminal for egui-pop: virtualised, themed,
highlighted through a pluggable highlighter (syntect by default), a
terminal over a PTY or a log console, every edit and session moment a
step an application can veto, rewrite or follow.

Part of [egui-pop](https://github.com/quazardous/egui-pop), an
application framework and component library for
[egui](https://github.com/emilk/egui). Most applications depend on
[`egui-pop`](https://crates.io/crates/egui-pop), which re-exports this
crate as `egui_pop::editors` (feature `editors`); depend on
`egui-pop-editors` directly only to use this layer alone.

Guide: [Text and code
editors](https://github.com/quazardous/egui-pop/blob/main/docs/editors.md).
All of egui-pop's documentation: [the docs
index](https://github.com/quazardous/egui-pop/blob/main/docs/README.md).

License: MIT OR Apache-2.0.
