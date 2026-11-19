---
egui-pop-api: minor
egui-pop: minor
---

The in-app picker lists through a `FileBrowser`: its folder is read off the interface thread on the system's file system, its entries coming in over the next frames (`FilePicker::loading` says while they do), and the browser filters, orders, groups and ranks them by the field's text. Its own listing, sorting and grouping code is gone; `FilePicker::entries` and `visible` read the browser. `FileBrowser::set_show_hidden` lists hidden entries too. Under a filter that refuses a group's main file, the browser shows the group's members as entries of their own, as the picker did.
