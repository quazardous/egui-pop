# egui-pop-i18n

[![crates.io](https://img.shields.io/crates/v/egui-pop-i18n.svg)](https://crates.io/crates/egui-pop-i18n)
[![docs.rs](https://img.shields.io/docsrs/egui-pop-i18n)](https://docs.rs/egui-pop-i18n)

Translations for egui-pop applications, as a plugin: Fluent catalogues
per locale with fallback chains, a language setting, locale-aware
numbers, dates and plurals, a lookup step, a debug collector, and the
key extractor and checks for tools.

Part of [egui-pop](https://github.com/quazardous/egui-pop), an
application framework and component library for
[egui](https://github.com/emilk/egui). Most applications depend on
[`egui-pop`](https://crates.io/crates/egui-pop), which re-exports this
crate as `egui_pop::i18n` (feature `i18n`); depend on `egui-pop-i18n`
directly only to use this layer alone.

Guide:
[Translations](https://github.com/quazardous/egui-pop/blob/main/docs/i18n.md).
All of egui-pop's documentation: [the docs
index](https://github.com/quazardous/egui-pop/blob/main/docs/README.md).

License: MIT OR Apache-2.0.
