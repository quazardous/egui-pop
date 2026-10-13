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
`workbench.hide`, `workbench.focus` and `workbench.fold`
(`egui_pop::api::workbench`). A tab dragged onto another area moves
there when its rules allow it; otherwise the drop hint says why. The
hint is published like an overlay: `AppHandle::drop_hint` (panel,
area, rect, refusal), `overlay_rects` (kind `DropHint`, named
`drop:<panel>:<area>`), and the debug bridge's `workbench` collector,
which also shows the layout, each area's rect, the focused panel and
the tab being dragged.

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
    state.tone = if state.count == Some(0) { Tone::Quiet } else { Tone::Active };
});
# let _ = jobs;
```

## The window's own frame

`FramePlugin` (opt-in) draws the title bar in the `TITLE` zone above the
menu bar, and the window's resize edges, never thinner than 6 physical
pixels; a setting gives the system's frame back. The frame publishes
each window's hit regions in physical pixels (`egui_pop::core::frame`,
`HitTable`, `HitRegions::hit`), what a platform layer needs to answer
the system's hit tests itself. Steps: `frame.minimize`,
`frame.maximize`, `frame.close`, `frame.system_menu`, `frame.hit_test`.

The bar starts with the `CAPTION_START` zone, whose default block
(`frame::BLOCK_MENU`, `egui_pop.frame.menu`) is the system menu button
and the application's name; an application puts its logo there by
replacing that block, and keeps the menu with `frame::menu_button`. The
bar itself (`frame::BLOCK_TITLE`) keeps following the setting. The
`CAPTION` zone holds the application's buttons, before the window's.
In full screen the bar is not drawn and publishes no caption.

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
steps run. The window's handle comes from eframe when the application
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
loading `keys.load`; each new conflict is a `keys.conflict` signal. Two
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
