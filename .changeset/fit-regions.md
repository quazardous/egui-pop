---
egui-pop-fit: minor
egui-pop: minor
---

Slot regions: `fit::layout_regions` places cards in named regions around and over a surface, such as read-outs and tool chips on an image: corners, edges, the centre and an overlay (`Anchor`), over the surface or around it within bounds (`Side`), each with a gap, a priority, a flow and a depth. Regions take their room in priority order; each packs its cards with the slot engine from its anchor, folds its lowest-priority cards when room runs out, and mirrors start and end right to left. The gallery's regions page shows them.
