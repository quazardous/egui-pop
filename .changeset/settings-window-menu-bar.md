---
egui-pop-api: patch
egui-pop: patch
---

The Settings app opened in a window of its own draws the main window's bars, as a window opened with `WindowSpec::shell_bars` does: the title bar's start and the main menu, evaluated and run in that window, the main window's commands greyed.
