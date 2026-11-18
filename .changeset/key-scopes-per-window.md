---
egui-pop-model: major
egui-pop-api: major
egui-pop-core: major
egui-pop: major
egui-pop-agent: major
egui-pop-editors: major
egui-pop-i18n: major
egui-pop-imaging: major
egui-pop-platform: major
egui-pop-test: major
egui-pop-views: major
egui-pop-viewport: major
---

Keys stay in the window they are pressed in. Every key binding has a scope, and each window dispatches its own keys bubbling from the focus outwards: a focused surface (`surface:<name>`), then the window's focused panel (`KeyScope::Panel`, `panel:<id>` in keymap files, `BindOptions::panel`), then the window (`KeyScope::Window`, `window:<id>`, `BindOptions::window`; `main` is the main window and its detached panels), then every window (`global`). `focusedPanel` is now each window's own: the workbench's in the main window and its detached panels, an application window's last focused panel in it (empty while none was), so a panel's keys no longer fire from another application window, where they used to act on the main window's panel. A binding without a scope (or `global`) whose `when` requires `focusedPanel == '<id>'` is that panel's (`KeyScope::infer`), so keymaps written before scopes load unchanged. Among the bindings that may fire, the narrower scope wins after the depth of the focus (traces say "narrower scope"); a modal blocks window and panel bindings as it blocks global ones. Breaking: `KeyScope` has the new variants `Window` and `Panel`, `Verdict` the new variants `OtherWindow` and `PanelNotFocused`.
