---
egui-pop-api: minor
egui-pop-views: minor
---

An application-wide selection: `selection::SelectionPlugin` provides a
`SelectionService` (the `SELECTION` token) holding the selected items and
the hovered one, as string ids (`SelectionItem`), shared by every panel.
A click replaces, Ctrl toggles, Shift selects a range in the widget's order;
subscribers and the journal (`selection.changed`) hear each change, and the
context keys `hasSelection` and `selectionCount` follow. `selection.back` and
`selection.forward` walk the past selections (Alt+Left and Alt+Right in a
widget bound to the selection). Agents read the `selection` collector and
run `selection.set`, `selection.add`, `selection.clear` and `selection.hover`.

`TreeView::shared` and `ListView::shared` bind the tree and the list to it:
their rows show what is selected and hovered anywhere, and what they select
goes there. `TreeSource::item` and `ListRows::item` name a row's item.
