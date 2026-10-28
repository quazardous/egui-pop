//! Toolbars (`docs/toolbars.md`): a menu's items drawn as a row of
//! buttons, each with its command's icon, its title and key in the
//! tooltip, on while its `checked` clause holds, greyed where it does not
//! apply; groups separated; the tools that do not fit in "More tools";
//! agents read what was drawn.

use egui::accesskit::{Role, Toggled};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_api::menus;
use egui_pop_api::toolbars::{self, Labels, ToolKind, Toolbar};
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::ids::{BlockId, CommandId, MenuId, PluginId};
use egui_pop_model::menus::MenuItem;
use serde_json::{Value as Json, json};

/// The toolbar, drawn wide.
const WIDE: MenuId = MenuId::from_static("demo.toolbar");
/// The same tools, drawn in a narrow room.
const NARROW: MenuId = MenuId::from_static("demo.narrow");
/// The same tools with their titles beside the icons.
const BESIDE: MenuId = MenuId::from_static("demo.beside");
const SUB: MenuId = MenuId::from_static("demo.align");

const SAVE: CommandId = CommandId::from_static("demo.save");
const GRID: CommandId = CommandId::from_static("demo.grid");
const DELETE: CommandId = CommandId::from_static("demo.delete");
const PLAIN: CommandId = CommandId::from_static("demo.plain");
const ALIGN: CommandId = CommandId::from_static("demo.align.left");

struct Demo;

fn on(app: &egui_pop_api::AppHandle, key: &str) -> Json {
    let now = app.context().get(key) != "true";
    app.set_context(key, if now { "true" } else { "false" });
    json!(now)
}

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.app().set_context("grid", "true");
        let place = |spec: CommandSpec, group: &str, order: i32| {
            spec.menu(WIDE, group, order)
                .menu(NARROW, group, order)
                .menu(BESIDE, group, order)
        };
        ctx.command(
            place(
                CommandSpec::new(SAVE, "Save", "Demo")
                    .icon("save")
                    .key("Ctrl+S"),
                "1",
                0,
            ),
            |call| Ok(on(call.app, "saved")),
        )?
        .command(
            place(
                CommandSpec::new(GRID, "Show the grid", "Demo")
                    .icon("square")
                    .checked("grid"),
                "1",
                1,
            ),
            |call| Ok(on(call.app, "grid")),
        )?
        .command(
            place(
                CommandSpec::new(DELETE, "Delete", "Demo")
                    .icon("delete")
                    .when("never"),
                "2",
                0,
            ),
            |_| Ok(Json::Null),
        )?
        .command(
            place(CommandSpec::new(PLAIN, "Plain", "Demo"), "2", 1),
            |call| Ok(on(call.app, "plain")),
        )?
        .command(
            CommandSpec::new(ALIGN, "Align left", "Demo").menu(SUB, "1", 0),
            |_| Ok(Json::Null),
        )?;
        for menu in [WIDE, NARROW, BESIDE] {
            ctx.menu_item(MenuItem::submenu(menu, "3", 0, SUB, "Align"))?;
        }
        ctx.block(
            egui_pop_api::zones::MAIN,
            egui_pop_api::zones::Block::add(BlockId::from_static("demo.toolbars"), 0, |ui, app| {
                ui.horizontal(|ui| {
                    Toolbar::new(app, &WIDE).show(ui);
                });
                ui.horizontal(|ui| {
                    Toolbar::new(app, &BESIDE).labels(Labels::Beside).show(ui);
                });
                ui.allocate_ui(egui::vec2(90.0, 60.0), |ui| {
                    ui.horizontal(|ui| {
                        Toolbar::new(app, &NARROW).show(ui);
                    });
                });
            }),
        );
        Ok(())
    }
}

fn harness() -> Harness<'static, App> {
    let app = App::builder()
        .plugin(Demo)
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 500.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h.run();
    h
}

