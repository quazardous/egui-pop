# egui-pop-viewport

[![crates.io](https://img.shields.io/crates/v/egui-pop-viewport.svg)](https://crates.io/crates/egui-pop-viewport)
[![docs.rs](https://img.shields.io/docsrs/egui-pop-viewport)](https://docs.rs/egui-pop-viewport)

A generic tiled image viewport for egui-pop: pan and zoom in physical
pixels, tiles from an asynchronous source (proxy then exact), a
display-transform hook, overlays with handles, before/after, a pixel
readout, thumbnails and a histogram.

Part of [egui-pop](https://github.com/quazardous/egui-pop), an
application framework and component library for
[egui](https://github.com/emilk/egui). Most applications depend on
[`egui-pop`](https://crates.io/crates/egui-pop), which re-exports this
crate as `egui_pop::viewport` (feature `viewport`); depend on
`egui-pop-viewport` directly only to use this layer alone.

Guide: [The image
viewport](https://github.com/quazardous/egui-pop/blob/main/docs/viewport.md).
All of egui-pop's documentation: [the docs
index](https://github.com/quazardous/egui-pop/blob/main/docs/README.md).

License: MIT.
