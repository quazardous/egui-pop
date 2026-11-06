---
egui-pop-core: minor
egui-pop-api: minor
egui-pop-views: minor
egui-pop-editors: minor
egui-pop: minor
---

Every scroll area of egui-pop's own goes through `UiPop::scroll_area`, a sideways strip takes the plain wheel, and reduced motion stills egui's kinetic scrolling.

- **Keys and colours everywhere.** The panels, dialogs and lists (settings, file picker, palette, jobs, history, theme editor, tips, about, developer tools, completion), the views (list, tree, library grid and filmstrip, thumbnail grid, editor, viewer) and the gallery's pages answer Page Up, Page Down, Home, End and the arrows, and draw their bars in the kit's `scroll-bar` colours. With nothing focused, the pointer over a list or a grid pages it.
- **Forms for views.** `UiPop::scroll_area_viewport` (egui's `show_viewport`) and `UiPop::scroll_area_rows` (`show_rows`). A view whose focusable widget lies around its own area calls `focus::takes_keys` inside it, and the area then makes no second Tab stop.
- **A strip that scrolls only sideways takes the plain wheel**: the themes turn on egui's `always_scroll_the_only_direction`, so a strip of tabs or the filmstrip no longer needs Shift. At its end the wheel goes on to the area around it.
- **Reduced motion stills kinetic scrolling.** Under `ThemeSpec::reduced_motion`, a touch drag no longer coasts after the release and a trackpad no longer stretches an area past its end (egui 0.37's `KineticScrollStyle`); egui's own comes back with motion. A mouse wheel had neither.
