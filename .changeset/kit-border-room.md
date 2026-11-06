---
egui-pop-core: minor
egui-pop-api: patch
egui-pop: minor
---

A kit part's content no longer moves when its state changes: a state
restyles a part by paint alone (fill, border colour and width, radius,
ring, shadow), and the layout keeps room for the widest border any state
gives, the border at rest then present but invisible. A selectable row
or a toggle chip whose border appears under the pointer (the default
kit's `selectable-row`) kept its text in place only at rest; it now
keeps the same size and text position in every state (#236).
`Component::show` lays its content out the same way in every state, and
`Component::border_room` gives the room a widget keeps for the border.
`Kit::components` walks every component of a kit.
`SelectableRow::steady` is deprecated: every row is steady now; drop
the call.
