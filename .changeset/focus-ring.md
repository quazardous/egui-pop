---
egui-pop-core: minor
egui-pop-api: patch
egui-pop-viewport: patch
egui-pop-views: patch
egui-pop-editors: patch
---

A focus you can see: with the focus plugin (an application has it), the
widget the keyboard focused gets a ring of the `focus.ring` token, drawn
inside the part of it that shows, egui's own widgets included. Only the
keyboard shows it (Tab, an arrow, F6, a command, a screen reader): a click
focuses without a ring, as `:focus-visible` has it. A widget showing its
focus itself calls the new `focus::own_ring` each pass it has the focus;
one painting a kit state from `State::from_response` has it said for it,
and egui's text fields are left alone. `focus::ring_shown` says whether
the keyboard moved the focus where it is.
