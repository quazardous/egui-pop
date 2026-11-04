---
egui-pop-api: minor
---

An application can say its id: `AppBuilder::app_id("org.example.App")`, read
back with `AppHandle::app_id`. `run_native` and `run_native_with` take the
window's (`ViewportBuilder::with_app_id`) when none is given. The viewer's
YouTube player names the application with it.
