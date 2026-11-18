---
egui-pop-api: major
egui-pop: major
---

The file browser (`FileBrowser`) sorts its entries: by name, by when they last changed, by when they were taken, by size or by type, up or down, with folders first or among the files. People choose the order in a sort menu in its toolbar (not shown in the agenda, which orders by time itself), and an application sets it with `with_sort`/`set_sort` and `with_folders_first`/`set_folders_first`. The selection and the focus stay on their entries. Breaking: `files::sort::SortColumn` has a new variant `Taken` (a photo's capture time, else when it last changed) and is now `#[non_exhaustive]`.
