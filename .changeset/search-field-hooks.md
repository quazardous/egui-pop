---
egui-pop-views: minor
egui-pop: minor
---

`SearchField` takes two application hooks, meant with `whole_text(true)`:
`suggestions` shows the application's suggestions (label, kind, the text
once taken) in a list under the field (Up and Down move, Enter takes,
Escape closes the list, then clears the text, then leaves the field), and
`chips` draws the application's own chips in the field, each removable
(its button, Backspace twice or Ctrl+Backspace in the empty field). What
they did is read with `SearchState::take_events`. The field's own parsing
and chips stay the default.
