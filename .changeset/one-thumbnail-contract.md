---
egui-pop-views: minor
egui-pop: minor
---

The library's views take the file browser's thumbnails and draw their badges as the browser draws its marks. `library::thumbs::ProviderThumbs` is a `ThumbSource` over a `files::browser::ThumbnailProvider` for one frame (made before the views, it begins the provider's frame; dropped after, it ends it), so one provider serves the browser, the picker, the grid, the filmstrip and the comparison. `Thumb::from(&ThumbState)` and `MarkChip::from(Badge)` convert between the two. A grid's badges are indicators, bottom right, under the id `library.badge:<image>:<text>`, as many as fit in the cell.
