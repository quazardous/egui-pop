---
egui-pop-api: major
egui-pop: major
egui-pop-editors: major
egui-pop-i18n: major
egui-pop-imaging: major
egui-pop-platform: major
egui-pop-test: major
egui-pop-views: major
---

The file browser's marks can be icons: `EntryMarks::chips` puts `MarkChip`s in a corner of an entry's thumbnail, each an icon of the registry (`MarkChip::icon("phosphor:check-circle", "in your Library")`, top right by default) or a short text (`MarkChip::text("R+J", "RAW+JPG")`, bottom right), on a pill of its tone (`.tone`), in the corner chosen (`.corner`, `MarkCorner`). A chip's name is its tooltip and is said after the entry's name, as badges are. `paint_marks` gives where each chip was drawn.

- **Breaking:** `EntryMarks` has a new public field, `chips`: a struct literal gives it, or ends with `..Default::default()`.
