# 0006: The theme moves to `egui-pop-core`, `egui-pop` becomes the facade

Date: 2026-10-11. Status: accepted.

## Context

Phase 1 put the theme (tokens into egui's style, `Tokens::of`) in the
`egui-pop` crate, the one users depend on. With the layers of ADR 0001,
`egui-pop-core` needs the typed tokens (icons sized from tokens, the
fault boundary's error card, overlays), and the facade re-exports
everything, so the theme cannot stay in the facade without a back-edge.

## Decision

`theme` moves to `egui-pop-core` unchanged. `egui-pop` keeps its name and
becomes the facade: it re-exports `egui`, `egui_pop_tokens as tokens`,
`egui_pop_core::theme` as `theme` (so `egui_pop::theme::install` keeps
working), and the model, core and api crates as `model`, `core` and
`api`, plus a `prelude`.

## Consequences

- User code written against phase 1 compiles unchanged.
- `egui-pop` holds almost no code; its semver follows whatever it
  re-exports.
- The layering test lives in the facade crate, the one that sees every
  layer.
