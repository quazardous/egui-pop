# egui-pop-core

[![crates.io](https://img.shields.io/crates/v/egui-pop-core.svg)](https://crates.io/crates/egui-pop-core)
[![docs.rs](https://img.shields.io/docsrs/egui-pop-core)](https://docs.rs/egui-pop-core)

The egui layer of egui-pop: theme, focus scopes, a key dispatcher,
overlays, placement, accessibility helpers, strings, icons, persistence,
fault boundaries, value binding and a debug bridge.

Part of [egui-pop](https://github.com/quazardous/egui-pop), an
application framework and component library for
[egui](https://github.com/emilk/egui). Most applications depend on
[`egui-pop`](https://crates.io/crates/egui-pop), which re-exports this
crate as `egui_pop::core` (and `egui_pop::theme`); depend on
`egui-pop-core` directly only to use this layer alone.

Guide: [Visual
kits](https://github.com/quazardous/egui-pop/blob/main/docs/kit.md). All
of egui-pop's documentation: [the docs
index](https://github.com/quazardous/egui-pop/blob/main/docs/README.md).

License: MIT OR Apache-2.0.
