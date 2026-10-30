# Collapsible sections

Status: second version (2026-10-30): heights set by hand and the push.

A stack of collapsible sections fills a panel: each section is a title
band (a disclosure mark, a title, a count) over its own scrolling body.
It is `egui_pop::core::sections`:

```rust
use egui_pop::core::sections::{Section, Sections};

fn sessions(ui: &mut egui_pop::egui::Ui, live: &[String]) {
    Sections::new("sessions").show(ui, |stack| {
        stack.section(Section::new("live", "Live").count(live.len() as u64), |ui| {
            for name in live {
                ui.label(name);
            }
        });
        stack.section(Section::new("shut", "Shut").count(0).open(false), |ui| {
            ui.weak("No shut session");
        });
    });
}
```

- `Sections::new(name)`: the stack's name, unique in the application.
  Its layout is remembered under it, and agents read it by it.
  `capitals(false)` keeps the titles' case on screen (their accessible
  names always keep it).
- `Section::new(key, title)`: the key is what the layout remembers;
  `count(n)` puts a count on the band (`99+` past 99),
  `min_height(points)` its body's least height (two rows by default),
  `open(false)` folds it until the user opens it.
- `show` returns what the user did this pass (`toggled`, `resized`,
  `reset`) and the layout after it.

The heights' arithmetic is in `egui_pop::model::sections`, without egui:
`fit` lays a stack out, `plan_drag` says what moving a boundary does
(its `DragMode` leaves room for other gestures).

## Behaviour

- **Fold.** A click on a band, or Space or Enter while it has the
  keyboard, folds or opens its section; Left folds it and Right opens
  it. A folded section keeps its band in place.
- **Heights.** A section's height is set by hand, in points, or sized by
  the list. Sections never set by hand share what the others leave,
  equally, and the last open section always takes what is left: it never
  keeps a height of its own. The bands of folded sections below the last
  open one therefore sit at the panel's bottom. Each body keeps its least
  height.
- **Resize.** The line above a band is a boundary when an open section
  lies just above it and another one below it. It follows the pointer in
  both directions, from the heights at the start of the drag, so it does
  not drift and gives back what it took when the pointer returns. Nothing
  gives way in proportion:
  - **Down:** the sections between the boundary and the last shrink, the
    nearest first, each down to its least; then the last does. The drag
    stops once the whole stack is at its least.
  - **Up:** the section above shrinks to its least, then those above it,
    the nearest first. The first open section below takes all the room;
    nothing else moves.
  - Folded sections are skipped. A section that gives way or moves gets
    a height of its own; one the list would not move stays in the list.
- **Keyboard.** A boundary takes the focus in reading order, after the
  section above it. Up and Down move it by a row, by the same rules;
  Enter drops the heights set by hand.
- **Reset.** A double click on a boundary drops the heights set by hand.
- **A panel too short.** Sections sized by the list give way first, down
  to their least; then those set by hand, from the bottom up. The last
  open section stays in view. The stored heights come back with the
  room; the next drag first aligns them on what is shown. When even the
  least heights do not fit, the whole stack scrolls, with a scrollbar
  shown then only.
- **Scroll.** Each body scrolls on its own.
- **Remembered.** Which sections are folded, and the heights set by hand,
  are kept in egui's persisted memory under the stack's name, so they
  come back with the window's layout. `Layout::load` and `Layout::store`
  read and write them whole, for an application that keeps them with its
  own layout (`Layout::height` and `set_height` for one section). A
  section's height is kept while it is folded, so it opens again at its
  size. A layout stored with the shares of an earlier version keeps its
  fold state, and its sections read as sized by the list.

## Classic conventions, and where this departs

Compared, 2026-10-27, with VS Code's side bar views (Explorer, Source
Control), JetBrains IDEs' tool windows, the macOS Finder sidebar,
Windows Explorer's navigation pane and Qt (`QToolBox`, `QSplitter`):

