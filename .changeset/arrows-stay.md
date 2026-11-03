---
egui-pop-core: patch
---

With the focus plugin, a plain arrow no longer takes the focus out of the
F6 region holding it (a side area, the centre, a palette, the status
bar): egui's move to the nearest widget that way is undone when it
crosses into another region, and the focus stays. Tab and F6 cross
regions; a widget outside every region keeps egui's rule.
