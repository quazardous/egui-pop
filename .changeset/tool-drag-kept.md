---
egui-pop-viewport: patch
---

A drag on the curve editor, a colour wheel, the histogram or the navigator goes on to its release whatever its own change does around it. Each tool now senses under an id keyed by its label in its `Ui`, not egui's automatic id: a widget showing up above it once its value moved (a "modified" mark, a button) gave it another id, and egui ended the drag after its first frame. And the histogram stays live through a drag begun on a current picture when its panel turns stale under it (`areas::inert`), its own change the cause: the drag was given up after its first frame. Two tools with the same label in one `Ui` would share an id: name them apart.
