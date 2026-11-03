---
egui-pop-api: major
egui-pop-i18n: major
egui-pop-platform: major
egui-pop-views: major
egui-pop-editors: major
egui-pop-imaging: major
egui-pop-test: major
egui-pop: major
---

Richer places in the in-app file picker's side bar (egui-pop-api;
`docs/files.md`): the system's folders (Desktop, Documents, Downloads,
Pictures, Music, Videos, from the XDG user directories on Linux), and the
volumes mounted under Devices (mount points under `/run/media`, `/media` and
`/mnt`, `/Volumes` on macOS, the drives on Windows), read when a picker opens
(`files::places`, `Files::set_system_places` for an application's own). A
bookmark's menu renames it, moves it up or down, or removes it
(`files::rename_bookmark`, `files::move_bookmark`, kept in
`settings/files.bookmark_names`). The side bar scrolls. French included. To
move: `Places` gains fields and is `#[non_exhaustive]` (start from
`Places::default()`), and `PickerAnswer` gains `RenameBookmark` and
`MoveBookmark` and is `#[non_exhaustive]`: a `match` on it needs a wildcard arm.
