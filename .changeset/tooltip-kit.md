---
egui-pop-tokens: minor
egui-pop-core: minor
egui-pop: minor
---

Tooltips have a kit component, `tooltip`: its fill, border, radius,
padding, shadow, text colour and `font-size`, its `placement` (`widget`,
under the widget from its start edge as egui's, or `pointer`, under the
pointer from its position) and its `offset`. `egui_pop_core::tooltip`'s
`TooltipExt::tip`, `disabled_tip` and `tip_ui` draw them, with egui's
timing and accessibility, where `on_hover_text` keeps egui's popup look.
The defaults are the popup's: nothing changes until a theme sets them.
