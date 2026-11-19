---
egui-pop-api: minor
egui-pop: minor
---

`FileBrowser` filters, ranks and selects by kind, as the in-app picker does: `set_filter` shows only the entries an `EntryFilter` passes (extensions in any case, folders kept for going through them, or one kind of entry; a group follows its main file), `set_query` shows those a fuzzy query matches, best first, a group answering to its members' names too, and `set_selection_policy` with a `SelectionPolicy` lets the person select files, folders or any, one or several (a click, the keys, a rubber band, select all and `set_selection` keep to it; a refused entry is still focused and opened). What a filter or a query hides leaves the selection and the focus. The picker ranks and filters through the same code. The gallery's file browser page has a field, a filter and a "Select files only" switch.
