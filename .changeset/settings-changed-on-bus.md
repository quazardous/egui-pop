---
egui-pop-api: major
---

Settings changed are on the bus (ADR 0029): every change of a setting's value is said once by the value store, its one writer (ADR 0016), as the new event point `settings.changed` (`bus::SettingsChanged`, a `bus::SettingChange`: `path`, `scope`, `by`, `seq`), whether it was made here or by another client of the host's store (`by` is `store` then, and the scope the one it changed in). The application says them at the start of its logic, after its requests, after the Settings app's sync applied what another client changed, and at the end of the main window's pass (`AppHandle::deliver_settings`), so a change heard from another client is followed in the pass it is heard. The theme, the pointer profile and the main window's frame are its local listeners (`bus::THEME_INSTALL`, `bus::POINTER_PROFILE`, `bus::FRAME_DECORATIONS`) instead of comparing their settings every pass; an application's part that follows a setting listens there too. `App::keymap` now returns a `Ref<Keymap>` (the keymap is shared with the `pointer.profile` listener).

**Breaking:** `App::keymap` returned `&Keymap`, now `Ref<Keymap>`: code calling it keeps the guard while it reads (`let keymap = app.keymap();`) and drops it before the keymap can change; a method call on it (`app.keymap().to_file()`) compiles unchanged.
