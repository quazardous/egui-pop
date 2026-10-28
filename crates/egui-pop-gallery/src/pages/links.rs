//! The links page: links the application declares (a web page, mail,
//! the gallery's own scheme, a command), links that ask first (found in
//! content, a text showing another address, an internationalised host),
//! local files (a folder, a program refused), a blocked scheme, egui's
//! own hyperlink routed through the same policy, an agent's request, and
//! the requests seen so far.

use egui::Ui;
use egui_pop::api::links::{self, Link};
use egui_pop::api::stories::Story;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::model::commands::Origin;
use egui_pop::model::ids::{PluginId, StoryId};
use serde_json::json;

use super::section;
use crate::shell::SHELL;

pub const LINKS_PAGE: PluginId = PluginId::from_static("gallery.links");

/// The repository: a declared web link.
pub const REPOSITORY: &str = "https://github.com/quazardous/egui-pop";
/// A link found in content: asked first.
pub const REPORT: &str = "https://example.org/report";
/// What an agent asks to open.
pub const AGENT_TARGET: &str = "https://example.org/agent";

/// The page.
#[derive(Debug, Default)]
pub struct LinksPage;

impl Plugin for LinksPage {
    fn id(&self) -> PluginId {
        LINKS_PAGE
    }

    fn title(&self) -> String {
        "Links".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL, links::PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.link("gallery.links: the repository", REPOSITORY)
            .link("gallery.links: the palette", "command:app.palette");
        ctx.story(
            Story::new(StoryId::from_static("links"), "Links", show)
                .description("One policy for every link: declared links open at once, content and agents ask first, programs are refused, the real target shows on hover.")
                .group("Framework", 105),
        )?;
        Ok(())
    }
}

fn show(ui: &mut Ui, app: &AppHandle) {
    ui.label("Hover a link to see where it goes; right-click it for Open, Show in folder and Copy link; Ctrl+click or a middle click opens it outside.");
    section(ui, "Declared by the application");
    ui.horizontal_wrapped(|ui| {
        links::link(ui, app, "Source code", REPOSITORY);
        links::link(ui, app, "Write to us", "mailto:someone@example.org");
        links::link(ui, app, "The tokens page", "gallery://page/tokens");
        links::link(ui, app, "The command palette", "command:app.palette");
    });
    section(ui, "Asked first");
    ui.horizontal_wrapped(|ui| {
        Link::content("A link found in a document", REPORT).show(ui, app);
        links::link(ui, app, "https://example.org", "https://example.com/login");
        Link::content("An internationalised host", "https://bücher.example/").show(ui, app);
    });
    section(ui, "Local files");
    ui.horizontal_wrapped(|ui| {
        let tmp = egui_pop::platform::dirs::temp_dir();
        links::link(ui, app, "The temporary folder", &tmp.display().to_string());
        Link::content("A program", "/opt/tools/install.sh").show(ui, app);
    });
    section(ui, "Blocked");
    Link::content("A script", "javascript:alert(1)").show(ui, app);
    section(ui, "egui's own hyperlink, through the same policy");
    ui.hyperlink_to("egui's Hyperlink", REPOSITORY);
    section(ui, "An agent asks");
    if ui.button("Ask to open a page, as an agent").clicked() {
        app.run_as(
            links::CMD_OPEN,
            json!({ "target": AGENT_TARGET }),
            Origin::Agent("gallery".into()),
        );
    }
    section(ui, "Requests so far");
    let recent = links::recent(app);
    if recent.is_empty() {
        ui.weak("Nothing asked yet.");
    }
    for r in recent.iter().rev().take(6) {
        ui.monospace(format!(
            "{}  {}  {} ({}, {})",
            r["status"].as_str().unwrap_or_default(),
            r["how"].as_str().unwrap_or_default(),
            r["target"].as_str().unwrap_or_default(),
            r["source"].as_str().unwrap_or_default(),
            r["origin"].as_str().unwrap_or_default(),
        ));
    }
}
