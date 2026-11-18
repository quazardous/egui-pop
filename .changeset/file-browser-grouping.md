---
egui-pop-api: minor
egui-pop: minor
---

The file browser (`FileBrowser`) groups files that go together, by the application's groupings (`with_grouping`, over `files::groups`, as the in-app picker does): a RAW with its sidecars and its JPEG is one entry, badged with its members (`+XMP +JPEG`), said to screen readers with them ("Grouped with …"), and showing a member's thumbnail when it has none of its own. Its members leave `entries`, and `group_of` gives an entry's group. The groups are found as the listing's pages come, a large place's off the interface thread (`groups_pending`). People turn grouping off and on in the sort menu, and an application with `set_grouped`.
