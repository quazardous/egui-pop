---
egui-pop-api: minor
egui-pop: minor
---

A `PlacesPanel` can hold an import window's sources column, each part
opt-in and the panel unchanged without them: a section folds
(`PlaceSection::foldable`, or `PlaceSection::open` when the application
keeps the state, reported in `PlacesOutput::folded`), says what it lacks
when empty (`PlaceSection::empty`) or stays shown for the application's
content (`PlaceSection::keep`); an item shows a line under its name
(`PlaceItem::line`, said as its description), a badge in a tone
(`PlaceItem::badge_tone`) and a toggle of the application's at its end
(`PlaceToggle`, a pin, reported in `PlacesOutput::toggled`); and
`PlacesPanel::under_item` and `PlacesPanel::under_section` give the
application the room under an item or a section's items (an account's
albums, a history's past imports), in the panel's keyboard order.
`PlaceProviders::sections` and `Places::sections` keep a section with an
empty line or kept.
