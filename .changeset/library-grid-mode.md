---
egui-pop-views: minor
egui-pop-api: minor
egui-pop: minor
---

`egui_pop_views::library::LibraryGridMode` is the library grid as a file browser's view mode (`library`, "Library grid"), registered on a `ViewModes`. It lays the entries out as the thumbnails mode does (zoom as a count per row, rubber band, the same keys), and draws the library's overlays over each file's cell: flag, colour labels, rating stars, a rejected image dimmed. A screen reader hears them as the cell's description. The marks are the application's (`Fn(&SourceEntry) -> Marks`). The culling keys (0 to 5, P, X, U, 6 to 9, with auto-advance) give `Signal<PathBuf>` into a `LibrarySignals` queue the application drains. The file browser no longer types ahead a key its mode took. New strings: `egui-pop-files-mode-library`, `egui-pop-views-library-rated` (en, fr).
