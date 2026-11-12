---
egui-pop-api: minor
egui-pop-i18n: minor
egui-pop: minor
---

The Settings app has two panes. On the left, a tree lists the scopes
(General, Interface, Input, and those plugins add) with their sections
nested under each; it replaces the tabs on top, which ran off a narrow
window and pushed the page past its right edge. On the right, the
section's page wraps to the width it gets. The divider between them is
dragged, or moved with its arrow keys (Enter or a double click gives
the default back), and its width is kept across reopens and runs
(`admin::NAV_WIDTH`, `admin::nav_width`, `admin::want_nav_width`). The
search sits above the tree; the arrow keys moving through the tree show
the page they land on.
