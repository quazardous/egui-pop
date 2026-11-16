---
egui-pop-api: minor
egui-pop-views: minor
---

The viewer's web view is on the bus (ADR 0029, slice 5, second part): a native window (`web-view:<n>`, owned by `egui_pop.viewer`, a child of the window its page is drawn in), opened by its `window.open` before the backend is asked and closed by its `window.close` wherever it is dropped (page left or hidden, too small, failed, its window closed). The listener `viewer::WEB_VIEW_LISTENER` drops it in that call, so a window closing closes its web view first. New: `AppHandle::registry_window_of`.
