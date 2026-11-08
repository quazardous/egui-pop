# The workbench

What an application's window holds around its content: its own frame,
the scale of each window, panels in areas, slim strips, panels detached
into windows of their own, named arrangements, and the keymap changed
at run time. Every change goes through a step (ADR 0011): a plugin may
watch it on the journal, veto or redirect it, replace it or follow it.

## Panels in areas

A plugin declares panels with their rules; the workbench places them in
areas (the centre, the start and end side bars, the bottom panel), as
tabs, rows, columns and grids over [egui_stack](https://github.com/quazardous/egui_stack)
(the continuation of egui_dock),
saves where the user put them and repairs that layout at the next start.
The shell's side bar, main and inspector zones are panels of their areas.

**One dock space.** The areas are the named docks of one egui_stack
`DockSpace` (`center`, `start_side`, `end_side`, `bottom`). The
workbench lays the areas out (a side area's width, its fold and flyout,
an area lying over the centre, the bottom panel's alignment) with
egui_stack's layout layer: each area is a zone holding its dock, with
its least, largest and default sizes as constraints, the centre the zone
that grows, a folded side area the zone's strip; the zones that hold no
dock refuse a dragged tab (`DockSpace::bind`). It draws each area's dock
where the layout put the area; the docks share one drag, so a
tab goes from any area to any other as it goes inside one. Each area's
rules are its dock's: any split, as inside one area; never one of
egui_stack's floating windows (a panel leaves the main window through
Detach, into a window of its own); kept when it is empty, so a closed
area stays a drop target. The layout document keeps one tree per area,
as before the space: a layout saved before reads as it did, each area's
tree becoming its dock's.

**The starting layout.** Without a saved layout, every panel starts in
its default area. `AppBuilder::layout` gives the layout to start with
instead (the panels' areas, tabs and splits, the closed panels, the
folded areas), repaired against the declared panels as a saved layout
is; the `Default` arrangement resets to it. An application that does not
use the shell's zone panels lists them as closed:

```rust
use egui_pop::api::workbench::{PANEL_INSPECTOR, PANEL_MAIN, PANEL_SIDE_BAR};
use egui_pop::model::ids::PanelId;
use egui_pop::model::layout::{Layout, Node};
use egui_pop::model::panels::Area;
use egui_pop::prelude::*;

const TREE: PanelId = PanelId::from_static("demo.tree");
const VIEW: PanelId = PanelId::from_static("demo.view");

let builder = App::builder()
    .layout(Layout {
        areas: vec![
            (Area::StartSide, Node::tab(TREE)),
            (Area::Center, Node::tab(VIEW)),
        ],
        hidden: vec![PANEL_SIDE_BAR, PANEL_MAIN, PANEL_INSPECTOR],
        ..Layout::default()
    })
    // Every area runs to the window's edges, a line between them.
    .frameless(&Area::ALL);
```

**Frameless areas.** An area has a margin around it, and each of its
panels' bodies a margin and a rounded border (the kit's `dock-body`).
`AppBuilder::frameless` takes both away for the areas it names, so their
panels run to the window's edges and are set apart by the separator's
line only (a viewer whose views draw to their edges); a panel then gives
itself the margin it wants. `main_edge_to_edge` takes away only the
centre's outer margin.

