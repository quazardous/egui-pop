# egui-pop-api

[![crates.io](https://img.shields.io/crates/v/egui-pop-api.svg)](https://crates.io/crates/egui-pop-api)
[![docs.rs](https://img.shields.io/docsrs/egui-pop-api)](https://docs.rs/egui-pop-api)

The application API of egui-pop: plugins that own what they register, an
app builder over eframe (or headless), zones and blocks, hooks and
steps, services, the workbench (panels in areas, docking, the window's
own frame), menus, dialogs, palettes and toasts, visual kits, and
gallery stories.

Part of [egui-pop](https://github.com/quazardous/egui-pop), an
application framework and component library for
[egui](https://github.com/emilk/egui). Most applications depend on
[`egui-pop`](https://crates.io/crates/egui-pop), which re-exports this
crate as `egui_pop::api` (and `egui_pop::prelude`); depend on
`egui-pop-api` directly only to use this layer alone.

Guide: [Getting
started](https://github.com/quazardous/egui-pop/blob/main/docs/getting-started.md).
All of egui-pop's documentation: [the docs
index](https://github.com/quazardous/egui-pop/blob/main/docs/README.md).

License: MIT.
