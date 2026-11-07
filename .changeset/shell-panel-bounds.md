---
egui-pop-api: minor
egui-pop: minor
---

An application sets how narrow and how wide the shell's panels get:
`AppBuilder::shell_panel_min_size(panel, size)` replaces egui-pop's
least width (120 points for the side bar, 160 for the inspector),
`shell_panel_max_size(panel, size)` sets a largest one, in points or as
a share of the window (`workbench::ShellSize`), and
`AppBuilder::centre_min_size(size)` keeps the centre at least that wide,
the side areas giving way to it down to their own least. Nothing set,
the panels size as before.
