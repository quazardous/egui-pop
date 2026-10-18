# egui-pop-imaging

[![crates.io](https://img.shields.io/crates/v/egui-pop-imaging.svg)](https://crates.io/crates/egui-pop-imaging)
[![docs.rs](https://img.shields.io/docsrs/egui-pop-imaging)](https://docs.rs/egui-pop-imaging)

The image tools of egui-pop in an application: the histogram's drag, the
curve editor, the colour wheels and the zoom control and navigator, and
the node graph editor, each finished change a step an application can
veto, redirect, replace or follow.

Part of [egui-pop](https://github.com/quazardous/egui-pop), an
application framework and component library for
[egui](https://github.com/emilk/egui). Most applications depend on
[`egui-pop`](https://crates.io/crates/egui-pop), which re-exports this
crate as `egui_pop::imaging` (feature `imaging`); depend on
`egui-pop-imaging` directly only to use this layer alone.

Guide: [Image
tools](https://github.com/quazardous/egui-pop/blob/main/docs/image-tools.md).
All of egui-pop's documentation: [the docs
index](https://github.com/quazardous/egui-pop/blob/main/docs/README.md).

License: MIT.