#[test]
fn a_toolbar_shows_its_commands_icons_keys_and_states() {
    let h = harness();
    let app = h.state().handle().clone();
    let layout = toolbars::layout(&app, &WIDE).expect("drawn");
    assert_eq!(layout.overflow, 0, "{layout:?}");
    let kinds: Vec<ToolKind> = layout.tools.iter().map(|t| t.kind).collect();
    assert_eq!(
        kinds,
        [
            ToolKind::Command,
            ToolKind::Command,
            ToolKind::Separator,
            ToolKind::Command,
            ToolKind::Command,
            ToolKind::Separator,
            ToolKind::Submenu,
        ]
    );
    let tool = |label: &str| {
        layout
            .tools
            .iter()
            .find(|t| t.label == label)
            .unwrap_or_else(|| panic!("{label} in {layout:?}"))
            .clone()
    };
    let save = tool("Save");
    assert_eq!(save.icon.as_deref(), Some("save"));
    assert_eq!(save.keys.as_deref(), Some("Ctrl+S"));
    assert!(save.enabled);
    let grid = tool("Show the grid");
    assert!(grid.checkable && grid.checked);
    let delete = tool("Delete");
    assert!(!delete.enabled);
    assert!(delete.why.is_some());
    assert_eq!(tool("Plain").icon, None);
    // Icon-only buttons are named by their titles; a toggle is announced
    // as toggled.
    // (The three toolbars hold the same tools: the first is the wide one.)
    let node = h.query_all_by_label("Show the grid").next().unwrap();
    assert_eq!(node.accesskit_node().role(), Role::Button);
    assert_eq!(node.accesskit_node().toggled(), Some(Toggled::True));
    let delete = h.query_all_by_label("Delete").next().unwrap();
    assert!(delete.accesskit_node().is_disabled());
    let drawn = egui_pop_core::icon::registry::drawn(&h.ctx);
    assert!(
        drawn
            .iter()
            .any(|d| d.surface == "toolbar" && d.icon == "save"),
        "{drawn:?}"
    );
}

#[test]
fn a_click_runs_the_command_and_its_state_follows() {
    let mut h = harness();
    let app = h.state().handle().clone();
    h.query_all_by_label("Show the grid")
        .next()
        .unwrap()
        .click();
    h.run();
    h.run();
    assert_eq!(app.context().get("grid"), "false");
    let grid = toolbars::layout(&app, &WIDE)
        .unwrap()
        .tools
        .into_iter()
        .find(|t| t.command == Some(GRID))
        .unwrap();
    assert!(!grid.checked);
}

#[test]
fn titles_show_beside_the_icons_when_asked() {
    let h = harness();
    let app = h.state().handle().clone();
    let beside = toolbars::layout(&app, &BESIDE).unwrap();
    assert_eq!(beside.labels, Labels::Beside);
    // Every toolbar of the harness draws its own: one is enough.
    assert!(h.query_all_by_label("Save").next().is_some());
}

#[test]
fn the_tools_that_do_not_fit_go_into_more_tools() {
    let mut h = harness();
    let app = h.state().handle().clone();
    let narrow = toolbars::layout(&app, &NARROW).unwrap();
    assert!(narrow.overflow > 0, "{narrow:?}");
    // From the end: the submenu went first.
    let align = narrow
        .tools
        .iter()
        .find(|t| t.kind == ToolKind::Submenu)
        .unwrap();
    assert!(align.overflow);
    // No separator ends the row.
    let shown: Vec<_> = narrow.tools.iter().filter(|t| !t.overflow).collect();
    assert_ne!(shown.last().map(|t| t.kind), Some(ToolKind::Separator));
    h.get_by_label("More tools").click();
    h.run();
    h.run();
    let rows: Vec<String> = menus::rows_drawn(&h.ctx)
        .into_iter()
        .filter(|r| r.menu == NARROW)
        .map(|r| r.label)
        .collect();
    for t in narrow
        .tools
        .iter()
        .filter(|t| t.overflow && t.kind != ToolKind::Separator)
    {
        assert!(rows.contains(&t.label), "{} in {rows:?}", t.label);
    }
}

#[test]
fn agents_read_the_toolbars_drawn() {
    let h = harness();
    let snapshot = h.state().handle().debug_snapshot();
    let wide = &snapshot["toolbars"]["demo.toolbar"];
    assert_eq!(wide["orientation"], "horizontal");
    assert_eq!(wide["tools"][0]["command"], "demo.save");
    assert_eq!(wide["tools"][0]["icon"], "save");
}
