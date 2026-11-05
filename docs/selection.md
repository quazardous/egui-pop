# The shared selection

In an application showing the same things in several panels (a scene
tree, a list, a 3D view, a timeline), selecting or hovering a thing in
one should show it in all of them. egui-pop's views keep a selection of
their own (the tree's cursor and selected nodes, a list's rows); the
**shared selection** is one selection for the whole application, which
widgets opt into. Everything below is `egui_pop::api::selection`, and
`shared` on the tree and the list in `egui_pop::views`.

## The model

- **The service**: `SelectionService`, under the `SELECTION` token,
  provided by `SelectionPlugin`. It holds the selected items, in the
  order selected, the one item hovered, the anchor of a range, and the
  past selections. It is thread-safe: a worker may select too.
- **Items** are `SelectionItem`s: a string id the application chooses,
  the same in every widget showing the thing: a path
  (`world/points/a`), `kind:id`, a number. A string rather than a typed
  key, because widgets with different key types (a tree by path, a table
  by row id, a canvas by entity) meet on it, and it goes through the
  journal, the history and the agents' commands as it is.
  `SelectionItem::from_key` makes one from a view's key: a string key as
  it is, any other key its JSON text.
- **Gestures** (`Gesture`, `Gesture::of(modifiers)`):

  | Gesture | Click | Does |
  |---|---|---|
  | `Replace` | plain | selects the item alone |
  | `Toggle` | Ctrl (Cmd on macOS) | adds the item, or removes it |
  | `Range` | Shift | selects from the anchor to the item, in the widget's order, and only that |
  | `AddRange` | Ctrl+Shift | adds that range |

  `SelectionService::click(item, gesture, order, source)` applies one;
  `order` is the widget's items as shown (empty when it has no order: a
  range then selects the item alone, as it does when the anchor is not
  in that order). `set`, `add`, `remove`, `toggle`, `apply` (add and
  remove as one change) and `clear` are there for the rest.
- **Hover**: one item at most. A widget says what it hovers every pass
  it is drawn (`hover(source, Some(item))`, or `None`): its `None`
  clears a hover it said, never another widget's, and a hover no widget
  repeats goes after a pass (a widget no longer drawn). An agent's hover
  (`selection.hover`) stays until changed.
- **Changes**: each change of the items bumps `revision()` (the hover
  has `hover_revision()`), and every change calls the subscribers
  (`subscribe(f)`, `unsubscribe`) with a `Change`: the items, the
  hovered one, the revision, whether the items changed, and who made
  it (the widget's id, `command`, `history`). Once a pass the plugin
  publishes `selection.changed` on the journal and sets the context
  keys.

## Context keys

| Key | Value |
|---|---|
| `hasSelection` | `true` when something is selected, else `false` |
| `selectionCount` | how many items are selected |
| `selectionCanBack` | `selection.back` has a selection to go back to |
| `selectionCanForward` | `selection.forward` has one to go to |

A command acting on the selection says `.when("hasSelection")`.

## History: back and forward

Every change of the items keeps the selection before (100 at most).
`selection.back` selects it again, `selection.forward` comes back;
a new selection after going back drops what was ahead, as a browser
does. Selecting is not editing: the undo history does not see it, and
Ctrl+Z does not undo a selection.

The keys are **Alt+Left** and **Alt+Right**, bound on the selection's
focus surface (`SURFACE`, `selection`): they work while the focus is in
a widget bound to the selection, and only when there is somewhere to go
(`selectionCanBack`, `selectionCanForward`). Elsewhere the keys stay
what they were: the context stack's `nav.back` (Alt+Left, outside a
modal), the file picker's back and forward (in its modal), the viewer's
back and forward (on its surface). A surface binding outranks a global
one while its surface has the focus, so the keymap reports no conflict.

## Binding a widget

The tree and the list bind with `shared`:

```rust,ignore
use egui_pop::api::selection::SELECTION;
use egui_pop::views::tree::TreeView;

let selection = app.service(&SELECTION).expect("SelectionPlugin");
TreeView::new("scene")
    .app(app)
    .shared(&selection)
    .show(ui, &mut state.tree, &scene);
```

- Their rows show selected and hovered when they are so anywhere.
- What they select still runs through their step (`tree.select`,
  `list.select`, which a plugin may veto or redirect), then goes to the
  service: a plain click or keyboard move replaces the whole selection;
  with Ctrl held, only the view's own items change and the others'
  stay.
- A view keeps its own selection as a mirror of the shared one, limited
  to its items, so its cursor, anchor (Shift ranges in its own order)
  and keyboard work as before. `TreeState::selection` and
  `ListState::selection` read the mirror.
- A row's item is `TreeSource::item(key)` or `ListRows::item(row)`, by
  default `SelectionItem::from_key` of its key. Give the id the other
  widgets use: a list keyed by row number names its entity's path.
- The view is drawn inside the selection's focus surface, where
  Alt+Left and Alt+Right walk the history.

A widget of the application's (a canvas, a viewport) uses the service
directly: `contains` and `is_hovered` to draw, `hover` every pass,
`click` on a click, and `selection::surface(ui, ...)` around it for the
history's keys.

```rust,ignore
use egui_pop::api::selection::{self, Gesture};

selection::surface(ui, |ui| {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let under = entity_at(response.hover_pos());
    service.hover("scene.view", under.as_ref());
    if response.clicked() && let Some(item) = &under {
        let gesture = Gesture::of(ui.input(|i| i.modifiers));
        service.click(item, gesture, &[], "scene.view");
    }
    // Paint each entity, selected when `service.contains(item)`, ringed
    // when `service.is_hovered(item)`.
});
```

The gallery's **Shared selection** page shows a tree, a list and a canvas
sharing one selection.

## For agents: `selection.*`

The `selection` collector shows the items (ids, in the order selected),
their count, the item hovered and by which widget, the anchor, the
revision, who made the last change, and how many selections back and
forward go to.

| Command | Arguments | Answer |
|---|---|---|
| `selection.set` | `items` (ids) | Selects them, and only them: `changed`, `items`, `count`. |
| `selection.add` | `items` | Adds them. |
| `selection.clear` | none | Selects nothing. |
| `selection.hover` | `item` (an id, or null) | Hovers it everywhere until a widget or another call hovers something else: `hovered`. |
| `selection.back`, `selection.forward` | none | The selection before or after; refused when there is none (`selectionCanBack`, `selectionCanForward`). |

`set`, `add`, `clear` and `hover` are kept out of the command palette;
Back and Forward are there.

## Not included

- Undoing a selection: back and forward are its history.
- A selection per window or per document: there is one per
  application. Several would be several services, a later change.
- Views other than the tree and the list (the table, the library's
  grid) do not bind yet.
