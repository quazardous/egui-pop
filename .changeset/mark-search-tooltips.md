---
egui-pop-views: minor
egui-pop-i18n: patch
---

The library's mark buttons and search field explain themselves: each `ColorLabels` and `FlagToggle` button shows its name as a tooltip ("Red label", "Pick"), with the key a `CullKeymap` binds to it when given through `keys` ("Red label (6)", in English and French), or the application's own text through `tooltip`; the search field's `+` ("Add a filter"), clear ("Clear the search") and each chip's remove button show their name as a tooltip too. Colour labels are now drawn as same-shape swatches, as in Lightroom and darktable, on the marks bar and the grid's badges; `LabelStyle::Shape` keeps a shape each for colour-blind users, chosen by the application (`LabelStyle::set`) or the theme (`library.label-style`).