| | Established apps | Here |
|---|---|---|
| Fold | A click on the header (VS Code, Finder, `QToolBox`), Space or Enter on a focused header (VS Code) | The same |
| Folded header | Stays in place. VS Code pins the headers below the last open view to the bottom | The same as VS Code |
| Resize | A sash between two open views (VS Code), a splitter handle (`QSplitter`, JetBrains' split tool windows). Finder and Explorer do not resize sections: their sidebar scrolls as one | A boundary on the line above a band, as VS Code |
| Who gives way | VS Code's `SplitView.resize`: the views past the sash, the nearest first, each clamped in turn, to a hard stop. `QSplitter` the same by default | The same: the sections below going down, those above going up, and the first open section below takes the room going up |
| Keyboard on the sash | VS Code has none; WAI-ARIA's window splitter moves with the arrows and resets with Enter | The arrows and Enter, as WAI-ARIA |
| Left and Right on a header | VS Code folds and opens | The same |
| Panel too short | VS Code's split view scrolls as a whole, its scrollbar when needed. Qt clips | Scrolls as a whole, the scrollbar then only |
| Double click on the boundary | VS Code gives the views equal sizes again. `QSplitter` does nothing | The heights set by hand dropped: equal again, as VS Code |
| Double click on the header | JetBrains maximises a tool window. VS Code folds then opens again | Two clicks, as VS Code: no maximise |
| Remembered | Fold and sizes, per workspace (VS Code, JetBrains), and the folded sidebar sections (Finder) | Fold and heights set by hand, with the window's layout |
| Count | VS Code shows a badge on a view's header (Source Control's changes) | A pill after the title |
| Titles | VS Code: capitals, small and bold | The same by default (`capitals`) |

Departures, on purpose or not yet done:

- No fold animation (egui's `CollapsingHeader` has one): the stack is
  laid out in one pass, so a test sees the same frame each time.
- Sections are not reordered by dragging their bands (VS Code's views
  are). Sections cannot be maximised or moved out of the stack either.

Not yet: a drag and a double click on the title, as an option; Ctrl held
in a drag, pulling the sections below as a block; windowed rows in a
section, a body without its scroll area, and a wheel stopping at the end
of a section's scroll.

## The kit

| Component | Part | Handler |
|---|---|---|
| `section-band` | The band: `height` (28), `padding-x` (`space.l`) between its sides and its mark and count, `font-size` (`font.size.s`) at the section text role's weight, `align` `start`; `border-*` the lines above and under it (two stacked bands share one line). `hover` under the pointer, `active` while pressed, `focus` (a ring inside the band, and around a focused boundary) | `section`, or `plain` |
| `section-disclosure` | The mark: the `disclosure-open` or `disclosure-closed` glyph role in `text` (the accent), `width` across (10), in the band's state | `plain` (a transparent box) |
| `section-count` | The count: based on `indicator-badge` (its pill, height and tones), quieter at rest: the raised surface, a `border.default` outline, the default text | `pill`, or `box` |

The mark, the title and the count are `space.inline` apart.

## Named and observable

- A band is a button named by its title and its count ("Live, 3"),
  expanded or not.
- A boundary is a splitter named after the section above it ("Resize
  Live", the `egui-pop-section-resize` string), focusable, with its
  increment and decrement actions. Its value is the height of that
  section's body, between its least and how far the stack lets it grow.
- `sections::laid_out(ctx, name)` gives each section as drawn on the
  last pass: key, title, count, open, height, whether that height is set
  by hand, band and body rectangles. The debug bridge's `state` collector
  lists it as `core.sections.laid`, each section's height and whether it
  is set by hand in its summary.
- The gallery's `sections` page has its tests and snapshots: a stack
  sized by the list, one already pushed, and a panel too short.

## Survey: adopt, wrap or own

Checked 2026-10-27, on egui 0.36 and crates.io.

| Candidate | What it has | Why not |
|---|---|---|
| egui's `CollapsingHeader`, `CollapsingState` | Fold with an animation, the open state remembered per header, a custom header (`show_header`) | A body's height is its content's: nothing divides a panel between sections, nothing resizes them, and each header keeps its own record where the stack needs fold and shares in one |
| `egui_extras::StripBuilder` | Fixed, relative and remainder sizes in a row or a column | No fold and no drag |
| `egui::Resize` | One box resized from a corner | Not a stack |
| `egui_tiles` 0.18 | Linear containers with resizable shares, tabs, drag and drop | Its own tree and tab bars. A folded section is not a tile state, and the bands would still be drawn by hand |
| `egui_dock` 0.21 | Tabs and splits | No folding headers. It is a workbench, as egui-pop already uses it, not a panel's content |
| `egui_animation` | A collapse animation | Animation only |
| Styled kits (`functora-egui`, `egui-elegance`, `egui_sauge` and the like) | Accordions or collapsibles in their own style | Their own theme systems, not the kit; no resizable stack found |

**Verdict: own.** No crate stacks folding sections with resizable
boundaries, and the band must be drawn by the kit like every other
part. The widget is small, built on egui's `ScrollArea` and
interaction, with egui's persisted memory for the layout.
