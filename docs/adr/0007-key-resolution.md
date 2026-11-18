# 0007: Key resolution: focus depth, layers, removal

Date: 2026-10-11. Status: accepted.

## Context

Several bindings may claim one key: a dialog's Escape and a text field's
Escape inside it, a framework default and the user's rebinding, a
component's suggestion. Users must be able to rebind and to unbind
anything, and when a key does something unexpected, someone must be able
to see why.

## Decision

The context a key is resolved in is rebuilt each frame from the **focus
chain**: the scopes (`egui_pop_core::focus::scope`) containing the
focused widget, outermost first; each sets context keys at its depth
(the root is depth 0). A binding's depth is the deepest depth among the
keys its `when` clause tests. Among the bindings of the pressed keys
whose `when` holds, the winner is the greatest
`(depth, layer, specificity, declaration order)`: the binding closest to
the focus wins, then `User > Workspace > Default > Suggestion`, then the
clause testing more keys, then the last declared.

Two removals, both stored in the user's keymap document:

- `"command": "-id"` removes that command's bindings from lower layers;
- `"command": null` unbinds the keys: bindings of exactly those keys in
  lower layers are gone, whatever their command.

A shortcut is one or more chords (`Ctrl+K Ctrl+S`); a prefix of a live
binding makes the dispatcher wait for the next chord. Keys are logical
(keypad digits are digits).

Every resolution can produce a **trace**: each candidate binding with its
verdict (won, lost to, clause false, removed by, unbound by, prefix),
which the debug bridge keeps for the last keys pressed.

A binding whose command went with a disconnected plugin is dormant
([ADR 0028](0028-removed-contributor-work-in-flight.md)): kept in its
layer, it is no candidate for its keys, which go to the next binding;
the trace gives it the verdict "lost" with `because` `dormant: its
command's plugin is disconnected` (`Keymap::set_dormant`,
`Keymap::dormant`).

## Consequences

- A user binding meant to beat a focused component's binding uses the
  same `when` (the shortcuts editor copies it), or unbinds the key.
- Resolution is pure (`egui-pop-model::keymap`); the egui dispatcher only
  turns key events into chords and consumes the keys that resolve.

## Addendum (2026-10-12): scopes and Escape

A binding listens globally or on a surface (`surface:<name>`, a focus
scope that must be in the focus chain); a surface binding's depth is its
scope's. Each binding says whether a text input keeps its key (`auto`
leaves plain keys to it), and an open modal blocks global bindings
unless they go through it. Escape is taken in a fixed order: a focused
text input lets go first (and swallows that press), then egui's drag,
the top overlay, the key bindings, the widgets (`egui_pop_core::focus`).

## Addendum (2026-11-18): scopes per window

Every binding has a scope, and each window dispatches its own keys,
bubbling from the focus outwards:

| Scope | In files | Fires |
|---|---|---|
| Surface | `surface:<name>` | while that focus scope is in the window's focus chain |
| Panel | `panel:<id>` | while that panel is the focused panel of the window the key is pressed in |
| Window | `window:<id>` | only in that application window (`main`: the main window and its detached panels) |
| Global | `global` (or none) | in every window |
| System | `system` | as global, and from the system while the application is not focused |

`focusedPanel` belongs to each window: the main window and its detached
panels share the workbench's, an application window keeps the panel
focused last in it (empty while none was), so a panel's keys never fire
from another window. Among the bindings of the pressed keys that may
fire, the winner is the greatest `(depth, scope, layer, specificity,
order)`: nearer the focus first, then the narrower scope (surface, panel,
window, global), then as before. A modal open in a window blocks every
binding there but a surface's, unless it goes through modals.

A binding declared without a scope (or `global`) whose clause requires a
panel (`focusedPanel == '<id>'`, alone or in a conjunction) is that
panel's: keymaps written before scopes load unchanged, and are saved
without the inferred scope. `when` keeps the other conditions. The
focused widget before the keymap is a later step.
