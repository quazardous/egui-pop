//! The toolbars page: a menu's items drawn as toolbars (`toolbars`):
//! icons with their titles and keys in the tooltips, toggles on, greyed
//! tools, groups separated, a submenu; the same tools with their titles,
//! squeezed so the rest goes into "More tools", and as a vertical tool
//! palette whose tools come from a list, the one in use on. egui-pop's own Edit commands
//! take their place with their built-in icons.

use egui::Ui;
use egui_pop::api::clipboard;
use egui_pop::api::menus::ListItem;
use egui_pop::api::stories::Story;
use egui_pop::api::toolbars::{Labels, Toolbar};
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::model::commands::CommandSpec;
use egui_pop::model::ids::{CommandId, MenuId, PluginId, StoryId};
use egui_pop::model::menus::MenuItem;
use serde_json::json;

use super::section;
use crate::shell::SHELL;

pub const TOOLBARS: PluginId = PluginId::from_static("gallery.toolbars");
/// The toolbar, with icons alone.
pub const BAR: MenuId = MenuId::from_static("gallery.toolbars.bar");
/// The same tools with their titles beside the icons.
pub const BAR_TITLES: MenuId = MenuId::from_static("gallery.toolbars.titles");
/// The same tools in a narrow room.
pub const BAR_NARROW: MenuId = MenuId::from_static("gallery.toolbars.narrow");
/// A vertical tool palette.
pub const PALETTE: MenuId = MenuId::from_static("gallery.toolbars.palette");
/// The toolbars' submenu.
pub const ALIGN_MENU: MenuId = MenuId::from_static("gallery.toolbars.align");
pub const CMD_NEW: CommandId = CommandId::from_static("gallery.toolbars.new");
pub const CMD_OPEN: CommandId = CommandId::from_static("gallery.toolbars.open");
pub const CMD_SAVE: CommandId = CommandId::from_static("gallery.toolbars.save");
pub const CMD_GRID: CommandId = CommandId::from_static("gallery.toolbars.grid");
pub const CMD_LOCK: CommandId = CommandId::from_static("gallery.toolbars.lock");
pub const CMD_PUBLISH: CommandId = CommandId::from_static("gallery.toolbars.publish");
pub const CMD_ALIGN: CommandId = CommandId::from_static("gallery.toolbars.align");
pub const CMD_TOOL: CommandId = CommandId::from_static("gallery.toolbars.tool");
/// The palette's tools, listed when it is drawn.
pub const TOOLS: &str = "gallery.toolbars.tools";

/// The horizontal toolbars holding every tool; the one with titles holds
/// the page's own commands only, so it fits.
const BARS: [MenuId; 2] = [BAR, BAR_NARROW];

/// The toolbars page.
#[derive(Debug, Default)]
pub struct ToolbarsPage;

fn toggle(app: &AppHandle, key: &str) -> serde_json::Value {
    let on = app.context().get(key) != "true";
    app.set_context(key, if on { "true" } else { "false" });
    json!({ key: on })
}

impl Plugin for ToolbarsPage {
    fn id(&self) -> PluginId {
        TOOLBARS
    }

