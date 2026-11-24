---
egui-pop-api: minor
egui-pop: minor
---

An application words its arrangements its own way:
`AppBuilder::arrangement_strings(ArrangementStrings::new(save_as,
save_title, save_verb, suggested))` replaces the layout menu's "Save the
current arrangement as…", the save dialog's title and button and the
name it suggests by whole strings under its own keys, translated by its
catalogues (a "Studio layout" instead of an "arrangement"). Without it,
egui-pop's texts stay.
