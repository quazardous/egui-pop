---
egui-pop-api: minor
egui-pop-i18n: minor
egui-pop: minor
---

Zen mode shows the panel that matters alone, full size, wherever it is:
`workbench.zen` takes a `panel`, which the panel menu's new "Zen this
panel" passes; without one it picks the focused view panel, then the
application's choice (`pop.zen.panel` on the bus), then the centre's
active tab. A detached window's panel is zenned in its window. Zen mode
only changes what is drawn, so leaving it shows the layout as it was.
`AppHandle::set_zen_panel`, `zen_panel` and `zen_target` drive it; in
English and French.
