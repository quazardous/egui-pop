---
egui-pop-api: minor
egui-pop: minor
---

The in-app picker draws its list and grid through the file browser's
`details` and `thumbnails` modes: one rendering path. A mode lays the
entries out (virtualised, the focus kept in view, the keys' moves, the
thumbnails asked in reading order) and an `EntryDrawer` draws each entry:
the browser's `PlainDrawer`, or the picker's own, with its sortable column
headers, its groups' badges and the highlight its field drives.
`ModeContext` gains `drawer`, `max_height`, `name` and `many`;
`ModeOutput` gains `focused` and `columns`.
