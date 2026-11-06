---
egui-pop-model: minor
egui-pop-core: minor
egui-pop-api: minor
egui-pop-platform: minor
egui-pop-test: minor
egui-pop: minor
---

Shorter ways to write an application, found while building a real one:

- `Ctx::value(path, descriptor)` declares a value of the application's
  state, owned by the plugin, in one line, as `Ctx::setting` does for a
  setting.
- `SettingId::path()` gives a setting's store path (`settings/<id>`);
  `AppHandle::setting::<T>(&id)` and `AppHandle::value::<T>(path)` read
  a setting or a value.
- `AppHandle::group(origin, label, |g| ...)` and, in a command,
  `Call::group(label, |g| ...)` make the writes of one action one undo
  step: the group is ended when the closure returns `Ok`, cancelled
  (every write put back) on `Err` or a panic. It goes through the host
  (`Host::begin_group`), so a host keeping its own history sees it;
  inside a command, while the host dispatches, it goes through the new
  `Host::groups` (`host::Groups`), which a host keeping its own history
  gives (the default `begin_group` and `end_group` use it too). A
  refusal turns into a `String` with `?` (`From<Refused> for String`).
- `param::bound(ui, store, path)` draws the editor of a store's path
  with the descriptor it was declared with.
- `Origin` implements `Display` (`user`, `agent:<session>`...), the
  names `host::source_of` gives.
- `AppHandle::palette_search(query)` ranks the palette's entries as the
  palette does.
- `egui_pop::api::dirs::config_dir(app)` (also in
  `egui_pop_platform::dirs`) is the user's configuration folder, and
  `AppBuilder::persist_in_config(app)` keeps the settings, the layout
  and the window's place there.
- `egui_pop_test::app_snapshot` builds a snapshot harness, or skips with
  its reason where pictures are not compared (no adapter, or a GPU
  rather than lavapipe; `EGUI_POP_SNAPSHOT_ANY_GPU=1` compares on any
  GPU); `try_shared_setup` and `snapshot_skip_reason` say why.
