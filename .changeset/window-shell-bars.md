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

Application windows may show the main window's bars, as VS Code's windows do.

- **Shell bars.** `WindowSpec::shell_bars(true)` gives a window the start of the main window's title bar (the `CAPTION_START` zone: the menu button, the name or logo) and the main menu (`app.main`, when the application has a menu bar), evaluated and run in that window's context. The bars follow the window into the main window when it moves there.
- **Greyed with a reason.** A menu command whose `when` holds only in another open window (`appWindow == 'main'`) is greyed, its tooltip saying "Only in the { window } window" (new string `egui-pop-menu-in-window`, English and French).
- **Popups per window.** A menu opened with `menus::popup_at` from an application window is drawn in that window; it was drawn nowhere.
- **Tests.** `MultiWindow::node` gives where a window's last pass drew a named node, and whether it is disabled.
- **Breaking:** `WindowSpec` has a new public field, `shell_bars`: a struct literal gives it, or starts from `WindowSpec::new`.
