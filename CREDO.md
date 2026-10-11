# Credo

What egui-pop holds to, in order of precedence when two collide.

1. **Declare once, derive the rest.** A command yields its menu entry,
   its palette entry, its key binding and its agent surface. Nothing is
   declared twice.
2. **No silent no-op.** What is ignored is impossible by types or said
   out loud, with what happens instead (ADR 0003).
3. **Owned registrations.** Whatever a plugin registers goes away with
   it (ADR 0004).
4. **Layers, no back-edges.** Logic below drawing; drawing below the
   application API (ADR 0001).
5. **Easy to debug.** Every registry can describe itself, snapshot itself
   as JSON and be driven through a debug bridge; key resolution keeps a
   trace.
6. **Plain first, bound when needed.** Widgets work on `&mut T` like
   egui's; binding to a store is opt-in (ADR 0009).
7. **Accessible by default.** Every interactive widget has a role, a
   name and a value; contrast is tested per theme; focus is restored when
   an overlay closes.
8. **Tested headless.** Model tests without a GPU; widget tests with
   egui_kittest on a software renderer; snapshots per theme.
9. **Tokens, not literals.** Colours, sizes and durations come from the
   theme.
10. **Survey before building.** A maintained, permissively licensed crate
    is preferred when it fits; when we write our own, the ADR or the
    commit says why.
