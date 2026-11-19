---
egui-pop-api: minor
egui-pop-platform: minor
egui-pop: minor
---

With a backend that reads the system clipboard (`SystemClipboardPlugin`), `run_native` turns eframe's `clipboard_shortcuts` off and the keymap runs the copy, cut and paste chords everywhere: in a focused text field, `edit.copy`, `edit.cut` and `edit.paste` give it egui's own copy, cut and paste events, the pasted text read through the backend. A remapped chord now edits text fields too, and the paste key is seen on its press whatever the clipboard holds. New: `clipboard::set_keymap_shortcuts`, `clipboard::keymap_shortcuts`, `clipboard::SYSTEM_PLUGIN` and the `clipboardKeymap` context key. Nothing changes on the web, with the default backend or on eframe's own runner.
