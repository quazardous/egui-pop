# egui-pop-model

[![crates.io](https://img.shields.io/crates/v/egui-pop-model.svg)](https://crates.io/crates/egui-pop-model)
[![docs.rs](https://img.shields.io/docsrs/egui-pop-model)](https://docs.rs/egui-pop-model)

The logic of egui-pop applications without egui: typed ids, commands,
when-clauses, keymaps, menus, panels, layouts, a value store, versioned
documents, plugin ordering.

Part of [egui-pop](https://github.com/quazardous/egui-pop), an
application framework and component library for
[egui](https://github.com/emilk/egui). Most applications depend on
[`egui-pop`](https://crates.io/crates/egui-pop), which re-exports this
crate as `egui_pop::model`; depend on `egui-pop-model` directly only to
use this layer alone.

Guide: [Plugins, commands and
steps](https://github.com/quazardous/egui-pop/blob/main/docs/plugins.md).
All of egui-pop's documentation: [the docs
index](https://github.com/quazardous/egui-pop/blob/main/docs/README.md).

License: MIT.
