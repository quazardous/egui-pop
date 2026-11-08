---
egui-pop-api: minor
egui-pop: minor
---

A shell panel's least and largest sizes change while the application
runs: `AppHandle::set_shell_panel_min_size(panel, Some(size))` and
`set_shell_panel_max_size` (`None` puts back the default), read back with
`shell_panel_min_size` and `shell_panel_max_size`. A least width
measured from the panel's content (a header's buttons, which depend on
the fonts and the text size) is set there, and the area widens to it on
the next pass.
