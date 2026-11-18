---
egui-pop-api: minor
egui-pop-core: minor
egui-pop-tokens: minor
egui-pop: minor
---

`menus::submenu(ui, label, |ui| ..)` adds a submenu item to a menu drawn by hand (a context menu built in a closure), in the declared menus' look: it opens once the pointer rests on it for `motion.submenu.delay` (a new token, 200 ms), on a click, or on Right, Enter or Space; Left or Escape closes that level alone. Assistive technology reads a menu item with a menu, expanded while open. `MotionTokens::submenu_delay` reads the token.
