---
egui-pop-api: minor
egui-pop-views: minor
---

The viewer's web view (its YouTube page) is on the bus (ADR 0029, slice 5, second part): a native window of the registry (`web-view:<n>`, owned by `egui_pop.viewer`, a child of the window its page is drawn in), opened by its `window.open` before the backend is asked, and closed by its `window.close` wherever it is dropped: the page left or hidden, a panel too small, a failure, its own window closed by the user, its page drawn in another window. The viewer's local listener `viewer::WEB_VIEW_LISTENER` drops the view in that call: a window closing closes its web view first (`parent-closed`). New: `AppHandle::registry_window_of`, the registry window a viewport shows.
