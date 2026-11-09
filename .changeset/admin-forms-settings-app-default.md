---
egui-pop-api: minor
egui-pop-model: minor
egui-pop-i18n: patch
---

`Ctrl+,`, File > Settings and `app.settings` open the Settings app,
now in `DefaultPlugins`. The settings window's own pages are its pages,
as registered widget kinds: Input › Keyboard shortcuts (`shortcuts`),
Input › Pointer gestures (`gestures`), Interface › Fonts (`fonts`) and a
page per custom settings page (`settings-page`; `app.settings.page`
opens it there). A widget names the settings it edits in its options'
`settings`, so they are not generated again. The window opens from
General › Advanced (`app.settings.window`). A plugin added on its own
takes the place of a group's with the same id. A refresh also forgets the reads still in flight, so a page sees a change made right after it asked.
