# The workbench

What an application's window holds around its content: its own frame,
the scale of each window, panels in areas, slim strips, panels detached
into windows of their own, named arrangements, and the keymap changed
at run time. Every change goes through a step (ADR 0011): a plugin may
watch it on the journal, veto or redirect it, replace it or follow it.

## Panels in areas

A plugin declares panels with their rules; the workbench places them in
areas (the centre, the start and end side bars, the bottom panel), as
tabs and splits over [egui_dock](https://github.com/Adanos020/egui_dock),
saves where the user put them and repairs that layout at the next start.
The shell's side bar, main and inspector zones are panels of their areas.

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

Each panel gets `<id>.focus` and `<id>.toggle` commands. The layout
changes through the steps `workbench.move`, `workbench.show`,
`workbench.hide`, `workbench.focus`, `workbench.fold`,
`workbench.place` and `workbench.zen` (`egui_pop::api::workbench`). A tab dragged onto another area moves
there when its rules allow it; otherwise the drop hint says why. Over
an area it may go to, the hint (the kit's `dock-preview`) carries a
ghost of the dragged tab (the kit's `tab-preview`, with the tab's title
and width as laid out once per drag) where the drop puts it: after the
last tab of the area's first tab bar, or at the top of an area drawn
bare or closed. A refused drop shows no ghost. The tabs already there
are not pushed aside, and the ghost is always at the end: egui_dock
only follows a drag within its own area, so it gives no insertion
index for a tab coming from another one, and a drop on an area appends
to its first tabs (making room live and dropping between tabs are
possible follow-ups). A palette dragged over an area it docks in gets
the same ghost. The hint is published like an overlay:
`AppHandle::drop_hint` (panel, area, rect, refusal, and `preview`, the
ghost's area, index among the tabs it joins and rect), `overlay_rects`
(kind `DropHint`, named `drop:<panel>:<area>`), and the debug bridge's
`workbench` collector (`drop_hint.preview: {area, index, rect}`), which
also shows the layout, each area's rect, the focused panel and the tab
being dragged.

A side area lies **beside** the centre, taking its room, or **over**
it (`workbench.place`, `{ "area", "placement": "over" }`;
`AppHandle::place_area`, `AppBuilder::over` for the first start): its
folded strip keeps the room in the layout, so the centre keeps its
width (a terminal in it is not resized), and the area, unfolded, lies
above the centre next to its strip, with the kit's `side-area` shadow
(`shadow.side`). A click on the strip folds or unfolds it; its edge on
the centre's side resizes it. The placement is saved with the layout
(`over`), like folding; a plugin may veto the step.

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

The bottom panel spans the window's whole width by default. The
`workbench.panel_alignment` setting lines it up otherwise, as VS Code's
panel alignment does: `center` (between the side bars, which go down to
the bottom), `start` (under the start side bar) or `end` (under the end
one); `AppBuilder::panel_alignment` gives the default.

## The menu bar

The shell draws the main menu (`app.main`) as a menu bar. When the
window is too narrow for all of it, the menus that fit stay in the bar,
from the start, and the rest become submenus of a "…" entry (the
`more` icon, named "More menus"), keeping their titles, items and
`when` clauses. When no menu fits besides "…", the whole menu is one
entry (the `menu` icon, named "Main menu"). The blocks of the
`app.top-bar` zone keep the width they took, after the menus. Each
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
publishes no caption. It has no default key, so that each application
binds its own; Escape leaves it (`workbench.leave_zen`, while no
overlay is open, no mode is pushed and no text field has the
keyboard). The change is the `workbench.zen` step (`{ "zen": bool }`,
`AppHandle::set_zen`, `AppHandle::zen`, the `workbenchZen` context
key): a plugin may veto it, and the journal records it. Zen mode is a
state of the session, not of the layout: it is never saved, and the
next start shows the whole workbench. Combined with full screen, it
leaves the content alone on the screen.

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
ones (`Layout::floating`: place, rolled up, pinned), saved and repaired
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
- **Tab** (`palettes.toggle`, rebindable) hides and shows the unpinned
  palettes. It is the toggle only while some panel floats, no widget
  has the keyboard and no overlay is open; otherwise Tab moves the
  keyboard to the next widget, as always.
- **Strokes and the pointer.** `palettes::set_stroke(app, true)` says
  a stroke is in progress: with the `palettes.fade_on_stroke` setting
  on, the palettes fade until it ends. `palettes.idle_opacity` makes
  them translucent while the pointer is elsewhere.
- **Reset.** `palettes.reset` puts them back where they first open,
  rolled down and shown.
- **Steps** (ADR 0011): `palette.open`, `palette.float`, `palette.dock`,
  `palette.move` (once a drag is dropped: a veto puts the palette back,
  a redirect places it elsewhere), `palette.roll`, `palette.pin`; once
  done, the journal says `palette.opened`, `palette.floated`,
  `palette.docked`, `palette.moved`, `palette.rolled` or
  `palette.pinned`.

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
arrangement's template. Steps: `arrangement.save`, `arrangement.switch`,
`arrangement.reset`, `arrangement.delete`.

## Slim strips

`StripsPlugin` (opt-in) draws the activity bar, a button per panel of the
start side bar (`Ctrl+B` folds the side bar to its strip), and status bar
indicators fed by the host's journal: an `Indicator` is a reducer over
the events of a prefix, so a long job publishes its progress and the
status bar follows it.

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
(a menu of them) first, then the heaviest indicators give way; chips
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

Agents read what the user sees: `context.read` gives the visible status
items in its `status` field (each one's `id`, `text`, `tone`, `count`
and `progress`), and the `strips` debug collector shows the breadcrumb,
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
`CAPTION` zone holds the application's buttons, before the window's.

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

## Around the shell

`AppBuilder::wrap` draws around the whole application,
`main_edge_to_edge` drops the centre's margins, `on_exit` runs before the
`app.exit` step (whose default saves), `run_native_with` hands the built
application to an `eframe::App` of the application's own (its `make`
returns a `Result`: an error is logged, published as `app.make_failed`
and returned by eframe), and
`window::restore` gives `NativeOptions` the window's kept place.
