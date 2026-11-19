---
egui-pop-core: minor
egui-pop: minor
---

`UiPop::strong` is now `UiPop::strong_text`: egui's `Ui` has an inherent `strong`, which always wins over a trait method, so `ui.strong(..)` drew egui's bold-coloured label, never egui-pop's emphasis role. `UiPop::strong` stays, deprecated, forwarding to `strong_text`. A test keeps every `UiPop` method name off the list of egui's inherent `Ui` methods. The app and extension templates have a `clippy.toml` warning on `egui::Ui::strong` (`disallowed-methods`), and [Your first application](docs/first-app.md#lints) recommends it.
