---
egui-pop-core: minor
---

Rows and grids in the `UiPop` helpers: `ui.row(Row, ..)` places one line of cells and `ui.grid(Grid, ..)` lines of cells on shared tracks (`egui_pop_core::ui::layout`). A `Track` is sized by its content, a fraction of what is left, a percentage of the container or em (lines of body text), each with a `Length` minimum and maximum, never points. Cells sit on one text baseline by default (or at the start, centre or end), measured from the text they paint, and gaps are `Space` intents only. A row can share its width out equally among its content cells (a button bar) and sit at the end of its line; a header/value grid (`Cells::header`) folds to headers over values below a width (`Grid::fold_below`). Sizes come from the previous pass, with one more pass when they change, as egui's `Grid` does.
