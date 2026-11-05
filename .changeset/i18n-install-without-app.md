---
egui-pop-i18n: minor
---

`egui_pop_i18n::install(&ctx, &i18n)` installs an `I18n` in an egui
context without `App` or `I18nPlugin`: `t(ctx, ..)`, `text(ctx, ..)` and
`of(ctx)` read it (with their Fluent arguments), `tr!` reads it as the
global catalogues, and egui-pop's own text is translated through its
strings provider. `I18nPlugin` installs its catalogues the same way.
