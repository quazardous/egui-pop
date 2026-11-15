# Thumbnails

The contract of the family "Thumbnails"
([ADR 0027](adr/0027-component-families.md)): a thumbnail from an image
the application supplies, and a virtualised grid of them.

| Module | What it does |
|---|---|
| `egui_pop_core::thumbnail` | `Thumbnail`, `thumbnail_grid` and `ThumbnailGrid`, `fitted`, `caption_height` |
| `egui_pop_viewport::thumbnail` | The same items, re-exported for the image tools |

The drawing lives in `egui-pop-core` so that both the image tools and
the application API's file picker (`egui_pop_api::files`) draw them
(ADR 0001); the viewport crate only re-exports it. The photo library's
grid and filmstrip ([The photo library](library.md)) are a view over
the application's catalogue, built on their own; this page is the
plain widget.

**Owns**: a thumbnail's drawing (the image fitted, a placeholder colour
until it is there, a sign, a caption, the selection), and a grid that
builds only the rows in view.

**Refuses**: generating thumbnails from files (the application or the
platform services' `thumbnails` do); a spinner per thumbnail; a
catalogue's storage.

## The API

```rust
use egui_pop_core::thumbnail::{Thumbnail, ThumbnailGrid};

fn ui(ui: &mut egui::Ui, selected: &mut usize, reveal: Option<usize>) {
    ThumbnailGrid::new(10_000, egui::vec2(96.0, 72.0))
        .id_salt("results")
        .reveal(reveal)
        .show(ui, |ui, i| {
            let r = ui.add(
                Thumbnail::new(format!("Image {i}"))
                    .placeholder(egui::Color32::from_gray(90))
                    .caption(format!("IMG_{i:04}.jpg"))
                    .selected(i == *selected),
            );
            if r.clicked() {
                *selected = i;
            }
        });
}
```

- `Thumbnail::new(label)`: `image(texture, [w, h])` (drawn fitted,
  keeping its aspect), `placeholder(colour)` (the image's average,
  shown until the image is there), `icon` (a sign for what has no image:
  a folder, a broken file), `caption` (cut to the width), `selected`,
  `size` (default: the grid cell, or 96 × 72).
- `thumbnail_grid(ui, count, cell, item)` is `ThumbnailGrid::new(count,
  cell).show(ui, item)`. `ThumbnailGrid::show` returns the number of
  columns; `max_height` bounds it, `reveal` scrolls by the least that
  shows a cell (one not drawn yet included), `id_salt` tells two grids'
  scrolling apart in one `Ui`.
- `fitted(outer, [w, h])`: the largest rectangle of that aspect inside
  `outer`, centred; `caption_height(ui)`: the room a caption takes.

## States and failure

- No image: the placeholder colour, the sign over it if any. There is
  no loading state of its own: the application gives the image when it
  has it.
- An image of a different aspect than the cell is letterboxed, never
  stretched; a caption and a sign stay inside the thumbnail.
- The grid's rows stay where they are as it scrolls; at its end the
  grid keeps the wheel, so the page around it does not scroll on.

## Accessibility

A thumbnail is a selectable button named by its label (give a label
even when the caption is hidden). The grid is a scroll area; keyboard
moves between cells are the caller's (the photo library has its own
keymap).

## Tier

From [API stability](stability.md): **Experimental** ("Thumbnails and
their grid (`egui_pop_core::thumbnail`, re-exported by
`egui_pop_viewport::thumbnail`), with the photo library").

## Tests

- `crates/egui-pop-viewport/tests/images.rs`: a thumbnail named,
  selectable and clicked; only the rows in view built; a cell not drawn
  brought into view and the columns said; rows steady while scrolling;
  caption and sign inside; aspect kept.
- `crates/egui-pop-core/tests/wheel.rs`: the grid keeps the wheel at its
  end.
- The file picker's thumbnails: `crates/egui-pop-api/tests/files.rs`.
