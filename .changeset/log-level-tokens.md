---
egui-pop-tokens: minor
egui-pop-core: minor
egui-pop: minor
---

Log levels have colours: `palette.log.trace`, `palette.log.debug`,
`palette.log.info`, `palette.log.warn` and `palette.log.error` are system
tokens (`egui_pop_tokens::LOG_LEVELS`), read in code through
`Tokens::of(ctx).palette.log` (`LogPalette`). `pop-dark` aliases them to the
muted text and the status colours (trace muted, debug `info`, info
`success`, warn `warning`, error `danger`), so every built-in theme gives
them; `gruvbox-light` darkens info's green for its canvas. Each is checked
as text on `surface.canvas` and `surface.panel` (`contrast::PAIRS`).
