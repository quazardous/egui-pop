---
egui-pop-tokens: minor
egui-pop-core: minor
egui-pop: minor
---

A sixth log level, past error: `palette.log.critical`, the critical level
some loggers add (Python's `logging`, Rerun's text log), last in
`egui_pop_tokens::LOG_LEVELS` and `LogPalette` (`critical`, and in
`levels()`). `pop-dark` aliases it to `{red.12}`, danger's red at its
scale's high-contrast text step, so every built-in theme gives it; it is
checked as text on `surface.canvas` and `surface.panel`. The Rerun-like
kit gives it Rerun's critical red.
