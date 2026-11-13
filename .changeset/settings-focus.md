---
egui-pop-api: minor
egui-pop: minor
---

The Settings can edit one of the host's foci (`Admin::set_foci`, `SettingsFoci`): opened from a window, they edit that window's focus, whose store the host attaches. Once there are several foci, a header names the one edited ("Settings for") and its switch picks another (`admin.focus` for agents). A setting with no host level is the app's, the same in every focus.
