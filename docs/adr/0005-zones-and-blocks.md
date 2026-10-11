# 0005: Zones and blocks

Date: 2026-10-11. Status: accepted.

## Context

Plugins need to put things on screen in places other plugins own: a
status bar item, a section in a settings page, a decoration around
another plugin's view. Hard-coding those places in the shell does not
scale; letting plugins draw anywhere makes the result depend on call
order.

## Decision

A **zone** is a named place (`ZoneId`, typed constants for the shell's:
`zones::TOP_BAR`, `zones::SIDE_BAR`, `zones::MAIN`, `zones::INSPECTOR`,
`zones::STATUS_BAR`); a plugin may declare its own zones and show them
anywhere it draws (`zones::show(ui, handle, zone)`). A **block** is
something drawn in a zone: an id, a weight (lighter first) and a draw
function. Four operations, resolved when the app is built (not per frame):

- `add`: a new block;
- `replace(id)`: takes the place (and weight) of an existing block;
- `extend(Before(id) | After(id))`: a new block placed next to another;
- `wrap(id)`: a wrapper drawn around an existing block, receiving the
  inner block as a closure it calls (or not) where it likes.

Resolution order: adds (by weight, then registration order), then
replaces, then extends, then wraps (by plugin order, the outermost wrap
last). A zone nobody declared, a replace, extend or wrap whose target is
missing, or two blocks with the same id are reported (ADR 0003) with
what happens instead: the block is not shown, added at the end, or the
wrap is skipped.

## Consequences

- Removing a plugin restores the zones as they were (its replaces and
  wraps go with it).
- The zones collector of the debug bridge shows each zone's resolved
  blocks with their owners and the operations applied.
- Blocks get an `AppHandle` to reach services, the value store and the
  command runner; they keep their own state in their closure.