**Lines between panels.** Two panels of one area meet at the dock's
separator (the kit's `dock-separator`); two areas meet at the layout's
line between them, which the kit draws (as wide as the widgets' border,
shown at rest only when `side-area` has a border). It lies inside the
side or bottom area, on its edge facing the centre, in the room the
area's frame keeps for it, where egui drew a side panel's separator
(`Kit::paint_area_line`); it takes the drag that resizes the area. Those are the lines that mean something: the bodies' borders
beside them only repeat them, and a kit that wants one line where two
panels meet sets `dock-body.rest.border-width` to 0, as the Rerun-like
kit does (it also hides the dock's separator at rest).

**Bare panels.** A shell panel is declared `bare` (`PanelSpec::bare`): alone in
its area it is drawn plainly, with no tab bar and no scroll area from anyone. What
its blocks draw past its end is cut, and nothing says so. Beside another panel the
same content is in a tab, and egui_stack gives a tab's body a scroll area on both
axes. So a block in `SIDE_BAR`, `MAIN` or `INSPECTOR` that may run long brings its
own scroll area, and one sized to what it is given stays inside it: a viewport
filling `MAIN` must not scroll. The gallery shows both: its pages get a scroll area
each, but for those filling the zone themselves (`shell::FILLS_THE_ZONE`).

![The gallery's Workbench page: Outline beside the side bar, Properties
beside the inspector, Console and Notes in the bottom
panel](screenshots/workbench-pop-dark.png)

```rust
use egui_pop::api::steps::Before;
use egui_pop::api::workbench::STEP_MOVE;
use egui_pop::egui;
use egui_pop::model::ids::PanelId;
use egui_pop::model::panels::{Area, PanelSpec};
use egui_pop::prelude::*;

const LOG: PanelId = PanelId::from_static("demo.log");
const TREE: PanelId = PanelId::from_static("demo.tree");

struct Demo;

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        // The log stays in the bottom panel; the tree may go anywhere.
        ctx.panel(
            PanelSpec::new(LOG, "Log", Area::Bottom)
                .only_in(&[Area::Bottom])
                .min_size(80.0),
            |ui, _| {
                ui.label("log");
            },
        )?
        .panel(
            PanelSpec::new(TREE, "Tree", Area::StartSide).default_size(220.0),
            |ui, _| {
                ui.label("tree");
            },
        )?;
        // A veto: the tree never goes to the bottom panel.
        ctx.before(STEP_MOVE, |_, args| {
            if args["panel"] == "demo.tree" && args["area"] == "bottom" {
                Before::Veto("the tree stays on a side".into())
            } else {
                Before::Go
            }
        });
        Ok(())
    }
}

let app = App::builder().plugin(Demo).build(&egui::Context::default()).unwrap();
let h = app.handle();
assert_eq!(h.layout().area_of(&LOG), Some(Area::Bottom));
// Refused by the panel's rules, then by the plugin, then done.
assert!(!h.move_panel(&LOG, Area::StartSide).unwrap().ok());
assert!(h.move_panel(&TREE, Area::Bottom).unwrap().vetoed().is_some());
assert!(h.move_panel(&TREE, Area::EndSide).unwrap().ok());
assert_eq!(h.layout().area_of(&TREE), Some(Area::EndSide));
```

A side area is never narrower than the largest `PanelSpec::min_size` of
its panels. The shell's own panels (`PANEL_SIDE_BAR`, 120 points at
least; `PANEL_INSPECTOR`, 160) take the application's sizes:
`AppBuilder::shell_panel_size` the width they start at,
`shell_panel_min_size` and `shell_panel_max_size` their least and
largest, in points or as a share of the window
(`workbench::ShellSize::Share(0.6)`). While the application runs,
`AppHandle::set_shell_panel_min_size` and `set_shell_panel_max_size`
change them (`None` puts back the default): a least width measured from
the panel's content, which depends on the fonts and the text size, is
set there, each frame or when it changes, and the area widens to it on
the next pass. `AppBuilder::centre_min_size`
keeps the centre at least that wide: the side areas give way to it,
each down to its own least, and a window too narrow for all of them
squeezes the centre.

Each panel gets `<id>.focus` and `<id>.toggle` commands. `AppHandle::focused_panel`,
the context key `focusedPanel`, is the panel the keys obey; see
[The focused panel](#the-focused-panel). The layout
changes through the steps `workbench.move`, `workbench.show`,
`workbench.hide`, `workbench.focus`, `workbench.fold`,
`workbench.place`, `workbench.flyout`, `workbench.flyout_size`,
`workbench.arrange` and `workbench.zen` (`egui_pop::api::workbench`).

**Drop zones.** A tab dragged inside an area shows where it would go on
the group under the pointer itself, with no buttons: near an edge (in
egui_stack's edge band), the half of the group a split would give it,
bordered and tinted; elsewhere the whole group, to join its tabs; over a
tab bar, the place among its tabs. The zone is the kit's `dock-preview`
(its `rest` border and fill, the fill at its `opacity`), so it follows
the theme like the hint between areas; the same tokens feed egui_stack's
`OverlayStyle::zone_fill`, `zone_stroke` and `zone_corner_radius`. A tab
dropped outside every area stays where it was. A drop on the edge of a
group in a row (a column) joins that row (that column) rather than
nesting a split, and a drop beside a group in a grid joins the grid.

**Rows, columns and grids.** An area holds groups of tabs in rows,
columns and grids, nested. "Arrange as grid" (`workbench.arrange.grid`)
lays out the row, column or grid holding the focused panel's group in
rows and columns, row by row: as many columns as the square root of
their number, rounded up, or `columns` from the command's arguments;
"Arrange as row" and "Arrange as column" (`workbench.arrange.row`,
`workbench.arrange.column`) lay them out again side by side or one above
the other. Each takes a `panel` argument for another group than the
focused panel's, an agent's way in; a group alone in its area has
nothing to arrange (`{ "arranged": false }`). They go through the
`workbench.arrange` step (`panel`, `as`: `grid`, `row` or `column`,
`columns`), `AppHandle::arrange` from code. The layout document saves a
grid as a node of kind `grid` (`columns`, `col_shares`, `row_shares`,
`children`); a row or column of more than two as splits nested on their
second side, each `ratio` the share of its first side. Layouts saved
before grids read as they did.

**Between areas.** A tab dragged onto another area gets the same zones
there, with the same preview: the middle of a group joins its tabs, an
edge splits the group on that side, a tab bar takes it between two tabs.
An area its panel stays out of (`PanelSpec::only_in`, or content the
area cannot host) refuses it: the zone shows disabled, with the reason
("Log stays out of the start side bar"), and a drop there changes
nothing. A closed side or bottom area is offered along the areas' edge
(a band of 48 points), an area drawn bare (a zone alone, without a tab
bar) over its whole rect: dropped there, the tab joins its tabs. Once
the tab is dropped in another area, the move goes through the
`workbench.move` step (`panel`, `area`): a plugin's veto puts the tab
back where it was, a redirect sends it to the area the step names. A
palette dragged over an area it docks in shows the hint with a ghost of
its tab (the kit's `tab-preview`) after the area's first tabs.

The hint is published like an overlay: `AppHandle::drop_hint` (panel,
area, rect, refusal, `zone`, the egui_stack `DropZone` (`Center` to join
tabs, an edge to split), `index` among the tabs it would join, and for a
palette `preview`, its ghost's area, index and rect), `overlay_rects`
(kind `DropHint`, named `drop:<panel>:<area>`), and the debug bridge's
`workbench` collector (`drop_hint: {name, panel, area, rect, refusal,
zone, index, preview}`), which also shows the layout, each area's rect,
the focused panel and the tab being dragged.

### The focused panel

There is one: the panel the widget focus is in, or the last one it was in.
`when` clauses reading `focusedPanel` follow the keyboard.

| What happens | `focusedPanel` | The widget focus |
|---|---|---|
| Tab or a widget takes the focus, in a panel | that panel | that widget |
| `workbench.focus`, `<panel>.focus`, `focus_panel` | that panel, at once | in the next pass, see below |
| F6 or Shift+F6 ([regions](inputs.md#f6-through-the-regions)) | the panel it lands in | the region's last, or as below |
| Ctrl+Page Down or Up (`workbench.tab.next`, `.previous`) | the tab beside it in its group, wrapping | as below |
| A press in a panel | that panel | as the widget pressed decides |
| The focus goes to the menu, the status bar or a dock's tab | unchanged | there |
| Escape, or the focused widget going away | unchanged | none |
| A panel closes while focused | none, or back from a palette | none |

**Handing a panel the keyboard.** It shows the panel, then gives the focus to
the widget the panel named with `egui_pop::core::focus::prefer`; without one, to
its first focusable widget; without any, to the panel itself. If the focus is
in the panel already it stays where it is. Behind a modal, or when a command
from an agent, a script or a device finds someone typing in a field, only
`focusedPanel` moves. The focus lands a pass later, so the key that ran the command is not
typed into the field it lands on.

- **A canvas** is a widget like another: one that senses clicks or drags is
  focusable, and a panel whose toolbar comes first says `focus::prefer(ui.ctx(),
  response.id)` so the canvas gets the keyboard. The image viewport does, as do
  the library's grid (over its filters) and a text editor (over its find bar).
  A panel with a search field first and a list below should do the same.
- **A panel with nothing focusable** holds the focus itself: a stop named after
  the panel, drawn only while it holds the focus, so it is no extra Tab stop.
  Tab goes on from there. It shows nothing yet ([item 4 of the keyboard
  survey](keyboard-survey.md)).

**The last panel stays** when the focus goes to something no panel holds. In
egui a menu's items take the focus and nothing gives it back when it closes, and
Escape on any widget leaves nothing focused: if `focusedPanel` emptied, opening
a menu or pressing Escape would turn off every key of the panel worked in, and
a command in that menu would act on nothing. VS Code has both kinds of keys:
`focusedView`, `sideBarFocus` and `panelFocus` empty on blur, `activeEditor`,
`activeViewlet` and `activePanel` stay. `focusedPanel` is of the second kind.
Whether a widget holds the focus at all is `widgetFocus`.

**Palettes and flyouts only borrow the keys.** A press on one leaves them where
they were: a click on a palette's background keeps the keys on the image, as in
an image editor. A widget of it taking the focus (a press on a field, Tab, its
`.focus` command) does move them, and when it lets go (Escape, a click
elsewhere, the palette closing) they go back to the panel of the layout that
had them.

**Detached windows.** egui keeps a focus per window. A widget taking the focus
in a window, a press in it or the window getting the system's focus back moves
`focusedPanel`; `<panel>.focus` raises the panel's window.

Following is a plain write, not a step: it cannot loop back through
`workbench.focus`. Each change, from either side, is published once as the event
`workbench.focused` (`{ "panel" }`).

A side area lies **beside** the centre, taking its room, or **over**
it (`workbench.place`, `{ "area", "placement": "over" }`;
`AppHandle::place_area`, `AppBuilder::over` for the first start): its
folded strip keeps the room in the layout, so the centre keeps its
width (a terminal in it is not resized), and the area, unfolded, lies
above the centre next to its strip, with the kit's `side-area` shadow
(`shadow.side`). A click on the strip's grip folds or unfolds it; its edge on
the centre's side resizes it. The placement is saved with the layout
(`over`), like folding; a plugin may veto the step. The centre never
folds: `workbench.fold` refuses it.

The line between a side or bottom area lying beside the centre and the
centre resizes the area: by a drag, or with the keyboard once Tab or
Shift+Tab reaches it ("Resize the start side bar", a splitter to assistive
technology). An arrow across the line moves it 16 points, with Shift 64,
and Enter or a double click gives the area its default size back, as a
dock separator and a section's boundary do. The sizes the user gave are
kept with egui's memory, as egui's side panels kept theirs; a session
saved when the areas were egui panels keeps its areas' sizes.

### Strip icons and flyouts

A side area's strip (folded, or lying over the centre) shows one icon
per panel it holds, in tab order: the panel's `PanelSpec::icon`, a
vector icon tinted by the kit's `activity-bar-item` and crisp at any
scale, or its title's first letter when it has none. The strip widens
to hold them. An icon's tooltip gives the panel's title and its key
(the first binding of `<panel>.focus`, else of `<panel>.toggle`), and
the icon is a button named by the title.

An application keeps a side area's strip bare with
`AppBuilder::bare_strips(&[Area::StartSide])`: its grip glyph alone,
`area-strip.width` wide, no icon per panel, and a click unfolding the
area as before (`AppHandle::bare_strip` says which areas are bare). It
is the application's choice, as `AppBuilder::frameless` is, not the
theme's: whether a folded area lists its panels is part of how the
application is laid out, and the same theme serves applications with
and without the icons.

On a folded area, a click on an icon opens that panel alone as a
**flyout** over the centre, next to the strip, without moving the
layout (`workbench.flyout`, `{ "panel", "open" }`;
`AppHandle::open_flyout`). Escape or a click away closes it, the focus
going back where it was: it is an overlay of the stack
(`egui_pop::core::overlay::hold`), above which menus and dialogs open
as usual. A combo box's list or a context menu opened in the panel is
part of the flyout: a click in it does not close the flyout, and Escape
closes it first. A click on another icon switches to that panel. A double
click on the icon, or the pin button in the flyout's band, unfolds the
area with the panel shown. The flyout's free edge resizes it; the
width is kept per panel and saved with the layout (`flyouts`,
`workbench.flyout_size`). Only a folded side area's panels fly out;
unfolding the area closes the flyout. On an area lying over the
centre, open, a click on an icon shows its panel there.

A panel publishes an indicator through the status bar's model,
speaking for it with `Indicator::panel`: while the panel is not shown
(its area folded, or another tab in front), its icon carries the state
as a micro-indicator: a count as a badge, else its errors or warnings,
else a dot in its tone, and `detail` joins the icon's tooltip. Such an
indicator is not a status bar item.

```rust
use egui_pop::api::strips::{Indicator, Tone};
use egui_pop::model::ids::PanelId;

const SEARCH: PanelId = PanelId::from_static("app.search");

// Hits waiting in the search panel, on its icon while it is folded.
let hits = Indicator::new("search.hits", "No hits", "search.")
    .panel(&SEARCH)
    .reduce(|state, event| {
        let n = event.data["hits"].as_u64().unwrap_or(0);
        state.count = Some(n);
        state.text = format!("{n} hits");
        state.tone = if n == 0 { Tone::Neutral } else { Tone::Accent };
    });
# let _ = hits;
```

Agents read the strips in the `workbench` collector (`strips`: each
icon's area, panel, title, icon, key, whether its panel shows, and its
indicator's state with `marked` when the icon shows it; `flyout`: the
panel and its rect) or with `workbench::strip`; the mark itself is in
the `indicators` collector as `strip:<indicator id>`.

Plugins declare panels too: egui-pop's jobs centre (`egui_pop.jobs`)
and devtools are panels like the application's.
`AppHandle::declared_panels` lists every declared panel with the plugin
that declared it (its `OwnerId`), so a host that checks layouts or
templates against its own panels can accept those of the plugins it
connected, and a template may place the jobs centre.

## Auxiliary windows

A detachable panel moves to an OS window of its own, with its own scale
and the application's frame when it draws one
(`AppHandle::detach_panel`, `attach_panel`; the tab menu offers
Detach). Closing that window is the `workbench.close_window` step: its
default docks the panel back, a veto keeps the window open. Its place is
kept in the layout and repaired before the window opens. The window is
titled with the panel and the application's name (`AppBuilder::name`,
"Filmstrip — Helios"), not the main window's whole title.

**Tabs in a detached window.** Other panels may join a detached window
as tabs: it shows them in one tab bar (no splits) and is titled after
the tab shown. A panel joins it:

- **by a drag**: while a tab is dragged, a detached window under the
  pointer shows the drop zone the areas show (the kit's `dock-preview`,
  egui_stack's `DropZone::at` over the window's body), disabled with its
  reason for a panel that may not leave the main window; let go there,
  the tab joins the window's tabs. egui keeps each window's input apart
  and the system hands a drag to the window it began in, so the window
  sees the pointer only through the main one: a window embedded in the
  main one (kittest, a backend without windows) always does, an OS
  window does when both windows know where they are on the screen
  (X11, Windows, macOS), not on Wayland, which hides window positions;
- **from the tab's menu**, "Move to window", which lists the detached
  windows by their panels and works everywhere; an agent runs the
  `workbench.move_to_window` command (`panel`, the focused one by
  default; `window`, a panel of that window), `AppHandle::move_to_window`
  from code.

Each way is one `workbench.join_window` step (`panel`, `window`). Its
tab's menu moves a panel out again ("Move to …" docks it, Detach gives
it a window of its own); closing the window docks every panel it holds.
The layout document keeps the first panel under `panel` and the others
under `tabs`, the shown one as `shown`: an application that predates
tabs in windows opens the first panel there and docks the others in
their default areas.

Such a window is a window of the application in full:

- **Keys.** The keymap reaches it: the key dispatcher runs in each
  window's pass, in that window's context. Its focus chain is its own
  (egui keeps a focus per window), so an editor's find or save fires in
  the window where the editor has the keyboard, and the `window`
  context key holds the detached panel's id there (empty in the main
  window): `when: "window == 'app.log'"` binds a key in that window
  only.
- **Dialogs.** A dialog comes up over the window the user worked in
  last (`AppHandle::active_window`: focused, clicked or typed in), so a
  dialog opened from a detached panel's window is shown there. While it
  is open, every other window is blocked: its keys are dropped and a
  backdrop covers it; a click there brings the dialog's window to the
  front. A window closed with a dialog open gives it to the main window.
  Each window has its own overlay stack (`egui_pop_core::overlay`);
  blocks of the `WINDOW_OVERLAYS` zone are drawn in every window, each
  drawing what belongs to it.
- **Menus, the palette and toasts.** As in VS Code's auxiliary windows,
  they open in the window in use: the command palette and a popup menu
  (`menus::popup_at`) open over the window the user worked in when they
  opened, and a context menu over the widget it belongs to; the toasts
  show in the active window. Each goes to the main window when its
  window closes. The palette, a modal overlay, blocks the other windows
  as a dialog does. The settings window and the in-app file chooser
  open in the main window.

The bottom panel lies under the editors by default, between the side
bars, which go down to the bottom, as VS Code has it. The
`workbench.panel_alignment` setting lines it up otherwise, as VS Code's
panel alignment does: `justify` (the window's whole width, under both
side bars), `start` (under the start side bar) or `end` (under the end
one); `AppBuilder::panel_alignment` gives the default.

## The menu bar

The shell draws the main menu (`app.main`) as a menu bar. When the
window is too narrow for all of it, the menus that fit stay in the bar,
from the start, and the rest become submenus of a "…" entry (the
`more` icon, named "More menus"), keeping their titles, items and
`when` clauses. When no menu fits besides "…", the whole menu is one
entry (the `menu` icon, named "Main menu"). The blocks of the
`app.top-bar` zone keep the width they took, after the menus. At the
bar's end, the commands of `MENU_BAR_END` show as icon buttons, and the
blocks of the `app.top-bar-end` zone (`zones::TOP_BAR_END`) before them,
left to right: a document's name, a readout, any widget of the
application's. They too keep the width they took; the menus overflow
first. Without the menu bar (`AppBuilder::menu_bar(false)`), the top bar
still shows when either zone has blocks. Each
label is measured once, and again only when the language, the kit or
the scale changes.

Each menu has an access key: the first letter of its label not taken
by a menu before it (File, Edit, View, Help: F, E, V, H). Alt and that
letter open the menu, in the bar or inside "…" with the menu's
submenu open; a key binding on the same keys wins. The accessibility
tree gives each menu its access key.

`menus::bar_layout(app, &MENU_MAIN)` says where each menu went last,
and the `menu_bar` debug collector shows it: `bar`, `overflow`,
`collapsed` and `keys`. `menus::bar_within` draws any declared menu as
such a bar, keeping room at its end.

## Zen mode

`workbench.zen` (View > Zen mode) toggles a view of the centre alone:
the title bar, the menu and status bars, the activity bar, the side and
bottom areas and the floating palettes are hidden, and the frame
publishes no caption. `Shift+F11` toggles it (Sublime Text's
distraction free mode; VS Code's `Ctrl+K Z` is left out, since it would
make `Ctrl+K` a prefix everywhere while it deletes to the line's end in
a text field). An application binds its own key in its defaults, and
takes the default away below them:

```rust,ignore
ctx.keybinding("Ctrl+K Z", workbench::CMD_ZEN, "", Layer::Default)?
    .unbind("Shift+F11", Layer::Workspace)?;
```

Escape leaves it (`workbench.leave_zen`, while no
overlay is open, no mode is pushed and no text field has the
keyboard; in a text field, the first Escape leaves the field and the
second leaves zen mode). The way out stays in sight: on entering, a
strip at the top centre (the kit's `fullscreen-exit` component) says
"Zen mode: Escape to leave" (the key bound to `workbench.leave_zen`)
next to a "Leave zen mode" button, for three
seconds, then fades over `motion.duration` (at once under reduced
motion); screen readers hear it once. Afterwards, the pointer at the top
edge brings the strip back, as the menu bar of a full screen window on
macOS, until it leaves the strip. The command palette (`Ctrl+Shift+P`)
still opens in zen mode and offers "Leave zen mode". In full screen,
zen's strip stands in for full screen's own. The change is the
`workbench.zen` step (`{ "zen": bool }`,
`AppHandle::set_zen`, `AppHandle::zen`, the `workbenchZen` context
key): a plugin may veto it, and the journal records it. Zen mode is a
state of the session, not of the layout: it is never saved, and the
next start shows the whole workbench. Combined with full screen, it
leaves the content alone on the screen.

## The layout controls

VS Code's layout controls, the rule here (`egui_pop::api::layout_controls`):

- **The area toggles.** `workbench.toggle.primarySideBar` (`Ctrl+B`),
  `workbench.toggle.panel` (`Ctrl+J`) and
  `workbench.toggle.secondarySideBar` (`Ctrl+Alt+B`) fold a shown area
  to its strip, or show it again, through the `workbench.fold` step: the
  area keeps its panels, its tabs and its size. An area with nothing
  left to show gets its first closed panel back (`workbench.show`). The
  primary side bar is the start side area, the panel the bottom one,
  the secondary side bar the end side one. They are in the View menu,
  ticked while their area shows; the context keys `sideBarVisible`,
  `panelVisible` and `auxiliaryBarVisible` say so. The strips plugin's
  own fold command (`view.side_bar.toggle`) has no key: `Ctrl+B` is
  VS Code's.
- **The options**, settings each change of which is one undo step: the
  primary side bar on the left or the right
  (`workbench.side_bar_position`, `left` by default; the secondary one
  lies across from it, the activity bar beside the primary one; the
  `workbench.layout.sideBarPosition` command), and the bottom panel
  under the editors or across the full width
  (`workbench.panel_alignment`, `center` by default as VS Code's; the
  `workbench.layout.panelAlignment` command).
- **The layout menu** (`layout_controls::MENU_LAYOUT`, View > Customize
  layout, the `workbench.layout.customize` command): the area toggles;
  the arrangements, the application's templates and the saved ones, the
  current one ticked; "Save the current arrangement as…", which asks for
  a name in a dialog; and the options.
- **In the title bar**, when the application asks for them:

  ```rust,ignore
  App::builder().plugin(FramePlugin::own().layout_controls(true))
  ```

  A "Customize layout" button opening that menu, then a toggle per
  area, at the end of the bar before the window's buttons (the
  `egui_pop.frame.layout` block of the `app.caption` zone). Each toggle's
  icon is filled while its area shows and outlined while it is folded or
  hidden (`layout-left`, `layout-bottom`, `layout-right` and their
  `-off` twins); its tooltip gives its key, and assistive technology
  hears it as a toggle. They are drawn by the kit's `icon-button`.
  `layout_controls::show` draws the same controls anywhere else (the
  gallery's Layout controls page).
- **The side bars' icons follow their side**, as VS Code's: with the
  primary side bar on the right, its toggle shows `layout-right` and
  the secondary side bar's `layout-left`, and the title bar's toggles
  read secondary, panel, primary from the start. The layout menu, the
  View menu and the palette mirror the toggle commands' icons the same
  way (`layout_controls::command_icon`); the menus keep their order, as
  VS Code's layout menu does, and the commands and their keys stay the
  same. The title bar's icons are listed by the debug bridge's `icons`
  collector (the `layout controls` surface), the menus' rows by
  `menus::rows_drawn`.

## Palettes and toolboxes: never modal

Decided on 2026-10-13: a toolbox, a colour palette or a navigator
floating over the canvas is **non-modal, always**. It never blocks the
interface, never takes the keyboard unless clicked into, and gives the
keyboard back on Escape. Anything that must block until it is answered
is a dialog (`egui_pop::api::dialogs`, movable when it hides what it
acts on), never a palette.

**One model, two placements.** A palette is a panel declared with
`PanelSpec::palette()`: it opens floating over the workbench, and its
area is where it docks. Any panel declared `floatable(true)` may float
too. The layout keeps floating panels next to the docked and detached
ones (`Layout::floating`: place, size, rolled up, pinned), saved and repaired
with them.

```rust
use egui_pop::egui;
use egui_pop::model::ids::PanelId;
use egui_pop::model::panels::{Area, PanelSpec};
use egui_pop::prelude::*;

const TOOLS: PanelId = PanelId::from_static("demo.tools");

struct Demo;

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        // Floats when it opens; docks in the start side bar.
        ctx.panel(
            PanelSpec::new(TOOLS, "Tools", Area::StartSide).palette(),
            |ui, _| {
                ui.label("Brush");
            },
        )?;
        Ok(())
    }
}

let app = App::builder().plugin(Demo).build(&egui::Context::default()).unwrap();
assert!(app.handle().layout().floating_of(&TOOLS).is_some());
```

- **The band.** A palette has the movable dialogs' header band
  (`egui_pop_core::band`, the kit's `header-band` and
  `band-icon-button`): its title, then Pin, Roll up and Close.
  Rolled up, only the band is left (a double click on the band does
  it too). Pinned, it stays where it is, in sight and opaque.
- **Dragging.** The band drags the palette, kept inside the workbench
  (again when the window is resized) and snapping to the workbench's
  edges and to the other palettes within the `palette.magnet` token.
  Dropped on a side bar or the bottom panel (or a closed one's edge),
  with the drop hint the tabs use, it docks there. A docked floatable
  panel floats again from its tab menu (Float).
- **Sticky edges.** Dropped on an edge of the workbench (snapped onto it
  or kept against it), a palette is anchored there: `FloatingGroup::anchor`
  holds, on each axis, the start edge, the end edge or none, and the
  distance to it. When the window is resized, or a saved layout is
  restored at another size, an anchored edge keeps its distance, so a
  palette on the right edge follows it; the other axis keeps its place.
  Dragged away from the edge, it is no longer anchored. A palette off
  the edges, or snapped to another palette only, is just kept inside.
  The anchor is saved with the layout (layouts saved before it load
  unanchored); `palettes::anchor_of` and `palettes::anchored_pos`
  compute it.
- **Groups.** Palettes float in groups (`Layout::floating` holds
  `FloatingGroup`s; a palette alone is a group of one). A palette
  dropped on another one's top or bottom quarter (`palettes::join_at`,
  with the drop hint the tabs use) stacks above or below it: a column
  where each member keeps its own band, its own roll-up (a column of
  rolled-up toolboxes) and its Close. The group's band, over its
  members', lists their titles: it drags the group as one unit, pins
  it and anchors it as a single palette. Dropped on a side bar or the
  bottom panel, it docks every member there, as tabs; floating one of
  them again (Float) gives the whole group back, from the members still
  docked beside it (`Layout::docked_groups`). A member's own band drags
  it out of its group (dropped back over its group, it stays), or onto
  another palette's edge into that group. Dropped on the middle of
  another palette, a palette joins its group as a tab: the group's
  band holds the tabs (a click shows one), the shown member keeps its
  own band, which drags it out. The `palette.group.gap`
  token is the room between stacked members. A group is saved as the
  floating panel it was before groups, its other members under `group`:
  an application that predates groups floats them apart.
- **Docking conventions.** Groups follow the classic docking
  conventions (Photoshop, Krita, GIMP, Qt docks, VS Code), decided on
  2026-10-21:
  - *Close group* is an entry of the menu on the group's band (its
    `…` button), never a button on the band, so that a stray click
    cannot close a group. It runs `palette.close_group`, which a hook
    may veto.
  - *A tab dragged out* of its tabs comes out of them
    (`palette.ungroup`): alone where it is dropped, or into the palette
    or group it is dropped on, following the usual drop rules (an edge
    stacks, the middle adds a tab, an area docks it). Dropped back on
    its own tabs, it stays. The member's band still drags it too.
  - *Closing the shown tab* shows its neighbour on the right, or the one
    on its left when it was the last (Qt's default, as in browsers).
  - *An edge drop on a group of tabs* stacks the palette above or below
    it, and the tabs stay tabs: a stack's member may be a tab group
    (`Member::tabs`), one level deep, never a stack in tabs. Its band
    holds its tabs, the shown tab's Close and the tab group's Roll; the
    band drags the tab group out of the stack whole (`palette.ungroup`
    with `tabs: true`). A middle drop on a member of a stack makes that
    member alone a tab group, the rest of the stack staying a stack. A
    group docked whole (`palette.dock` with `group: true`) and floated
    back comes back nested as it was.
  - *Layouts.* A nested tab group is saved as its first tab, its other
    tabs under `tabs` and the shown one as `shown`: layouts saved before
    load unchanged, and an application that predates nesting reads the
    first tab as a plain member of the stack and puts the other tabs
    back as it does any panel missing from a layout (a palette floats
    again on its own, a panel opening closed stays closed).
- **Sizes.** A palette is sized by its content until it is resized:
  its right edge, its bottom edge and the corner between them (the
  `palette.grip` token's width) drag its size, kept between the
  `palette.min-width` and `palette.max-width` tokens (never narrower
  than its panels' least width), `palette.min-height` and
  `palette.max-height`, and inside the workbench. Its content scrolls
  when it is taller than the room left; rolled up, it keeps its width.
  The size is the group's (`FloatingGroup::size`, saved with the layout;
  layouts saved before it load sized by their content): palettes
  joining a group take its size, a palette coming out of it is sized by
  its content again. Resized against an edge of the workbench, it is
  anchored there; pulled away from it, it no longer is. A double click
  on an edge gives it its content's size back, and `palettes.reset`
  does for every palette.
- **Tab** (`palettes.toggle`, rebindable) hides and shows the unpinned
  palettes. It is the toggle only while some panel floats, no widget
  has the keyboard and no overlay is open; otherwise Tab moves the
  keyboard to the next widget, as always.
- **Strokes and the pointer.** `palettes::set_stroke(app, true)` says
  a stroke is in progress: with the `palettes.fade_on_stroke` setting
  on, the palettes fade until it ends. `palettes.idle_opacity` makes
  them translucent while the pointer is elsewhere.
- **Reset.** `palettes.reset` puts them back where they first open,
  rolled down, sized by their content and shown.
- **Steps** (ADR 0011): `palette.open`, `palette.float`, `palette.dock`,
  `palette.move` (once a drag is dropped: a veto puts the palette back,
  a redirect places it elsewhere), `palette.resize` (`{panel, size}`,
  once an edge is dropped; `size: null` for the content's size; a veto
  gives it its size back), `palette.roll`, `palette.pin`,
  `palette.group` (`{panels, into, at}`, `at` being `above`, `below` or
  `tab`), `palette.ungroup` (`{panel, pos}`, `tabs: true` for a nested
  tab group whole) and `palette.close_group` (`{panel}`); a group docks
  whole through `palette.dock` with `group: true`. Once done, the
  journal says `palette.opened`, `palette.floated`, `palette.docked`,
  `palette.moved`, `palette.resized`, `palette.rolled`,
  `palette.pinned`, `palette.grouped`, `palette.ungrouped` or
  `palette.group_closed`.

Not built yet: a palette outside the main window, as a native utility
window. egui's viewports have no owner window (only "always on top" of
every application), so such a window would float above other
applications too.

## Where the layout is kept

With a persistence directory (`AppBuilder::persist`), the layout is
loaded at the start and saved by `App::save` (so at exit) in the
document the host names, `Host::layout_document`: `layout` by default,
a workspace's own (`workspaces/photos/layout`, asked again at each load
and save), or `None` for a host keeping the layout itself through
`AppHandle::layout` and `set_layout` (`LocalHost::with_layout_document`
opts out the same way). An application switching workspaces calls
`App::load_layout` once its host names the new one. Both are steps,
`layout.load` and `layout.save` (`{ "document" }`): a plugin may
redirect the document, veto, or load and save elsewhere.

## Arrangements

Named layouts saved, switched and reset, kept in the host's store
(`egui_pop::api::arrangements`, in `DefaultPlugins`). A plugin offers
templates (`Ctx::arrangement`); resetting goes back to the current
arrangement's template. The `Default` arrangement's template is the
starting layout (`AppBuilder::layout`), or every panel in its default
area. Steps: `arrangement.save`, `arrangement.switch`,
`arrangement.reset`, `arrangement.delete`.

## Slim strips

`StripsPlugin` (opt-in) draws the activity bar, a button per panel of the
start side bar (`Ctrl+B`, the workbench's `workbench.toggle.primarySideBar`,
folds the side bar to its strip), and status bar
indicators fed by the host's journal: an `Indicator` is a reducer over
the events of a prefix, so a long job publishes its progress and the
status bar follows it.

The activity bar and the status bar are each one Tab stop
([a roving group](inputs.md#a-roving-group-is-one-stop)): Up and Down go
between the activity bar's buttons, Left and Right between the status bar's
items, and Tab coming back lands on the one focused there last.

```rust
use egui_pop::api::strips::{Indicator, Tone};

let jobs = Indicator::new("jobs", "Idle", "job.").reduce(|state, event| {
    let n = state.count.unwrap_or(0);
    state.count = Some(match event.name.as_str() {
        "job.started" => n + 1,
        "job.done" => n.saturating_sub(1),
        _ => n,
    });
    state.tone = if state.count == Some(0) { Tone::Neutral } else { Tone::Accent };
});
# let _ = jobs;
```

An indicator can follow the context stack ([contexts](contexts.md)):
`when("place == 'photos.album'")` shows it only where the clause holds,
with the grammar of commands over the context keys (`place`,
`placeDepth`, `modePushed`, `mode.<group>`, a frame's own keys);
`during(&frame)` writes a frame's clause for it. Hidden, it keeps
reducing events, so its state is right the moment it shows again. A
clause that does not parse keeps it hidden and is a diagnostic
(`indicator-when`).

With the context stack installed too, the status bar starts with where
the user is and what is active ([status and context](status-and-context.md)):
the breadcrumb (a click on a crumb goes back to that place, the current
one does nothing), then a chip per active mode whose group is declared
visible (`FrameSpec::visible`, its tone `FrameSpec::tone`). A click on a
chip leaves its mode; a pushed mode shows a back mark and goes back to
what it replaced; a mode whose guard refuses keeps its chip and says why
in its tooltip. On a narrow window the middle crumbs collapse into `…`
(a menu of them) first, then indicators give way (the lowest
`priority` first, then the heaviest) into the bar's `…` menu; chips
never hide. `StripsPlugin::breadcrumb(false)` keeps the bar to its
indicators, for an application showing the breadcrumb elsewhere
(`contexts::breadcrumb`, `contexts::mode_chips`); `contexts::crumbs`,
`contexts::chips` and `contexts::why_not` give what they show.

The user hides status items and orders them from an item's context
menu (only status items can be muted); agents use `status.item.hide`
(`{ "id", "hidden" }`) and `status.item.move` (`{ "id", "toward":
"start" | "end" }`), code `strips::hide` and `strips::move_item`. The
choices (`strips::choices`) are kept per place of the context stack in
its `view` document (`contexts::place_value`, `set_place_value`), so a
bar arranged for the darkroom comes back as it was; a place without
choices of its own takes the root's. Without the context stack they
last the session.

Items sit at the bar's start or end (`align`), show busy, error and
warning states, and may open a popover or a rich hover card: see
[the status bar](status-bar.md).

Agents read what the user sees: `context.read` gives the visible status
items in its `status` field (each one's `id`, `text`, `tone`, `count`,
`progress`, and the states of [the status bar](status-bar.md#what-agents-read)), and the `strips` debug collector shows the breadcrumb,
the chips, every item with why it is hidden (`hidden_by`: the `when`
clause that does not hold, `user`, or `narrow`) and the choices of the
place shown; in code, `strips::status` and `strips::visible`.

## The window's own frame

`FramePlugin` (opt-in) draws the title bar in the `TITLE` zone above the
menu bar, and the window's resize edges, never thinner than 6 physical
pixels; a setting gives the system's frame back. The frame publishes
each window's hit regions in physical pixels (`egui_pop::core::frame`,
`HitTable`, `HitRegions::hit`), what a platform layer needs to answer
the system's hit tests itself. Steps: `frame.minimize`,
`frame.maximize`, `frame.close`, `frame.system_menu`,
`frame.exit_fullscreen`, `frame.hit_test`.

The bar starts with the `CAPTION_START` zone, whose default block
(`frame::BLOCK_MENU`, `egui_pop.frame.menu`) is the system menu button
and the application's name; an application puts its logo there by
replacing that block, and keeps the menu with `frame::menu_button`. The
bar itself (`frame::BLOCK_TITLE`) keeps following the setting. The
`CAPTION` zone holds the application's buttons, before the window's;
`FramePlugin::layout_controls(true)` puts VS Code's layout controls
there, nearest the window's buttons (see "The layout controls").

**In full screen** (`F11`) the bar stays, with both zones: an
application keeps its menus, status items and logo there. Only what a
window needs goes: minimize, the resize edges (the hit table publishes
no edge), and moving the window by its bar. Close stays, and a button
leaving full screen takes maximize's place (the
`window-exit-fullscreen` glyph role, the `frame.exit_fullscreen`
step). A double click on the bar does nothing in full screen: it would
otherwise maximize, which full screen has no use for, and leaving full
screen on a double click aimed at an item of the bar would surprise
more than it helps; the way out is that button, or `F11`. The
`window.bar_in_fullscreen` setting (on by default) gives the former
behaviour back: off, the bar is not drawn in full screen and publishes
no caption, and the strip at the top edge (`WindowPlugin`) is the way
out.

```rust
use egui_pop::api::frame;
use egui_pop::prelude::*;

# fn connect(ctx: &mut Ctx<'_>) {
ctx.block(
    zones::CAPTION_START,
    Block::replace(frame::BLOCK_MENU, BlockId::from_static("app.logo"), |ui, app| {
        frame::menu_button(ui, app);
        ui.strong("My app");
    }),
);
# }
```

### Windows: Snap Layouts and Aero Snap

On Windows, when the application draws the frame, `FramePlugin`
subclasses the main window (the `egui-pop-win32` crate) and answers the
system's hit test from the published regions: the caption moves the
window natively (Aero Snap, shake), the maximize button shows Snap
Layouts, the edges resize. Clicks on the buttons and a double or right
click on the caption still come back as the frame's requests, so their
steps run. In full screen the caption is answered as client area (the
system must not move the window) and so is the button leaving full
screen, which is no maximize button: Snap Layouts never show in full
screen. The window's handle comes from eframe when the application
starts with `run_native` (or `run_native_with`); an application starting
eframe itself passes it with `frame::set_native_window`. Elsewhere,
nothing is installed. A plugin may keep the system's hit testing:

```rust
use egui_pop::api::frame::{FramePlugin, STEP_HIT_TEST};
use egui_pop::core::frame::{FrameLayout, Hit, HitRegions};
use egui_pop::egui;
use egui_pop::prelude::*;

struct KeepSystemHitTest;

impl Plugin for KeepSystemHitTest {
    fn id(&self) -> PluginId {
        PluginId::from_static("keep_hit_test")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.before(STEP_HIT_TEST, |_, args| {
            if args["install"] == true { Before::Veto("ours".into()) } else { Before::Go }
        });
        Ok(())
    }
}

let app = App::builder()
    .plugin(FramePlugin::own())
    .plugin(KeepSystemHitTest)
    .build(&egui::Context::default())
    .unwrap();
assert!(app.handle().diagnostics().is_empty());

// What the layer answers is what the regions say (physical pixels).
let bar = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 32.0));
let layout = FrameLayout {
    window: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0)),
    caption: Some(bar),
    maximize: Some(egui::Rect::from_min_size(egui::pos2(712.0, 0.0), egui::vec2(44.0, 32.0))),
    grip: 4.0,
    resizable: true,
    ..FrameLayout::default()
};
let regions = HitRegions::measure(&layout, 2.0);
assert_eq!(regions.hit(1468, 30), Hit::Maximize);
assert_eq!(regions.hit(800, 30), Hit::Caption);
```

## Scale

Each window has its own scale: `egui_pop::core::scale` gives it
(`Scale::of`), snaps stroke widths to whole pixels (`stroke`,
`hairline`) and says once when it changes (`changed`; `scale.changed` on
the journal). The interface zoom is egui-pop's (`egui_pop::api::zoom`): a
setting in fixed steps, `Ctrl+Alt+=`, `Ctrl+Alt+-` and `Ctrl+Alt+0`
(rebindable), the `interface.zoom` step; egui's own zoom keys are off.

## The keymap at run time

A host whose registry changes declares commands after build
(`AppHandle::declare`, `undeclare`): menus, the palette and the keymap
follow. A profile is a whole keymap document: `export_keymap(layer)` and
`load_keymap(doc, layer)` keep scopes, `when` clauses and every key, and
report what changed (`KeymapDiff`). Rebinding is the `keys.rebind` step,
loading `keys.load`; each new conflict is a `keys.conflict` signal.

Keys change in the user's layer (`rebind`, `reset_keys`, saved with the
interface's settings) or in the workspace's (`rebind_in`,
`reset_keys_in` with `Layer::Workspace`), which the application keeps
and reads back with `export_keymap`. The shortcuts editor offers the
choice once the application asks (`offer_workspace_keys`), the user's
by default, and says after each key where it comes from
(`key_sources`): `workspace` or `user`, nothing for a default, or the
application's own name for it (`name_key_sources`). Two
bindings of one key whose `when` clauses can never both hold
(`view == 'masks'` and `view == 'lighttable'`, `When::excludes`) are no
conflict.

Pointer gestures live in the same keymap (`Ctrl+Wheel`, `MiddleDrag`
bound to gesture commands in a canvas's scope), the pointer profile's
in the default layer; `rebind_gestures` gives a gesture several
triggers at once. See [Pointer gestures and profiles](pointer-gestures.md).

## Around the shell

`AppBuilder::wrap` draws around the whole application,
`main_edge_to_edge` drops the centre's margins, `frameless` an area's
margins and its panels' frames, `layout` gives the starting layout,
`on_exit` runs before the
`app.exit` step (whose default saves), `run_native_with` hands the built
application to an `eframe::App` of the application's own (its `make`
returns a `Result`: an error is logged, published as `app.make_failed`
and returned by eframe), and
`window::restore` gives `NativeOptions` the window's kept place.
