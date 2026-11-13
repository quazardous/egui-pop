---
egui-pop-views: major
egui-pop-i18n: major
egui-pop: major
---

The thumbnail grid and the filmstrip can carry images out (`ThumbGrid::drag_out`): a drag that starts on a cell signals `Signal::DragOut` with the selection when the cell is in it, else that image alone, for the application to set its drag payload; the rubber band then starts between cells. While it lasts the cursor shows a carry and the pointer how many images go. `Signal` gains the `DragOut` variant.
