# Collapsible sections

Status: first version (2026-10-27).

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

## Behaviour

- **Fold.** A click on a band, or Space or Enter while it has the
  keyboard, folds or opens its section. A folded section keeps its band
  in place.
- **Heights.** Open sections divide the panel's height by their shares
  (equal at first). The bands of folded sections below the last open
  one therefore sit at the panel's bottom. Each body keeps its least
  height. If the panel is too short for all of them, the stack
  overflows and the panel clips it.
- **Resize.** The line above a band is a boundary when an open section
  lies above it and another one below it. Dragging it resizes the
  section above against the next open section below. A double click on
  it gives every section its share back.
- **Scroll.** Each body scrolls on its own.
- **Remembered.** Which sections are folded, and the shares, are kept
  in egui's persisted memory under the stack's name, so they come back
  with the window's layout. `Layout::load` and `Layout::store` read and
  write them whole, for an application that keeps them with its own
  layout. A section's share is kept while it is folded, so it opens
  again at its size.

## Classic conventions, and where this departs

Compared, 2026-10-27, with VS Code's side bar views (Explorer, Source
Control), JetBrains IDEs' tool windows, the macOS Finder sidebar,
Windows Explorer's navigation pane and Qt (`QToolBox`, `QSplitter`):

| | Established apps | Here |
|---|---|---|
| Fold | A click on the header (VS Code, Finder, `QToolBox`), Space or Enter on a focused header (VS Code) | The same |
| Folded header | Stays in place. VS Code pins the headers below the last open view to the bottom | The same as VS Code |
| Resize | A sash between two open views (VS Code), a splitter handle (`QSplitter`, JetBrains' split tool windows). Finder and Explorer do not resize sections: their sidebar scrolls as one | A boundary on the line above a band, as VS Code |
| Double click on the boundary | VS Code gives the views equal sizes again. `QSplitter` does nothing | Equal shares again, as VS Code |
| Double click on the header | JetBrains maximises a tool window. VS Code folds then opens again | Two clicks, as VS Code: no maximise |
| Remembered | Fold and sizes, per workspace (VS Code, JetBrains), and the folded sidebar sections (Finder) | Fold and shares, with the window's layout |
| Count | VS Code shows a badge on a view's header (Source Control's changes) | A pill after the title |
| Titles | VS Code: capitals, small and bold | The same by default (`capitals`) |

Departures, on purpose or not yet done:

- No fold animation (egui's `CollapsingHeader` has one): the stack is
  laid out in one pass, so a test sees the same frame each time.
- Left and Right do not fold and open a focused band (VS Code's
  headers do). A boundary takes no keyboard focus, so a resize is done
  with the pointer only.
- Sections are not reordered by dragging their bands (VS Code's views
  are). Sections cannot be maximised or moved out of the stack either.

## The kit

| Component | Part | Handler |
|---|---|---|
| `section-band` | The band: `height` (28), `padding-x` (`space.l`) between its sides and its mark and count, `font-size` (`font.size.s`) at the section text role's weight, `align` `start`; `border-*` the lines above and under it (two stacked bands share one line). `hover` under the pointer, `active` while pressed, `focus` (a ring inside the band) | `section`, or `plain` |
| `section-disclosure` | The mark: the `disclosure-open` or `disclosure-closed` glyph role in `text` (the accent), `width` across (10), in the band's state | `plain` (a transparent box) |
| `section-count` | The count: based on `indicator-badge` (its pill, height and tones), quieter at rest: the raised surface, a `border.default` outline, the default text | `pill`, or `box` |

The mark, the title and the count are `space.inline` apart.

## Named and observable

- A band is a button named by its title and its count ("Live, 3"),
  expanded or not.
- A boundary is a splitter named after the section above it ("Resize
  Live", the `egui-pop-section-resize` string). Its value is the height
  of that section's body.
- `sections::laid_out(ctx, name)` gives each section as drawn on the
  last pass: key, title, count, open, band and body rectangles. The
  debug bridge's `state` collector lists it as `core.sections.laid`.
- The gallery's `sections` page has its tests and snapshots.

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
