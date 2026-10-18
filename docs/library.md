# The photo library

`egui_pop::views::library` (crate `egui-pop-views`) holds the widgets of
a photo library, the kind of view used to import, cull and sort images:

- a **thumbnail grid**, virtualised (a hundred thousand images cost
  what a screenful does), and a **filmstrip**, the same view in one row,
  sharing the grid's selection;
- **ratings** (0 to 5 stars), **colour labels** and **pick or reject
  flags**, drawn over the thumbnails and edited with the mouse, the keys
  of a culling keymap (Lightroom's), or their own widgets;
- **stacks**, collapsed to their top image with a count, or expanded;
- a **comparison** of two to four images, zoom and pan synced;
- **tag chips** and a **tag input** completing from the application's
  tags;
- a **search field with filter chips**, giving the application a
  structured filter.

The application owns its images, their marks, their tags and the
filtering. The widgets show what the application gives them and say
what the user asked for, in their output and as steps (ADR 0011) a
plugin may veto, redirect or follow. Agents drive them through
commands.

The gallery's `library` page shows all of it on 10 000 synthetic
images.

## The grid and the filmstrip

The application implements two traits:

```rust
use egui_pop::views::library::{Badge, LibraryItems, Marks, Stack, Thumb, ThumbSource};

struct Shoot { /* the application's images */ }

impl LibraryItems<u64> for Shoot {
    fn len(&self) -> usize { todo!() }
    fn key(&self, index: usize) -> u64 { todo!() }      // the selection is kept by key
    fn label(&self, index: usize) -> String { todo!() } // accessible name and caption
    fn marks(&self, index: usize) -> Marks { todo!() }  // rating, labels, flag
    fn stack(&self, index: usize) -> Option<Stack<u64>> { None }
    fn badges(&self, index: usize) -> Vec<Badge> { Vec::new() }
    fn revision(&self) -> u64 { 0 } // changes when stacks change at the same count
}

struct Thumbs { /* a cache, a loader thread */ }

impl ThumbSource<u64> for Thumbs {
    fn thumb(&self, key: &u64, size: u32) -> Thumb {
        // Ready { texture, size }, or Pending { average } while it loads
        // (and start loading it), or Failed.
        todo!()
    }
    fn wanted(&self, keys: &[u64], size: u32) {
        // The images in view this pass: load these first.
    }
}
```

Only the cells in view are built: `LibraryItems` is asked about those
alone, and `ThumbSource::thumb` too. A thumbnail not there yet is drawn
as a placeholder: the image's average colour when the source knows it
(`Pending { average: Some(..) }`, from an embedded preview, a thumbhash
or a cache), else a skeleton block, never a spinner. The source loads
asynchronously and asks egui for a repaint when a thumbnail arrives.

```rust
use egui_pop::views::library::{CullKeymap, LibraryState, Signal, ThumbGrid};

fn ui(ui: &mut egui::Ui, app: &egui_pop::api::AppHandle, state: &mut LibraryState<u64>,
      shoot: &Shoot, thumbs: &Thumbs) {
    let keymap = CullKeymap::lightroom().auto_advance(true);
    let grid = ThumbGrid::new("grid").app(app).keymap(keymap.clone()).height(400.0)
        .show(ui, state, shoot, thumbs);
    let strip = ThumbGrid::filmstrip("strip").app(app).keymap(keymap).height(96.0)
        .show(ui, state, shoot, thumbs);
    for signal in grid.signals.into_iter().chain(strip.signals) {
        match signal {
            Signal::Rate { keys, rating } => { /* rate them */ }
            Signal::Label { keys, label, on } => { /* label them */ }
            Signal::Flag { keys, flag } => { /* flag them */ }
            _ => {}
        }
    }
}
```

`LibraryState::new("shoot")` names the library in steps and commands;
the grid and the filmstrip drawn over the same state share its
selection, its stacks and the cursor. The state also keeps the
thumbnails' size (`size`, `set_size`).

### Keys and pointer

The view takes the keyboard focus as a whole (a click on a cell gives
it). Its keys, read after the key dispatcher (a key the application
binds to a command never reaches it):

| Keys | Grid | Filmstrip |
|---|---|---|
| Left, Right | the previous, the next image | the same |
| Up, Down | the row above, below (same column) | nothing |
| Page Up, Page Down | a screenful of rows | a screenful of cells |
| Home, End | the first, the last image | the same |
| Shift with a move | extends the selection from the anchor | the same |
| Ctrl with a move | moves the cursor alone | the same |
| Space, Ctrl+Space | selects the cursor's image, toggles it | the same |
| Ctrl+A | selects every image shown | the same |
| Enter (a double click) | `Activate` the cursor's image | the same |
| Ctrl and the wheel, + and - | the thumbnails' size | nothing |

The pointer: a click selects, Shift extends a range, Ctrl toggles; a
drag across the cells draws a rubber band selecting what it touches
(with Ctrl or Shift held at its start, added to the selection). A
cell's stars rate it (the star of its rating clears it) and its stack
chip expands or collapses the stack. Marks made on a selected cell
apply to the whole selection.

### The culling keymap

`CullKeymap::lightroom()` binds Lightroom Classic's culling keys (also
Capture One's for ratings):

| Keys | Action |
|---|---|
| 0 to 5 | the rating (0 clears it) |
| P, X, U | pick, reject, unflag |
| 6, 7, 8, 9 | toggle red, yellow, green, blue (purple has no key) |
| S | expand or collapse the stack under the cursor |
| +, -, = | larger, smaller thumbnails |

`auto_advance(true)` moves the cursor to the next image after a rating,
a flag or a label (Lightroom's Caps Lock mode). `bind(key, action)`
changes one binding and `CullKeymap::none()` has none. A mark applies to
the selection, else to the cursor's image.

These are view keys, like the arrows: they do not appear in the
shortcut editor. An application wanting them rebindable there declares
its own commands and runs `library.rate` and the other steps through
[`change`](#marks-outside-the-grid).

### Overlays and stacks

Over each thumbnail (from 64 points up): the flag (a pennant, or a
cross, the image dimmed when rejected) in the top left corner, the
labels in the top right one, the stars in the bottom left one (empty
stars under the pointer), the badges (`Badge::new("RAW")`, toned with
`Tone`) in the bottom right one. Colour labels have a shape each
(circle, triangle, square, diamond, hexagon), so they never read by
colour alone.

A stack's members follow their top image in the application's order,
each saying `Stack { top, count }`. Collapsed, the top image shows with
its count and a chevron; expanded, the members follow it with a bar on
their left. `library.stack` expands or collapses one (the chevron, S, a
command); `LibraryState::set_expanded` does it without a step.

### Colours

The marks' colours come from the theme, through optional component
tokens falling back on system ones: `library.star` (`status.warning`),
`library.pick` (`text.strong`), `library.reject` (`status.danger`),
`library.label-red`, `-yellow`, `-green`, `-blue` (the status colours)
and `-purple`, `library.band`. `LibraryTokens::of(ctx)` reads them.
The boxes they sit on are the kit's (`docs/kit.md`): `thumb-cell`,
`thumb-layer` (a rejected image's veil, a stack member's bar),
`thumb-overlay-badge`, `pill` (badges by tone), `tag-chip`,
`mark-toggle`, `search-field` and `compare-pane`.

## Marks outside the grid

`RatingStars`, `ColorLabels` and `FlagToggle` edit one image's marks
(a metadata panel, a loupe): each takes the current value and returns
the one asked for in `changed`. Focused, the stars take 0 to 5, the
arrows, Home and End; each label and flag is a toggle button.

To run such a change through the library's steps (so hooks see it as
they see the grid's), call `change`:

```rust
use egui_pop::views::library::{Change, Signal, change};

let signal = change(Some(app), "shoot", Change::Rate { keys: vec![12], rating: 4 });
if let Signal::Rate { keys, rating } = signal { /* apply */ }
```

## Tags

`TagChip::new("beach").removable(true).show(ui)` draws a chip and says
whether its remove button was clicked. `TagInput` shows the tags as
removable chips and a field: Enter, a comma or Tab adds a tag, Up and
Down move in the completions, Backspace in the empty field removes the
last tag. Completions come from a `TagSource` (a `Vec<String>` is one,
matching fuzzily); the output's `added` and `removed` say what changed.

## Search and filter chips

`SearchField::new("shoot").app(app).show(ui, &mut search)` returns the
filter whenever it changes, as a `Filter`:

```rust
pub struct Filter {
    pub text: String,              // words, in the name or a tag
    pub min_rating: Option<u8>,    // at least this many stars
    pub labels: Vec<ColorLabel>,   // one of these labels
    pub flags: Vec<Flag>,          // one of these flags
    pub tags: Vec<String>,         // all of these tags
    pub from: Option<String>,      // YYYY-MM-DD, included
    pub to: Option<String>,
}
```

Criteria typed in the field (`rating>=3` or `rating:3`, `label:red`,
`flag:pick`, `tag:beach` or `#beach`, `date:2024-06-01..2024-06-30`,
`from:…`, `to:…`) filter as they are typed and become chips on Enter;
the `+` menu adds a rating, a label or a flag; each chip has its remove
button, and the clear button removes everything. `Filter::matches`
does the usual matching on a `Candidate` (name, marks, tags, date);
`Filter::parse`, `chips` and `remove` are there for the application's
own fields.

## The comparison

`CompareView::new("compare").synced(true).best(best).show(ui, &mut
state, &panes, &thumbs)` shows two to four `Pane`s (four in a square):
Ctrl and the wheel zoom around the pointer, a drag pans, a double click
goes back to fit; synced, every pane follows. Each pane has a "best"
toggle; the output's `best` is the image the user marked, and the
application says which one is (`best`).

## Steps

`LibraryPlugin` declares them; connect it when the views run with an
application (`.app(app)`). Each runs from the grid, the filmstrip, the
search field, `change`, or a command.

| Step | Arguments | Default |
|---|---|---|
| `library.select` | `library`, `keys`, `cursor` | sets the selection |
| `library.rate` | `library`, `keys`, `rating` | reports it: the application rates its images |
| `library.label` | `library`, `keys`, `label`, `on` | reports it |
| `library.flag` | `library`, `keys`, `flag` (`pick`, `reject`, `none`) | reports it |
| `library.tag` | `library`, `keys`, `add`, `remove` | reports it |
| `library.stack` | `library`, `stack` (the top image's key), `expanded` | expands or collapses the stack |
| `library.filter` | `library`, `filter` | sets the search field's filter |

A before-hook vetoes a mark (the view's output then holds
`Signal::Vetoed`) or redirects it (`Redirect` with other arguments: a
rating capped, other keys); the signal carries what ran. An after-hook
follows them, for example to write the marks to the application's
catalogue.

```rust
ctx.before(egui_pop::views::library::plugin::LIBRARY_FLAG, |_, args| {
    if args["flag"] == "reject" && read_only() {
        Before::Veto("this collection is read-only".into())
    } else {
        Before::Go
    }
});
```

## Commands (agents)

Each acts on `library` (else the library that last had the focus) and
on `keys` (else its selection), and queues its step for the view, which
runs it at its next pass as if the user had asked:

| Command | Arguments |
|---|---|
| `library.select` | `keys`, `cursor` |
| `library.rate` | `rating` (0 to 5) |
| `library.label` | `label`, `on` (else toggled from the cursor's image) |
| `library.flag` | `flag` |
| `library.tag` | `add`, `remove` |
| `library.filter` | `filter` (`{}` clears it) |

The debug bridge's `libraries` collector lists the libraries drawn,
their count, selection, cursor, filter and waiting requests.

## What was checked (2026-10-13)

| Crate | Version, licence | What it has | Decision |
|---|---|---|---|
| egui_virtual_list | 0.12.0, MIT, egui 0.36 | A list of items of varying height, measured as they are drawn and cached; a row may hold several items (a custom layout); no scrolling to an item, no keys, hides itself while resizing | **Own** the grid: its cells are all the same size, so the cells in view, a row's position and the cursor's are arithmetic, and End reaches the hundred-thousandth image at once without measuring the rows before it. egui_virtual_list stays the candidate for lists of rows of varying height (a history, a log) |
| egui_extras | 0.36.2, MIT OR Apache-2.0 | `TableBuilder`, `StripBuilder`, image loaders, a date picker | **Skip**: nothing for a grid of thumbnails; its image loaders are the application's business (the grid takes textures from a `ThumbSource`) |
| egui `ScrollArea::show_viewport` | egui 0.36, MIT OR Apache-2.0 | The visible part of a scrolled area | **Used**: the grid lays its cells out in it and builds those in view, as the list and the tree do |

No code was taken from either crate.

## Gaps

- The culling keys are the view's own (rebinding them is
  `CullKeymap::bind`); they are not commands in the shortcut editor.
- No drag and drop of images out of the grid (to a collection, to
  reorder); a drag draws the rubber band.
- The rubber band does not scroll the grid when it reaches an edge.
- The comparison zooms with Ctrl and the wheel only (no keys), and draws
  the thumbnail it is given: the application gives larger ones for it.
- Stacks are one level deep, and their members must follow their top
  image in the application's order.
