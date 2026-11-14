---
egui-pop-api: minor
egui-pop: minor
---

A wizard can run as a focus screen: `Admin::open_focus_wizard` (or `admin.wizard.open` with `"focus": true`) draws it over the main window's whole page whatever the Settings app's window setting (`admin.window`), the shell's menus, status bar and centre hidden until it ends; the title bar stays, to move or quit. For a setup run before anything else, such as a first launch. `Admin::focus_screen` and `admin::focus_screen` say whether one runs; the wizard's state answers `focus`.
