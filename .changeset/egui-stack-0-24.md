---
egui-pop-model: major
egui-pop-core: major
egui-pop-api: major
egui-pop: major
egui-pop-editors: major
egui-pop-i18n: major
egui-pop-imaging: major
egui-pop-platform: major
egui-pop-test: major
egui-pop-viewport: major
egui-pop-views: major
---

egui-pop moves to egui_stack 0.24: drops show as zones, and an area's groups are arranged in rows, columns and grids.

- **Drop zones.** A tab dragged inside an area shows the region it would take on the group under the pointer itself, bordered and tinted, with no buttons: the half of the group near an edge, the whole group elsewhere. It is the kit's `dock-preview`, so it follows the theme; the same tokens feed egui_stack's `OverlayStyle::zone_fill`, `zone_stroke` and `zone_corner_radius`. A tab dropped outside the dock opens in a window of its own when its panel may detach. The cross of buttons is no longer used.
- **Rows, columns and grids.** A drop on the edge of a group in a row or a column joins it rather than nesting a split. New commands "Arrange as grid", "Arrange as row" and "Arrange as column" (`workbench.arrange.grid`, `.row`, `.column`, in English and French) lay out the groups beside the focused panel's again, or beside the `panel` an agent names (`columns` for a grid); they go through the new `workbench.arrange` step, `AppHandle::arrange` from code (`workbench::Arrangement`).
- **The layout document** saves a grid as a node of kind `grid` (`columns`, `col_shares`, `row_shares`, `children`): `layout::Node` has a new variant, `Node::Grid`. A row or column of more than two is saved as splits nested on their second side. Layouts saved before read as they did.

Migration: a `match` on `egui_pop::model::layout::Node` needs a `Node::Grid` arm. egui_stack types re-exported by egui-pop are those of 0.24: see its changelog (`Node::Linear` and `Node::Grid` replace `Node::Horizontal` and `Node::Vertical`; `OverlayType::Zones` is the default).
