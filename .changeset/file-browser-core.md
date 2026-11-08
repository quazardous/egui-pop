---
egui-pop-api: minor
egui-pop: minor
---

A file browser with pluggable view modes (`files::browser`), the first step
of one browser for every file dialog and an import window:

- `FileSource`, paged and streamed entries with errors (`FsSource` over a
  `FileSystem`);
- `ViewMode` and its registry `ViewModes`, with the built-in `details` and
  `thumbnails` modes;
- `ThumbnailProvider`, requests with a size and a priority, cancelled when
  not renewed (`PreviewThumbnails` over the preview providers);
- `FileBrowser`: selection, focus, keyboard per mode, type-ahead, and a
  mode switcher remembered under the browser's id.
