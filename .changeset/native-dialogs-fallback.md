---
egui-pop-api: minor
egui-pop-test: minor
egui-pop: minor
---

The system's file chooser (feature `native-dialogs`) is a fallback, off at
build time and at run time. Even built in, it is used only when the
application allows it with `AppBuilder::system_file_chooser(true)` (read
back with `AppHandle::system_file_chooser`); the `files.native` setting is
declared only then. `FilesPlugin::default()` now gives the in-app picker,
as `FilesPlugin::in_app()` does.

It is never used under tests: `files::system_chooser_refused` (and
`files::system_chooser_refusal`, which says why) refuses it under
`cfg(test)`, once `files::refuse_system_chooser` was called, with
`EGUI_POP_TEST` (`files::TEST_ENV`) or `NEXTEST` in the environment, or when
the frames come with no window's focus (egui_kittest, a bare
`Context::run`). Every harness of `egui-pop-test` calls
`refuse_system_chooser`, so an application's own tests are covered whatever
its features. The files service's debug snapshot gives `native_allowed` and
`native_refused`.
