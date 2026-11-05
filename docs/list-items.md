# List items and property rows

`egui-pop-views` (the facade's `views` feature) draws one dense row
everywhere: the tree's rows, the list's rows, and rows of their own in a
panel. Its parts, left to right:

- a **drag handle**, shown with the actions (`ListItem::drag_handle`);
- **indent guides**, a line down the row for each level of its
  ancestors (`ListItem::guides`, `TreeView::guides`);
- the **collapsing arrow**'s column (`ListItem::collapsing`, or
  `Disclosure::Leaf` for a leaf aligned with branches);
- an **icon** in the row's text colour;
- the **label**, cut before what follows;
- **weak trailing text** in the row's muted tone (a count, a type);
- **action buttons** (a visibility eye, remove): shown while the row is
  hovered, focused or selected, or always (`Reveal::Always`); an action
  marked `pinned` always shows (a hidden layer's closed eye).

A row is `selected`, `hover`, `active` (pressed, or `ListItem::active`),
`focus` and `disabled`, the kit's states.

```rust
use egui_pop::egui;
use egui_pop::core::icon::Icon;
use egui_pop::views::item::{ItemAction, ListItem};

fn layers(ui: &mut egui::Ui, hidden: &mut [bool; 2], selected: &mut usize) {
    for (i, name) in ["Sky", "Ground"].into_iter().enumerate() {
        let eye = if hidden[i] {
            ItemAction::new("visibility", Icon::Hidden, format!("Show {name}")).pinned(true)
        } else {
            ItemAction::new("visibility", Icon::Visible, format!("Hide {name}"))
        };
        let out = ListItem::new(name)
            .icon(Icon::File)
            .depth(1)
            .guides(true)
            .trailing("layer")
            .selected(*selected == i)
            .action(eye)
            .id_salt(name)
            .show(ui);
        if out.action() == Some("visibility") {
            hidden[i] = !hidden[i];
        } else if out.response.clicked() {
            *selected = i;
        }
    }
}
```

`ItemResponse` says what happened: the row's `response` (clicked, double
clicked, right clicked), `toggled` (the arrow, Right, Left, or a screen
reader's expand or collapse), the `action` clicked, and the drag
`handle`'s response.

## The row height

Rows are `size.row.height` high, a token of their own: by default an
alias of `size.control.height`, so rows follow the density (24 points
comfortable, 20 compact) and the text size, as they did. A theme or a kit
makes rows denser or roomier than its buttons by giving the token a value
(`item::row_height` reads it). The tree, the list and the items all use it.

## In the tree and the list

The tree's rows are list items: `TreeView::icon`, `TreeView::trailing`,
`TreeView::guides` and `TreeView::actions` (with `TreeView::on_action`)
give their parts, `TreeView::row_widgets` keeps adding the application's
own widgets at the right end. An action's click runs the `tree.action`
step (`tree`, `key`, `action`; a veto ignores it), then `on_action`; it
does not select the row. The list's rows take an icon and trailing text
from `ListRows::icon` and `ListRows::trailing`, before what
`ListRows::ui` draws.

## The keyboard

- A standalone item takes the focus with Tab. Enter or Space clicks it;
  Right and Left open and close a collapsing one.
- Its action buttons follow it in the Tab order. They show while the
  item or one of them has the focus, so Tab walks from the item through
  its buttons to the next widget, and Shift+Tab walks back. Enter or
  Space on a button runs its action.
- In the tree, the cursor's row shows its actions while the tree has
  the focus, and they come right after the tree in the Tab order: Tab
  from the tree reaches the cursor's buttons, Tab from the last leaves
  the tree, Shift+Tab goes back to it. The arrows move the cursor as
  before; its new row's buttons follow.
- The drag handle is for the pointer: an application offers the same
  move through its commands (move up, move down) for the keyboard.

## Accessibility

A standalone item is a `ListItem` node (`ListItem::role` changes it)
named by its label, with its selected state, `expanded` and the expand or
collapse action when it collapses, and its level when nested. Each
action is a button named by its label, and also a custom action of the
row, so a screen reader offers it on the row itself. The tree's rows keep
their role and names; rows with actions list them as custom actions.

## Property rows

`PropertyRow` splits the same row in two: the property's name, in the
muted tone, and its value, any widget. The names of rows sharing a column
id (`PropertyRow::column`; by default the rows of one `Ui`) take the
width of the widest, so their values line up; the column never takes more
than half the row. A value is editable unless the row is `read_only`
(its widgets disabled).

```rust
use egui_pop::egui;
use egui_pop::views::property::PropertyRow;

fn points(ui: &mut egui::Ui, radius: &mut f32, visible: &mut bool, locked: bool) {
    PropertyRow::new("Path").text(ui, "world/points");
    PropertyRow::new("Archetype").text(ui, "Points3D");
    PropertyRow::new("Radius")
        .read_only(locked)
        .widget(ui, egui::DragValue::new(radius).speed(0.05));
    PropertyRow::new("Visible").widget(ui, egui::Checkbox::without_text(visible));
}
```

`text` shows plain, selectable text; `widget` adds a widget named by the
property for screen readers; `show` takes any content.

## The kit

- `list-item`, based on `list-row` (a kit restyling rows restyles items
  and the tree's and list's rows), adds a fill while `active`, muted text
  while `disabled`, and the `muted` tone for trailing text and property
  names.
- `list-guide`: an indent guide, the `outline` handler's line along a
  part with no width (`border.default`, one point).
- The action buttons are `icon-button`s; their icon is in the row's muted
  tone until hovered or focused. The arrow is the kit's disclosure glyphs.

The gallery's `list-items` page shows items, the same rows in a tree, and
a property panel like a viewer's selection panel.
