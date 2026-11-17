---
egui-pop-api: major
---

A floating palette resized by its left or top edge (0.12.1) is resized and moved by one step: its drop runs `palette.resize` with the new place too (`pos`), as the new `workbench::Change::ResizePaletteAt` (a new variant: a break for code matching `Change` exhaustively), instead of `palette.resize` then `palette.move`. `palettes::resize_rect` computes the frame a resize gives.
