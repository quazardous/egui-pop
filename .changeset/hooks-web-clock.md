---
egui-pop-hooks: patch
---

The kernel times its listeners with `web_time::Instant`: an emit on the web no longer panics in `std::time::Instant::now` (the gallery for the web stopped at start since `settings.changed` went on the bus). The web CI job now denies `std::time::Instant::now`, `SystemTime::now` and their `elapsed` in code built for `wasm32-unknown-unknown` (clippy.toml).
