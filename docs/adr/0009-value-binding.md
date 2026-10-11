# 0009: Three ways to bind a value

Date: 2026-10-11. Status: accepted.

## Context

egui widgets edit a `&mut T`, which is simple and stays the default. An
application with a value store (settings, parameters an agent may also
change, undo) needs widgets that read and write the store with an origin,
group a drag into one undo step and notify listeners. Some views only
show a value that something else changes.

## Decision

`egui_pop_core::binding::Binding<'a, T>` has three modes:

- `Binding::plain(&mut value)`: exactly egui's model, nothing else;
- `Binding::bound(&store, path)`: reads the store, writes through
  `Store::set` with `Origin::User`, opens a gesture when a drag starts
  and ends it when the drag stops (one undo step, the path held against
  other writers meanwhile); listeners registered on the store are told of
  each change;
- `Binding::watch(&store, path)`: reads the store, never writes; the
  widget is drawn disabled.

`binding::edit(ui, binding, |ui, &mut T| ui.add(...))` adapts any egui
widget to the three modes. There is no implicit data context: the store
is always passed explicitly.

## Consequences

- Components take a `Binding` and work the same in all three modes.
- A refused write (a gesture of another origin holds the path, a value
  out of range) is reported on the response's hover text and in the
  diagnostics, never dropped.
