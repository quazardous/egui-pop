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

The application's options inside the in-app file picker (egui-pop-api;
`docs/files.md`): `FileRequest::choice` adds a combo box and
`FileRequest::check` a check box, in the desktop portal's `choices` shape,
shown above the picker's buttons; `files::chosen_choices` reads each id with
its value when the choice ends, and `file.chosen` carries them. A run-hook
answers `choices` with its `paths` or calls `Files::set_choices`, and the
debug bridge's `answer` takes `choices`, each checked against the request. A
`FilePicker` drawn on its own gives them with `choices` and `set_choice`.
`rfd` cannot show them, so a request with choices opens the in-app picker
even where the system's chooser is used. To move: `FileRequest` is now
`#[non_exhaustive]` (a field added later breaks nobody); build it with
`FileRequest::open` and the others and their builders, not as a struct
literal.
