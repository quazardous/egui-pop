---
egui-pop-core: major
egui-pop-tokens: minor
egui-pop: major
egui-pop-api: major
egui-pop-editors: major
egui-pop-i18n: major
egui-pop-imaging: major
egui-pop-platform: major
egui-pop-test: major
egui-pop-viewport: major
egui-pop-views: major
---

Dense list items and property rows (egui-pop-views), in the style of Rerun's list items. `item::ListItem` is one row for lists, trees and panels: a drag handle, indent guides, a collapsing arrow, an icon, its label, weak trailing text and action buttons (`item::ItemAction`: a visibility eye, remove) shown while the row is hovered, focused or selected, or always, a pinned action at rest too. It is `selected`, `hover`, `active`, `focus` and `disabled`; its accessible node is a `ListItem` with its selected and expanded states, its level, and its actions as custom actions; Tab walks from the row through its buttons. `property::PropertyRow` splits the row into a name and a value (any widget, read-only or editable), the names of the rows sharing a column id taking the widest's width so that their values line up. The tree's and the list's rows are list items, looking as before: `TreeView::guides`, `TreeView::trailing` and `TreeView::actions` (a click runs the new `tree.action` step, then `TreeView::on_action`; Tab from the tree reaches the cursor's buttons), `ListRows::icon` and `ListRows::trailing`. Rows are `size.row.height` high, a new token (egui-pop-tokens, `Size::row_height` in egui-pop-core) kept apart from `size.control.height`, which it aliases by default: a theme makes rows denser or roomier than its buttons. The kit gains `list-item` (based on `list-row`: an `active` fill, `disabled` and `muted` text) and `list-guide` (the `outline` handler draws a line along a part with no width). The drag handle's name is translated (egui-pop-i18n, "Move { $name }").

Migration: `kit::Kit` has two new fields (`list_item`, `list_guide`); a `Kit` built from a struct literal adds them, or comes from `Kit::compile` or `Kit::of`.
