---
egui-pop-views: minor
egui-pop: minor
---

A row label too long for its row is elided with "…" and shown whole on hover, instead of being cut.

- **Rows.** A list item, a tree row and a list's default row draw their label on one line, elided with "…" before the row's trailing text, buttons and widgets, which stay whole. Hovering the row then shows the full label as its tooltip; the row's accessible name stays the full label.
- **Table cells.** A data table's cell `Ui` truncates text, so an egui label too long for its column is elided with "…" and shows the full text on hover, instead of being cut at the column's edge.
