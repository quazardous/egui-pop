---
egui-pop-views: minor
egui-pop: minor
---

`SearchField::whole_text(true)` leaves the text typed to the application:
it is not split into the field's criteria, Enter makes no chip of it, and
the filter's `text` is the text as typed, for an application reading it
with its own grammar. The field's own parsing stays the default.
