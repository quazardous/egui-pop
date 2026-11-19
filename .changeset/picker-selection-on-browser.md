---
egui-pop-api: minor
egui-pop: minor
---

The in-app picker keeps its highlight and selection in its `FileBrowser`, under a `SelectionPolicy` by the kind of choice: several files, one file, or one folder (saving: the folder Enter goes into). A click, the arrows, Ctrl+Space, select all and the rubber band go through the browser; `FilePicker::highlight` and `selected` stay public and read the browser after each step, and a value the application sets is taken at the next one, kept to the policy. The browser gains `SelectionPolicy::keeps_filtered` (the selection keeps the entries the filter or the query sets aside, as the picker's field always did) and `FileBrowser::set_focus`; a move without Shift takes the anchor along, a Shift range with no anchor starts from the entry the focus leaves, and a rubber band selects nothing when one entry is chosen at a time.
