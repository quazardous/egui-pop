# Collapsible sections

Status: fourth version (2026-10-30): titles that drag, the pull and empty
room, titles that stay, as options, after windowed rows and the wheel
contained, heights set by hand and the push.

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
  names always keep it). Two options, off by default: `grab_titles(true)`
  makes the titles drag too and `keep_titles(true)` leaves a title where it
  is when its section folds or opens.
- `Section::new(key, title)`: the key is what the layout remembers;
  `count(n)` puts a count on the band (`99+` past 99),
  `min_height(points)` its body's least height (two rows by default),
  `row_height(points)` its rows' height (the theme's by default,
  `sections::row_height`), `open(false)` folds it until the user opens it.
- Three kinds of body: `stack.section(section, |ui| ...)` draws anything
  in a scroll area, `stack.rows(section, count, |ui, range| ...)` draws
  windowed rows in one, and `stack.bare(section, |ui| ...)` leaves the
  scroll to an application's own view.
- `show` returns what the user did this pass (`toggled`, `resized`,
  `reset`) and the layout after it.

The heights' arithmetic is in `egui_pop::model::sections`, without egui:
`fit` lays a stack out, `plan_drag` says what moving a boundary does
(`DragMode::Push` or `Pull`), `keep_title` what folding keeps.

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
  lies above it and another one below it. It resizes the first open
  section above, past any folded ones: the line under a folded section
  and the line above it do the same, and the folded bands between follow.
  It follows the pointer in both directions, from the heights at the
  start of the drag, so it does not drift and gives back what it took
  when the pointer returns. Nothing gives way in proportion:
  - **Down:** the sections between the boundary and the last shrink, the
    nearest first, each down to its least; then the last does. The drag
    stops once the whole stack is at its least.
  - **Up:** the section above shrinks to its least, then those above it,
    the nearest first. The first open section below takes all the room;
    nothing else moves.
  - Folded sections are skipped. A section that gives way or moves gets
    a height of its own; one the list would not move stays in the list.
- **Keyboard.** A boundary takes the focus in reading order, after the
  open section above it; the lines under folded sections do not, as they
  would be a second stop for the same pair. Up and Down move it by a
  row, by the same rules; Enter drops the heights set by hand. With the
  keys of the pull held, they pull.
- **Reset.** A double click on a boundary drops the heights set by hand
  and any empty room.
- **The pull.** With the keys of `gesture.sections.pull` held (Ctrl with a
  drag by default, Cmd on macOS, in the pointer profile; rebindable in the
  shortcuts editor,
  [pointer-gestures.md](pointer-gestures.md)), a drag moves the sections
  below as a block, their heights kept; only the section above changes:
  - **Up:** it shrinks to its least, then those above it, the nearest first.
    The block follows, and empty room opens under the last section.
  - **Down:** the block goes into the empty room, then the last section
    alone gives way, down to its least, and the drag stops.
  - The keys pressed or released during a drag change the mode from there:
    the drag starts again from the stack as shown, and nothing jumps.
- **Empty room.** A stack with empty room under its last section keeps it,
  in `Layout::empty`, until a drag takes it back or a double click drops it.
  A plain drag takes it after the sections between, as a pull takes it
  first. In a panel too short, it gives way after the sections sized by the
  list and before those set by hand, and comes back with the room. A line
  with only folded sections under it is a boundary while there is empty
  room: the bands follow, and up it opens more.
- **Titles that drag** (`grab_titles(true)`). A press on a title that moves
  is a drag, by the rules above, the title staying under the pointer; one
  that does not is a fold, anywhere on the band: the line above it takes
  only the section's side of the border, and under a folded section, whose
  title does the same, there is no line. A title under a folded section
  resizes the first open one above, as the folded title does; with nothing
  open above it does not move. The keyboard does not change.
- **A double click on a title** drops the heights set by hand and any empty
  room, with the option. Its first click has folded the section, which is
  seen folded until the second: nothing can tell a first click from a single
  one, and holding each fold back for the 0.3 s a second could take would
  slow every fold. What holds is that the second lands on the title even if
  the fold took it away (what moved under the pointer does not get the
  click), that the fold ends as it was, and that the shares come back.
- **Titles that stay** (`keep_titles(true)`). Folding or opening a section
  leaves its title where it is, whatever does it, the keyboard included:
  - **Folding:** the open sections above keep their shown height, as if set
    by hand. Those below move up; with nothing open below, the room stays
    empty. A second click at the same place lands on the same title. The
    trade-off: those above no longer get the room a fold frees.
  - **Opening:** the section takes its room below its title, from the
    sections below, then from the empty room (with nothing open below, all
    of it, or its own height when it has one). In a panel with no room for
    it, those above still give way, and the title moves.
  - **The shares back:** a double click.
- **A panel too short.** Sections sized by the list give way first, down
  to their least; then those set by hand, from the bottom up. The last
  open section stays in view. The stored heights come back with the
  room; the next drag first aligns them on what is shown. When even the
  least heights do not fit, the whole stack scrolls, with a scrollbar
  shown then only, thin and quiet at rest whatever the theme does for other
  bars (`ScrollBarKit::seen_at_rest`).
