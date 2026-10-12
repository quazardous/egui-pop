//! The menus page: commands declared once, shown in the menu bar and in
//! a context menu with their keys, ticks and greyed states.

use egui::Ui;
use egui_pop::api::app::MENU_VIEW;
use egui_pop::api::menus::ListItem;
use egui_pop::api::stories::Story;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError, menus};
use egui_pop::model::commands::CommandSpec;
use egui_pop::model::ids::{CommandId, MenuId, PluginId, StoryId};
use egui_pop::model::menus::MenuItem;
use egui_pop::theme::Tokens;
use serde_json::json;

use super::section;
use crate::shell::SHELL;

pub const MENUS: PluginId = PluginId::from_static("gallery.menus");
/// The card's context menu.
pub const CARD_MENU: MenuId = MenuId::from_static("gallery.menus.card");
/// A submenu of it.
pub const ZOOM_MENU: MenuId = MenuId::from_static("gallery.menus.zoom");
pub const CMD_GRID: CommandId = CommandId::from_static("gallery.menus.grid");
pub const CMD_SNAP: CommandId = CommandId::from_static("gallery.menus.snap");
pub const CMD_ZOOM: CommandId = CommandId::from_static("gallery.menus.zoom");
pub const CMD_LOCKED: CommandId = CommandId::from_static("gallery.menus.locked");
/// The zoom submenu's list, filled when it opens.
pub const ZOOM_LIST: &str = "gallery.menus.zooms";

/// The menus page.
#[derive(Debug, Default)]
pub struct MenusPage;

fn toggle(app: &AppHandle, key: &str) -> serde_json::Value {
    let on = app.context().get(key) != "true";
    app.set_context(key, if on { "true" } else { "false" });
    json!({ key: on })
}

impl Plugin for MenusPage {
    fn id(&self) -> PluginId {
        MENUS
    }

    fn title(&self) -> String {
        "Menus".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let app = ctx.app();
        app.set_context("galleryGrid", "true");
        app.set_context("galleryZoom", "100");
        ctx.command(
            CommandSpec::new(CMD_GRID, "Show grid", "Menus")
                .description("Shows or hides the card's grid.")
                .key("Ctrl+Shift+G")
                .checked("galleryGrid")
                .menu(MENU_VIEW, "7_menus", 0)
                .menu(CARD_MENU, "1_view", 0),
            |call| Ok(toggle(call.app, "galleryGrid")),
        )?
        .command(
            CommandSpec::new(CMD_SNAP, "Snap to grid", "Menus")
                .description("Snaps what moves on the card to its grid.")
                .when("galleryGrid")
                .checked("gallerySnap")
                .menu(MENU_VIEW, "7_menus", 1)
                .menu(CARD_MENU, "1_view", 1),
            |call| Ok(toggle(call.app, "gallerySnap")),
        )?
        .command(
            CommandSpec::new(CMD_ZOOM, "Zoom", "Menus")
                .description("Sets the card's zoom (`percent`).")
                .args(json!({ "percent": 100 })),
            |call| {
                let p = call
                    .args
                    .get("percent")
                    .and_then(serde_json::Value::as_i64)
                    .ok_or("`percent` is missing")?;
                call.app.set_context("galleryZoom", &p.to_string());
                Ok(json!({ "percent": p }))
            },
        )?
        .command(
            CommandSpec::new(CMD_LOCKED, "Delete the card", "Menus")
                .description("Never applies here: shown greyed, with the reason.")
                .when("galleryCardUnlocked")
                .menu(CARD_MENU, "9_danger", 0),
            |_| Ok(json!(null)),
        )?
        .submenu(CARD_MENU, "2_zoom", 0, ZOOM_MENU, "Zoom")?;
        // A list filled when the menu opens: other zooms, the one in use
        // ticked.
        ctx.menu_item(MenuItem::list(ZOOM_MENU, "2", 0, ZOOM_LIST))?;
        ctx.list_provider(ZOOM_LIST, |_, cx| {
            [25, 75, 150]
                .into_iter()
                .map(|p| {
                    ListItem::new(&format!("{p}% (listed)"), CMD_ZOOM, json!({ "percent": p }))
                        .checked(cx.get("galleryZoom") == p.to_string())
                })
                .collect()
        });
        for (i, p) in [50, 100, 200].into_iter().enumerate() {
            ctx.menu_item(
                MenuItem::command(ZOOM_MENU, "1", i as i32, CMD_ZOOM)
                    .args(json!({ "percent": p }))
                    .label(&format!("{p}%"))
                    .checked(&format!("galleryZoom == '{p}'")),
            )?;
        }
        ctx.story(
            Story::new(StoryId::from_static("menus"), "Menus", show)
                .description("Commands declared once, in the menu bar and a context menu: keys, ticks, greyed items, submenus.")
                .group("Components", 40),
        )?;
        Ok(())
    }
}

fn show(ui: &mut Ui, app: &AppHandle) {
    let t = Tokens::of(ui.ctx());
    let cx = app.context();
    ui.label("The View menu holds two of these commands; the card's context menu holds them all, its Zoom submenu a list filled when it opens.");
    section(ui, "A card with a context menu");
    let frame = egui::Frame::group(ui.style())
        .inner_margin(t.space.l)
        .show(ui, |ui| {
            ui.set_min_size(egui::vec2(320.0, 120.0));
            ui.strong("Right-click this card");
            ui.label(format!(
                "Grid {}, snap {}, zoom {}%",
                if cx.get("galleryGrid") == "true" {
                    "on"
                } else {
                    "off"
                },
                if cx.get("gallerySnap") == "true" {
                    "on"
                } else {
                    "off"
                },
                cx.get("galleryZoom"),
            ));
        });
    let card = ui.interact(
        frame.response.rect,
        ui.id().with("gallery.menus.card"),
        egui::Sense::click(),
    );
    card.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Card with a context menu")
    });
    menus::context_menu_with(&card, app, &CARD_MENU, json!({ "card": "demo" }));
    let popup = ui.button("Open the card's menu here");
    if popup.clicked() {
        menus::popup_at(app, &CARD_MENU, popup.rect);
    }
    section(ui, "The same menu, drawn in place");
    egui::Frame::menu(ui.style()).show(ui, |ui| {
        ui.set_max_width(260.0);
        menus::show(ui, app, &CARD_MENU);
    });
}
