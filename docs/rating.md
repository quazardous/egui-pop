# Rating stars

A rating is zero to five stars. egui-pop draws every rating with one
widget, `egui_pop::core::rating::RatingStars`: a thumbnail's overlay in
the library grid, the library's marks panel
(`views::library::marks::RatingStars`, the same widget returning a
`MarkResponse`), a table's column, the file dialog's preview. This page
is the short study its behaviour comes from, then what egui-pop does
and how a kit restyles it.

## How photo applications rate (2026-11-06)

Read from each application's manual and support pages; the
applications were not run for this study.

| | Lightroom Classic | darktable | digiKam | Adobe Bridge | Apple Photos |
|---|---|---|---|---|---|
| Unrated item | Five dots under the thumbnail; with "show clickable items on mouse over only", under the pointer alone | The overlay's stars show as outlines, always or on hover (the overlay setting) | Outline stars under the thumbnail, under the pointer | Five dots under a selected thumbnail, hidden in small thumbnails | No stars: a favourite (a heart) only |
| Under the pointer | The stars a click gives | The stars up to the pointer light | The stars up to the pointer light | The dots and stars, clickable | The heart, clickable |
| Setting | A click on the n-th star or dot | A click on the n-th star | A click on the n-th star | A click on the n-th dot | A click toggles the favourite |
| Clearing | A click on the star of the current rating (on a five-star photo, the fifth) | A click on the first star of a one-star image (the preference "rating an image one star twice will not zero out the rating" turns it off) | No click: the keys | The "no rating" icon left of the stars, or a filled star of a selected thumbnail | The same click, a toggle |
| Keys | 0 to 5 (0 clears), `[` and `]` step | 0 to 5, R rejects | Ctrl+0 to Ctrl+5 | Ctrl+0 to Ctrl+5 (0 clears) | `.` toggles the favourite |
| Read-only | The metadata panel, the list of a collection | The image information panel | The properties sidebar | The metadata panel | The heart badge on favourites |

What they share:

- **An unrated item shows where its rating goes**, as dots or outlines,
  at least under the pointer. digiKam's bug tracker records the
  complaint the other way round: a blank area under a thumbnail that
  rates on a click is an accident waiting.
- **The pointer previews**: the stars up to it light before the click.
- **A click on the current value clears it**, in Lightroom without
  exception, in darktable for one star; Bridge and the keys give
  another way (an icon, the 0 key).
- **The keys are 0 to 5**, with or without Ctrl; 0 clears.
- **The click area is the stars**, not the row around them (digiKam
  narrowed it after accidental ratings).

Sources: Adobe's "Flag, label, and rate photos" (Lightroom Classic) and
"Label and rate files" (Bridge), darktable's user manual ("star ratings
& color labels"), digiKam's bug tracker (bugs 192425, 228810, 238232),
Apple's Photos for Mac guide.

## What egui-pop does

The recommendation, built:

- **Empty stars are faint outlines** (`EmptyStars`): `Always` in a
  panel, a row or a cell with room for them (the default), `OnHover`
  over a thumbnail, where they show while the cell or the stars are
  under the pointer. The grid's default is `OnHover`;
  `ThumbGrid::empty_stars(EmptyStars::Always)` shows them on every
  cell (the gallery's "Empty stars on every cell").
- **Under the pointer** the stars up to it light in the preview look,
  the rest stay outlines; the outlines never go while the pointer is on
  the stars, gaps included. The star under the pointer is read from its
  position across the row, so the preview never flickers between two
  stars.
- **Changing a value**: over a lower star, the preview shows the
  smaller value and the current one stays as a ghost (3 stars, the
  pointer on the first: one star lit, two ghosts, two outlines).
- **Clearing**: a click on the star of the current value clears the
  rating (Lightroom's rule); its preview is the current value all in
  ghosts. Each star's tooltip, and its accessible name, says what a
  click does: "Set 2 of 5", or "Clear rating" on the current star.
- **Keys**: a focusable rating takes 0 to 5 (0 clears), the arrows
  step it, Home and End go to 0 and 5; it reads as a slider. A view with
  its own keys turns focus off (`focusable(false)`): the library grid
  rates with its culling keymap (Lightroom's 0 to 5).
- **Read-only** (`read_only(true)`, or a disabled `Ui`): no preview, no
  click, no focus; the stars are drawn by `rating-star-display` and read
  as "3 of 5".

```rust
use egui_pop::core::rating::{EmptyStars, RatingStars};
use egui_pop::egui;

fn ui(ui: &mut egui::Ui, rating: &mut u8, hovered: bool, room: egui::Rect) {
    // A panel: outlines always, focusable.
    if let Some(r) = RatingStars::new(*rating).show(ui).changed {
        *rating = r;
    }
    // A table's column: shown only.
    let _ = RatingStars::new(*rating).read_only(true).size(14.0).show(ui);
    // An overlay: laid out by the caller, outlines while its item is hovered.
    let out = RatingStars::new(*rating)
        .id(ui.id().with("stars"))
        .focusable(false)
        .empty(EmptyStars::OnHover)
        .item_hovered(hovered)
        .show_at(ui, room);
    if let Some(r) = out.changed {
        *rating = r;
    }
}
```

`RatingResponse` holds the row's `response`, the value asked for
(`changed`, never the current one), the value a click would give while
the pointer is on the stars (`preview`) and whether it is (`hovered`).

Not decided: whether the darktable pointer profile should clear only
from one star, as darktable does. Pointer profiles do not touch the
rating today, and Lightroom's rule covers darktable's case.

## Kit components

The stars are the kit's (`docs/kit.md`), drawn by the `star` handler
(the fill, then the outline, in a part one star wide):

| Look | `rating-star` | Default |
|---|---|---|
| Empty | `rest`: `border-color`, `border-width` | `text.muted`, 1 px, no fill |
| Given | `selected`: `fill`, `border-color` | `warning` |
| Preview | `hover`: `fill`, `border-color` | `warning` at 55 %, its outline full |
| Ghost | `tone.muted.selected`: `fill`, `border-color` | `warning` at 25 %, its outline at 60 % |

`rating-star-display`, a read-only rating's star, is
[based on](kit.md#components-based-on-another) `rating-star`: restyling
the rating restyles it too; its own empty outline is the quieter
`border.strong`. Every built-in theme takes these defaults from the
default kit; a kit package (Rerun-like, a flavour) restyles them with
data only, as the gallery's `library-kit` snapshot does. The given star
and the empty outline are checked for contrast on the panel and the
raised surface (`contrast::PAIRS`, 3:1); the preview and the ghost are
lighter on purpose.

The `library.star` and `library.star-empty` tokens
(`LibraryTokens`) no longer colour the stars; `library.star-empty`
still colours the pick and reject toggles when off.