- **Scroll.** Each body scrolls on its own.
- **Windowed rows.** `stack.rows` goes through egui's
  `ScrollArea::show_rows`: its closure is given the rows in view and one
  past them, and draws those alone, each exactly the row's height, with no
  spacing between them, as a list's. The
  window is worked out again on every pass, so it follows the section's
  size, its scroll and its count, with no blank row left: when the rows
  under a scroll shrink, the stack asks for a pass by itself. Nothing is
  kept around the window, as a row is cheap to draw on the pass it comes
  into view.
- **Least height with rows.** Two rows, or all of them when there are
  fewer (none: a band alone). `min_height` still wins.
- **A body without scroll area.** `stack.bare` gives an application's own
  scrolling view the whole body, clipped to it, as `views::List`, which
  in a scroll area would nest two scrolls.
- **The wheel.** A wheel pushing past the end of a body's scroll stops
  there: neither the stack nor anything around it moves. So does one over
  a bare body, always: the stack cannot see whether the view inside has
  anywhere to go. Over a band, or a body with nothing to scroll, it
  scrolls a stack too tall, which would otherwise have nowhere to take a
  wheel; a stack with nowhere to go leaves it to what is around it, as
  that is no section's scroll. The contract is that nothing moves, not
  that nothing is drawn: eframe repaints on every wheel event
  (`egui-winit` answers each with `repaint: true`), so a widget cannot
  promise a pass is saved.
- **Remembered.** Which sections are folded, and the heights set by hand,
  are kept in egui's persisted memory under the stack's name, so they
  come back with the window's layout. `Layout::load` and `Layout::store`
  read and write them whole, for an application that keeps them with its
  own layout (`Layout::height` and `set_height` for one section). A
  section's height is kept while it is folded, so it opens again at its
  size. A layout stored with the shares of an earlier version keeps its
  fold state, and its sections read as sized by the list. `Layout::empty`
  is the empty room kept, none in a layout stored before it.

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
| Double click on the header | JetBrains maximises a tool window. VS Code folds then opens again | Two clicks, as VS Code: no maximise. With `grab_titles`, the heights set by hand dropped too, a departure |
| Drag on the header | VS Code's headers drag to reorder views. No app surveyed resizes from one | Nothing by default. With `grab_titles`, it resizes, a departure, and closes off reordering for that stack |
| A modifier held on the sash | VS Code's Alt resizes both sides symmetrically. No equivalent of a block found | `gesture.sections.pull`, a departure: the sections below as a block |
| Empty room under the last view | VS Code never leaves any (`distributeEmptySpace`) | Only after a pull or with `keep_titles`, a departure, until a double click |
| A title when its section folds | VS Code's panes divide the room again, and the title moves | The same by default. With `keep_titles`, it stays, a departure |
| Wheel at the end of a view | VS Code's lists keep it by default, even with nothing to scroll (`alwaysConsumeMouseWheel` in `listView.ts`). The web hands it to the page unless `overscroll-behavior: contain` | Kept, as VS Code, by a body with a scroll to end. A body with nothing to scroll hands it on, so that a stack too tall still takes a wheel over short sections |
| Remembered | Fold and sizes, per workspace (VS Code, JetBrains), and the folded sidebar sections (Finder) | Fold and heights set by hand, with the window's layout |
| Count | VS Code shows a badge on a view's header (Source Control's changes) | A pill after the title |
| Titles | VS Code: capitals, small and bold | The same by default (`capitals`) |

Departures, on purpose or not yet done:

- No fold animation (egui's `CollapsingHeader` has one): the stack is
  laid out in one pass, so a test sees the same frame each time.
- Sections are not reordered by dragging their bands (VS Code's views
  are). Sections cannot be maximised or moved out of the stack either.
- `grab_titles`, `keep_titles` and the pull are not any app's: they come from
  an application's demand, so the two options stay off by default, and the
  pull needs its keys held. The table's rows say what each departs from.

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
- A boundary is a splitter named after the section it resizes ("Resize
  Live", the `egui-pop-section-resize` string), with its increment and
  decrement actions; the one above a folded band takes the focus, and
  the lines under folded bands carry the same name. Its value is the height of that
  section's body, between its least and how far the stack lets it grow.
- `sections::laid_out(ctx, name)` gives each section as drawn on the
  last pass: key, title, count, open, height, whether that height is set
  by hand, band and body rectangles, how far its body is scrolled,
  when windowed the rows drawn, the empty room under it (the last section's)
  and how a drag under way resizes it (`Push` or `Pull`). The debug bridge's
  `state` collector lists it as `core.sections.laid`, each section's height,
  whether it is set by hand, its window and its drag (`dragged`, `pulled`)
  in its summary, and the empty room at its end.
- A title that drags stays a button named as it was. The pull's keys are not
  announced on it; the arrows on a boundary do the same as a pull with them.
- The gallery's `sections` page has its tests and snapshots: a stack
  sized by the list, one already pushed, a panel too short, a stack
  of a long windowed section over a `views::List`, and a stack with both
  options on, which the playground turns off.

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