    fn title(&self) -> String {
        "Toolbars".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL, clipboard::PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let app = ctx.app();
        app.set_context("galleryToolbarGrid", "true");
        app.set_context("galleryToolbarLocked", "false");
        app.set_context("galleryTool", "pick");
        // Each command goes into every horizontal toolbar.
        let bars = |mut spec: CommandSpec, group: &str, order: i32| {
            for bar in BARS.into_iter().chain([BAR_TITLES]) {
                spec = spec.menu(bar, group, order);
            }
            spec
        };
        ctx.command(
            bars(
                CommandSpec::new(CMD_NEW, "New card", "Toolbars")
                    .description("Starts a new card.")
                    .icon("plus"),
                "1_file",
                0,
            ),
            |_| Ok(json!(null)),
        )?
        .command(
            bars(
                CommandSpec::new(CMD_OPEN, "Open a card", "Toolbars")
                    .description("Opens a card.")
                    .icon("folder"),
                "1_file",
                1,
            ),
            |_| Ok(json!(null)),
        )?
        .command(
            bars(
                CommandSpec::new(CMD_SAVE, "Save the card", "Toolbars")
                    .description("Saves the card.")
                    .icon("save")
                    .key("Ctrl+Alt+S"),
                "1_file",
                2,
            ),
            |_| Ok(json!(null)),
        )?
        .command(
            bars(
                CommandSpec::new(CMD_GRID, "Show the grid", "Toolbars")
                    .description("Shows or hides the card's grid: on, the button is framed.")
                    .icon("square")
                    .checked("galleryToolbarGrid"),
                "3_view",
                0,
            ),
            |call| Ok(toggle(call.app, "galleryToolbarGrid")),
        )?
        .command(
            bars(
                CommandSpec::new(CMD_LOCK, "Lock the card", "Toolbars")
                    .description("Locks the card: an open padlock, a closed one while locked.")
                    .icon("unlock")
                    .icon_on("lock")
                    .checked("galleryToolbarLocked"),
                "3_view",
                1,
            ),
            |call| Ok(toggle(call.app, "galleryToolbarLocked")),
        )?
        .command(
            bars(
                CommandSpec::new(CMD_PUBLISH, "Publish", "Toolbars")
                    .description("Has no icon: its title stands in for one. Greyed while the card is locked.")
                    .when("galleryToolbarLocked != 'true'"),
                "4_share",
                0,
            ),
            |_| Ok(json!(null)),
        )?
        .command(
            CommandSpec::new(CMD_ALIGN, "Align", "Toolbars")
                .description("Aligns the card's items (`side`).")
                .args(json!({ "side": "left" })),
            |call| Ok(call.args.clone()),
        )?
        .command(
            CommandSpec::new(CMD_TOOL, "Pick a tool", "Toolbars")
                .description("Picks the palette's tool (`tool`).")
                .args(json!({ "tool": "pick" })),
            |call| {
                let tool = call
                    .args
                    .get("tool")
                    .and_then(serde_json::Value::as_str)
                    .ok_or("`tool` is missing")?;
                call.app.set_context("galleryTool", tool);
                Ok(json!({ "tool": tool }))
            },
        )?;
        // egui-pop's own Edit commands, with their built-in icons.
        for bar in BARS {
            for (i, id) in [
                egui_pop::api::app::CMD_UNDO,
                egui_pop::api::app::CMD_REDO,
                clipboard::CMD_CUT,
                clipboard::CMD_COPY,
                clipboard::CMD_PASTE,
            ]
            .into_iter()
            .enumerate()
            {
                ctx.menu_item(MenuItem::command(bar.clone(), "2_edit", i as i32, id))?;
            }
            ctx.submenu(bar, "5_arrange", 0, ALIGN_MENU, "Align")?;
        }
        for (i, side) in ["left", "centre", "right"].into_iter().enumerate() {
            ctx.menu_item(
                MenuItem::command(ALIGN_MENU, "1", i as i32, CMD_ALIGN)
                    .args(json!({ "side": side }))
                    .label(&format!("Align {side}")),
            )?;
        }
        // The palette: its tools from a list, each with its own icon, the
        // one in use on; then two toggles.
        ctx.menu_item(MenuItem::list(PALETTE, "1_tools", 0, TOOLS))?;
        ctx.list_provider(TOOLS, |_, cx| {
            [
                ("pick", "Pick", "picker"),
                ("search", "Find", "search"),
                ("edit", "Draw", "edit"),
                ("erase", "Erase", "delete"),
            ]
            .into_iter()
            .map(|(tool, title, icon)| {
                ListItem::new(title, CMD_TOOL, json!({ "tool": tool }))
                    .icon(icon)
                    .checked(cx.get("galleryTool") == tool)
            })
            .collect()
        });
        ctx.menu_item(
            MenuItem::command(PALETTE, "2_view", 0, CMD_GRID).checked("galleryToolbarGrid"),
        )?
        .menu_item(
            MenuItem::command(PALETTE, "2_view", 1, CMD_LOCK).checked("galleryToolbarLocked"),
        )?;
        ctx.story(
            Story::new(StoryId::from_static("toolbars"), "Toolbars", show)
                .description("A menu's items drawn as toolbars: icons with their titles and keys in the tooltips, toggles framed while on, greyed tools, groups separated, a submenu, titles beside the icons, the tools that do not fit in More tools, and a vertical tool palette.")
                .group("Components", 41),
        )?;
        Ok(())
    }
}

fn show(ui: &mut Ui, app: &AppHandle) {
    ui.label("Each toolbar draws a menu's items: a command's icon, its title and key in the tooltip, framed while its command is on, greyed where it does not apply (Publish while the card is locked; Undo, Cut and Paste when there is nothing to undo or no selection). Groups are separated; Align opens a submenu.");
    section(ui, "Icons");
    ui.horizontal(|ui| {
        Toolbar::new(app, &BAR).show(ui);
    });
    section(ui, "Titles beside the icons");
    ui.horizontal_wrapped(|ui| {
        Toolbar::new(app, &BAR_TITLES)
            .labels(Labels::Beside)
            .show(ui);
    });
    section(ui, "In a narrow room: the rest in More tools");
    ui.allocate_ui(egui::vec2(240.0, 48.0), |ui| {
        ui.horizontal(|ui| {
            Toolbar::new(app, &BAR_NARROW).show(ui);
        });
    });
    section(ui, "A vertical tool palette");
    ui.horizontal(|ui| {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            Toolbar::new(app, &PALETTE)
                .labels(Labels::Beside)
                .vertical()
                .show(ui);
        });
        ui.label(format!(
            "Tool: {}, grid {}",
            app.context().get("galleryTool"),
            if app.context().get("galleryToolbarGrid") == "true" {
                "on"
            } else {
                "off"
            }
        ));
    });
}
