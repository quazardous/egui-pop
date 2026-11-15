---
egui-pop-core: minor
egui-pop-api: patch
egui-pop-viewport: patch
---

Five places now do what their documentation says. A fault boundary's `Fault::count` counts the failures in a row: a retry that fails again adds one, a pass that draws starts again from zero (it was always 1). A plugin's icons go with it: `icon_set::unregister` and `icon_set::unregister_owner` remove registered icons (the name stops resolving at once, its textures go at the next theme install, and it keeps its id if registered again), and the application calls `unregister_owner` when a plugin is disconnected or fails to connect. The announcer reads the same text twice in a row twice: every other message of a live region ends with an invisible no-break space, so the region changes. The image viewport's accessible node is an image (`Role::Image`), with its name and its zoom as before. The `wait` module's docs name `egui_pop_test::wait::wait_until`, and `cargo xtask doc-paths --check` now also reads the crates' rustdoc comments.
