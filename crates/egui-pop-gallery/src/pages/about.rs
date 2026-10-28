//! The About page: what the gallery says about itself (the About
//! dialog's body, drawn in the page), a button opening the dialog, and
//! the Help menu's links to the repository and the documentation.

use egui::Ui;
use egui_pop::api::about::{self, ABOUT_PLUGIN, AppInfo, CMD_ABOUT};
use egui_pop::api::app::MENU_HELP;
use egui_pop::api::stories::Story;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::core::state::Slot;
use egui_pop::model::commands::CommandSpec;
use egui_pop::model::ids::{CommandId, PluginId, StoryId};
use serde_json::{Value as Json, json};

use super::section;
use crate::shell::SHELL;

pub const ABOUT_PAGE: PluginId = PluginId::from_static("gallery.about");

/// The repository.
pub const REPOSITORY: &str = "https://github.com/quazardous/egui-pop";
/// The site: the landing page, the gallery, the docs and the API.
pub const SITE: &str = "https://quazardous.github.io/egui-pop/";
/// The docs, on the site.
pub const DOCS: &str = "https://quazardous.github.io/egui-pop/docs/";

/// Help > egui-pop on GitHub.
pub const CMD_GITHUB: CommandId = CommandId::from_static("gallery.help.github");
/// Help > Documentation.
pub const CMD_DOCS: CommandId = CommandId::from_static("gallery.help.docs");

/// What the gallery says about itself.
pub fn info() -> AppInfo {
    AppInfo::new(crate::TITLE, env!("CARGO_PKG_VERSION"))
        .description("egui-pop's components in every variant, state, theme and density.")
        .licence("MIT OR Apache-2.0")
        .author("The egui-pop contributors")
        .repository(REPOSITORY)
        .website(SITE)
        .docs(DOCS)
        .issues(&format!("{REPOSITORY}/issues"))
        .icon(super::frame::logo_sized)
}

/// The About page, and the Help menu's links.
#[derive(Debug, Default)]
pub struct AboutPage;

impl Plugin for AboutPage {
    fn id(&self) -> PluginId {
        ABOUT_PAGE
    }

    fn title(&self) -> String {
        "About page".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL, ABOUT_PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.command(
            CommandSpec::new(CMD_GITHUB, "egui-pop on GitHub", "Help")
                .description("Opens egui-pop's repository in the browser.")
                .menu(MENU_HELP, "1_links", 0),
            |call| open_url(call.app, REPOSITORY),
        )?;
        ctx.command(
            CommandSpec::new(CMD_DOCS, "Documentation", "Help")
                .description("Opens egui-pop's documentation in the browser.")
                .menu(MENU_HELP, "1_links", 1),
            |call| open_url(call.app, DOCS),
        )?;
        ctx.story(
            Story::new(StoryId::from_static("about"), "About", show)
                .description("What an application says about itself: Help > About, with its version, licence, authors and links, and its version info to copy.")
                .group("Components", 58),
        )?;
        Ok(())
    }
}

fn open_url(app: &AppHandle, url: &str) -> Result<Json, String> {
    app.egui().open_url(egui::OpenUrl::new_tab(url));
    Ok(json!({ "opened": url }))
}

/// Whether the version info was copied from the page.
const COPIED: Slot<bool> = Slot::new("gallery.about.copied");

fn show(ui: &mut Ui, app: &AppHandle) {
    ui.label("AboutPlugin adds Help > About egui-pop gallery; the host describes the application in an AppInfo. The dialog's body, drawn in the page:");
    section(ui, "About the gallery");
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_max_width(440.0);
        let ctx = ui.ctx().clone();
        let copied = COPIED.get(&ctx).unwrap_or(false);
        if about::show(ui, app, copied) {
            COPIED.set(&ctx, true);
        }
    });
    section(ui, "Open");
    if ui.button("Open the About dialog").clicked() {
        app.run(CMD_ABOUT, Json::Null);
    }
}
