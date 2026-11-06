---
egui-pop-tokens: minor
egui-pop-core: minor
egui-pop-api: patch
egui-pop-views: minor
egui-pop-i18n: patch
egui-pop: minor
---

One rating widget everywhere, `egui_pop::core::rating::RatingStars`,
behaving as photo applications do (`docs/rating.md`):

- An unrated item shows faint outline stars, always or while it is
  hovered (`EmptyStars`; `ThumbGrid::empty_stars` for the grid, under
  the pointer by default), so it says it can be rated. The outlines no
  longer vanish when the pointer moves onto a thumbnail's stars.
- Under the pointer, the stars up to it preview the value a click gives;
  over a lower star the current value stays as a ghost. A click on the
  current star clears the rating; each star's tooltip and accessible
  name say what a click does ("Set 3 of 5", "Clear rating").
- `read_only(true)` shows a rating without editing it, read as
  "3 of 5": the file dialog's preview uses it.
- The library grid's stars, the library's `marks::RatingStars` (now a
  wrapper of the shared widget, with `read_only` and `empty`) and the
  file dialog's preview draw the same stars.
- The stars are kit components: `rating-star` (`rest` an empty outline,
  `selected` given, `hover` the preview, `selected` in the `muted` tone
  the ghost) and `rating-star-display`, based on it, drawn by the new
  `star` handler. A kit restyles them with data; the given star and the
  empty outline are contrast pairs. `library.star` no longer colours
  the stars.
