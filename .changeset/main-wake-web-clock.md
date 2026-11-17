---
egui-pop-api: patch
---

The main window wake-up reads the web-safe clock, so the web build no longer panics on `std::time::Instant`.
