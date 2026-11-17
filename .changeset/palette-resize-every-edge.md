---
egui-pop-api: patch
egui-pop: patch
---

A floating palette is resized by each of its four edges and four corners, each with the cursor of its direction: before, only its right edge, its bottom edge and the corner between them did, so it could not be widened leftwards. A left or top edge keeps the opposite edge where it is and the least width holds; its drop runs `palette.resize`, then `palette.move` for the new place.
