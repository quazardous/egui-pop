//! The zones demo: one plugin declares a card zone with three blocks;
//! another replaces one, extends another and wraps the third. Switching
//! the second plugin off puts the card back as its owner made it.

use egui::{RichText, Stroke, Ui};
use egui_pop::api::stories::Story;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::model::commands::CommandSpec;
use egui_pop::model::ids::{BlockId, CommandId, PluginId, StoryId, ZoneId};
use egui_pop::prelude::{Block, Place};
use egui_pop::theme::Tokens;
use serde_json::json;

use crate::shell::SHELL;

/// The card zone.
pub const CARD: ZoneId = ZoneId::from_static("demo.card");
pub const ZONES: PluginId = PluginId::from_static("gallery.zones");
pub const EXTENDER: PluginId = PluginId::from_static("gallery.zones-extender");
pub const CMD_TOGGLE: CommandId = CommandId::from_static("gallery.zones.toggle-extender");

const TITLE: BlockId = BlockId::from_static("card.title");
const BODY: BlockId = BlockId::from_static("card.body");
const FOOTER: BlockId = BlockId::from_static("card.footer");

/// Declares the card zone, its three blocks and the page.
#[derive(Debug, Default)]
pub struct ZonesPage;

impl Plugin for ZonesPage {
    fn id(&self) -> PluginId {
        ZONES
    }

    fn title(&self) -> String {
        "Zones".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.zone(CARD, "A card")?
            .block(
                CARD,
                Block::add(TITLE, 0, |ui, _| {
                    ui.strong("Card title");
                }),
            )
            .block(
                CARD,
                Block::add(BODY, 10, |ui, _| {
                    ui.label("The body as gallery.zones draws it.");
                }),
            )
            .block(
                CARD,
                Block::add(FOOTER, 20, |ui, _| {
                    ui.weak("Footer by gallery.zones");
                }),
            )
            .command(
                CommandSpec::new(CMD_TOGGLE, "Toggle the extender plugin", "Gallery")
                    .description("Connects or disconnects gallery.zones-extender.")
                    .key("Ctrl+E")
                    .when("galleryPage == 'zones'"),
                |call| {
                    let on = extender_connected(call.app);
                    call.app.set_plugin_enabled(EXTENDER, !on);
                    Ok(json!({ "extender": !on }))
                },
            )?
            .story(
                Story::new(StoryId::from_static("zones"), "Zones and blocks", show)
                    .description(
                        "One plugin declares a zone and its blocks; another replaces, extends and wraps them.",
                    )
                    .group("Framework", 100)
                    .playground(playground),
            )?;
        Ok(())
    }
}

/// Replaces the body, extends the title, wraps the footer.
#[derive(Debug, Default)]
pub struct Extender;

impl Plugin for Extender {
    fn id(&self) -> PluginId {
        EXTENDER
    }

    fn title(&self) -> String {
        "Zones extender".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![ZONES]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.block(
            CARD,
            Block::replace(BODY, BlockId::from_static("extender.body"), |ui, _| {
                ui.label("The body, replaced by gallery.zones-extender.");
            }),
        )
        .block(
            CARD,
            Block::extend(
                Place::After(TITLE),
                BlockId::from_static("extender.badge"),
                |ui, _| {
                    let t = Tokens::of(ui.ctx());
                    ui.label(RichText::new("Extended after the title").color(t.accent.default));
                },
            ),
        )
        .block(
            CARD,
            Block::wrap(
                FOOTER,
                BlockId::from_static("extender.frame"),
                |ui, _, inner| {
                    let t = Tokens::of(ui.ctx());
                    egui::Frame::new()
                        .stroke(Stroke::new(1.0, t.accent.default))
                        .corner_radius(t.radius.widget)
                        .inner_margin(t.space.s)
                        .show(ui, |ui| {
                            ui.small("Wrapped by gallery.zones-extender:");
                            inner(ui);
                        });
                },
            ),
        );
        Ok(())
    }
}

fn extender_connected(app: &AppHandle) -> bool {
    app.plugins()
        .iter()
        .any(|p| p.id == EXTENDER && p.connected)
}

fn show(ui: &mut Ui, app: &AppHandle) {
    let t = Tokens::of(ui.ctx());
    ui.label("The card below is the zone `demo.card`, drawn block by block:");
    ui.add_space(t.space.s);
    egui::Frame::new()
        .fill(t.surface.raised)
        .stroke(Stroke::new(1.0, t.border.default))
        .corner_radius(t.radius.window)
        .inner_margin(t.space.m)
        .show(ui, |ui| {
            ui.set_width(360.0);
            app.show_zone(ui, &CARD);
        });
    ui.add_space(t.space.l);
    ui.strong("Resolved blocks");
    let zone = app.zones().into_iter().find(|z| z.id == CARD);
    egui::Grid::new("card.blocks")
        .num_columns(4)
        .striped(true)
        .spacing([t.space.l, t.space.xs])
        .show(ui, |ui| {
            for h in ["Block", "Owner", "Weight", "Applied"] {
                ui.strong(h);
            }
            ui.end_row();
            for b in zone.map(|z| z.blocks).unwrap_or_default() {
                ui.monospace(b.id.as_str());
                ui.monospace(b.owner.as_str());
                ui.label(b.weight.to_string());
                ui.label(if b.notes.is_empty() {
                    "added".to_string()
                } else {
                    b.notes.join("; ")
                });
                ui.end_row();
            }
        });
    ui.add_space(t.space.m);
    ui.weak("Ctrl+E (on this page) or the playground switches the extender plugin.");
}

fn playground(ui: &mut Ui, app: &AppHandle) {
    let mut on = extender_connected(app);
    if ui.checkbox(&mut on, "Extender plugin connected").changed() {
        app.set_plugin_enabled(EXTENDER, on);
    }
}
