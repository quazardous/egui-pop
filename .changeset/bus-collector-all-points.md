---
egui-pop-api: patch
---

The debug bridge's `bus` collector lists every point on the bus, not only egui-pop's: an application's points on the bus it supplies show too. Each point carries its kind, who declared it (`by`: `egui_pop` from `bus::CONTRACT`, or `application`), whether it is declared, its description and its listeners in the order they run; the snapshot adds `missing_targets`, and its `order` text covers every point.
