---
egui-pop-model: minor
egui-pop-api: patch
egui-pop-views: major
egui-pop-editors: major
egui-pop: patch
---

`egui-pop-views` and `egui-pop-editors` can be used without the application layer: their dependency on `egui-pop-api` is now the `api` feature, on by default, so nothing changes for an application that keeps the defaults. Built with `default-features = false`, a view, an editor or a terminal is a plain egui widget on `egui-pop-core` and `egui-pop-model` only (the `.app(..)` and `.shared(..)` options, the steps' plugins and the library's `change` come with `api`); each crate's documentation shows one drawn that way, and `docs/standalone.md` names the lightest set. **Breaking** for a dependency declared with `default-features = false` that uses those items: it adds `features = ["api"]` (the facade's `editors` feature does). The viewer features (`markdown`, `html`, `video`, `webview`) turn `api` on. `SelectionItem` moves to `egui_pop_model::selection` and stays re-exported from `egui_pop_api::selection`. CI checks that neither crate, without its default features, depends on `egui-pop-api` or eframe, nor grows past the crate count recorded in `tools/light-deps.txt`.
