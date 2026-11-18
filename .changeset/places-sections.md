---
egui-pop-api: minor
egui-pop: minor
---

Places have sections of the application's: a `PlaceProvider` registered on `Files::place_providers` (or `FilesPlugin::places`) gives `PlaceSection`s of `PlaceItem`s (a name, an icon, a detail, a badge and, for a folder, its path), such as a photo import's cards and services. The in-app picker lists their folders under its bookmarks, each title a heading. `PlacesPanel` draws sections anywhere, one Tab stop with Up and Down across them, Enter or a click choosing; `Files::places` and `Places::sections` give every place as sections. `Places` has a new field, `app_sections`.
