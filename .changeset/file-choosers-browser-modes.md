---
egui-pop-api: minor
egui-pop-model: minor
egui-pop-i18n: patch
egui-pop: minor
---

The file choosers use the file browser's modes: the in-app picker's list and
grid are its `details` and `thumbnails` modes, picked with the browser's
switcher (a segmented control up to four modes, a menu beyond, translated
labels, icons from the registry) or `Ctrl+1` to `Ctrl+9` (`file.view.mode<n>`
commands). Each dialog reopens in the mode it last used, kept in the host's
store (`files::view_for`); `settings/files.view` reads `list` and `grid` as
`details` and `thumbnails`. A gallery page shows a `FileBrowser`.
