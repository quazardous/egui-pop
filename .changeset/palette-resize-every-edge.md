---
egui-pop-api: major
---

A floating palette is resized by each of its four edges and four corners, each with the cursor of its direction: before, only its right edge, its bottom edge and the corner between them did, so it could not be widened leftwards. A left or top edge keeps the opposite edge where it is and the least width holds; the drop runs `palette.resize` with the new place too (`pos`), as the new `workbench::Change::ResizePaletteAt` (a new variant: a break for code matching `Change` exhaustively). `palettes::resize_rect` computes the frame a resize gives.
