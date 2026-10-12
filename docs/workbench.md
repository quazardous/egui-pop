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
there when its rules allow it; otherwise the drop hint says why.

## Auxiliary windows

A detachable panel moves to an OS window of its own, with its own scale
and the application's frame when it draws one
(`AppHandle::detach_panel`, `attach_panel`; the tab menu offers
Detach). Closing that window is the `workbench.close_window` step: its
default docks the panel back, a veto keeps the window open. Its place is
kept in the layout and repaired before the window opens.

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
the system's hit tests itself (Windows Snap Layouts). Steps:
`frame.minimize`, `frame.maximize`, `frame.close`, `frame.system_menu`.

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

## Around the shell

`AppBuilder::wrap` draws around the whole application,
`main_edge_to_edge` drops the centre's margins, `on_exit` runs before the
`app.exit` step (whose default saves), `run_native_with` hands the built
application to an `eframe::App` of the application's own, and
`window::restore` gives `NativeOptions` the window's kept place.
