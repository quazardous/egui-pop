---
egui-pop-api: major
egui-pop: major
egui-pop-editors: major
egui-pop-i18n: major
egui-pop-imaging: major
egui-pop-platform: major
egui-pop-test: major
egui-pop-views: major
---

Dialogs have a Copy button, and may carry technical details for a bug report.

- **Copy.** A notice (`Info`) and a confirmation (`Confirm`) have a small copy icon in their bottom left corner, named and tooltipped "Copy", reached by Tab after their buttons. It puts the title and the text on the clipboard as plain text and says "Copied." a moment; Enter on it copies without answering the dialog. `.copyable(false)` leaves it out.
- **Errors.** `Info::error(title, text)` is a notice that is an error: its details are shown by default.
- **Details.** `.details(lines, mode)` gives a dialog key-value lines (`Details`), copied after its text under a `---` separator, `key: value` each. `DetailsMode::Hidden` keeps them off screen, `DetailsMode::Shown` shows them in a collapsed "Details" section, a read-only, monospace, selectable area holding exactly the copied lines; `DetailsMode::Auto` shows them on an error and hides them otherwise. `Info::copy_text` and `Confirm::copy_text` say what the button copies.
- **Breaking:** `Info` and `Confirm` have new public fields (`copy`, `details`, and `error` on `Info`): a struct literal of either gives them, or starts from `Info::new` and `Confirm::new`.
