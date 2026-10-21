# egui: popups should take the style of the `Ui` they are opened from (a comment on #4692)

> **Before posting (delete this block).** Repository: emilk/egui, as a
> comment on <https://github.com/emilk/egui/issues/4692> ("Tooltips should
> maybe inherit the style of the parent?", open), which asks this for
> tooltips; this widens it to every popup and proposes an API. egui's
> CONTRIBUTING ("Agents & LLM policy"): the human posting is responsible
> for it; no AI disclosure is required. Related: `ComboBox::popup_style`
> (<https://github.com/emilk/egui/pull/7360>). Checked against egui 0.36.2
> and 0.37.0.

The same holds for every popup, not only tooltips: a combo box's list, a
context menu, a menu, `Popup` itself. They are all `Area`s, and an `Area`'s
`Ui` is built from the context's style, since `Ui::new` falls back to
`ctx.global_style()` and `Area` has no style of its own
(`crates/egui/src/ui.rs`, `let style = style.unwrap_or_else(|| ctx.global_style());`).
So a style set on one `Ui` stops at its popups:

```rust
ui.scope(|ui| {
    ui.style_mut().visuals = egui::Visuals::light(); // in a dark app
    egui::ComboBox::from_label("Light?")
        .selected_text("a")
        .show_ui(ui, |ui| {
            ui.label("this list is dark");
        });
    ui.button("hover me").on_hover_text("this tooltip is dark");
    ui.button("right-click me").context_menu(|ui| {
        ui.label("this menu is dark");
    });
});
```

Our use case is a theme editor: its preview pane draws the theme being
edited in one `Ui` of the window, while the rest of the application keeps
its theme, so a broken draft cannot make the editor unusable. Everything in
the pane follows the draft except its popups. Other cases: a themed panel
(a dark viewer in a light app), a theme picker's thumbnails.

The hooks that exist (`Popup::style`, `ComboBox::popup_style`,
`MenuConfig::style`) are `StyleModifier`s applied over the context's style,
at the call site. They do not help when the popup is opened by
`Response::on_hover_text` or `Response::context_menu`, or by a widget from
another crate. Today we work around it by swapping the context's styles
(`Options::dark_style` and `light_style`) while the pane draws and putting
them back afterwards. That works because egui draws popups while their
widget is drawn, but it is not something egui promises.

**Proposal.** A popup takes as its base style the style of the `Ui` it is
opened from, and `Popup::style` and its relatives still modify it:

- `Area::style(impl Into<Arc<Style>>)`, forwarded to `UiBuilder::style`,
  for an `Area`'s base style (the context's when unset, as today).
- `Popup::base_style(impl Into<Arc<Style>>)`, passed to its `Area`;
  `Popup::from_response` (and so `menu`, `context_menu`, tooltips) sets it
  from the response.
- `Response` remembers the style of the `Ui` that made it
  (`Response::style() -> &Arc<Style>`, one `Arc` clone per widget), so
  `on_hover_ui`, `context_menu` and `Popup::from_response` inherit it.
  `ComboBox::show_ui` and `Ui::menu_button` have the `Ui` and can pass
  `ui.style()` directly.

This changes behaviour: a `Ui` with a style of its own sees its tooltips
and menus follow it, which is what #4692 suggests. An app that relies on
popups always using the global style can set
`Popup::base_style(ctx.global_style())`.

A smaller step, if keeping a style per `Response` costs too much:
`Area::style` and `Popup::base_style` alone, with `ComboBox` and
`menu_button` passing `ui.style()`. Tooltips and context menus would then
still need the `Response` change.

We can open a PR for either if one of them is welcome.
