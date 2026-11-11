---
egui-pop-core: minor
egui-pop-api: minor
egui-pop: minor
---

Foreground layers have ranks (ADR 0026, `egui_pop::layers`): in each
window, the areas lying over the centre and the flyouts (`Rank::Over`)
stay under the overlays (`Rank::Overlay`), which stay under the menus,
popups and dialogs, whatever was clicked last. A panel lying over the
centre no longer covers an application's card once clicked. egui-pop's
toasts, tips card and halo, hover cards, popovers, zen way out and the
file browser's quick look take `Rank::Overlay`; an application shows
its own overlay with `layers::area` (or ranks a layer with
`layers::rank`) instead of calling `ctx.move_to_top`. egui_stack 0.25.8
is required.
