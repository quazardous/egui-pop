//! The strips page: the activity bar beside the side bar, folding, and a
//! status bar indicator fed by the journal (a pretend job publishing its
//! progress).

use egui::Ui;
use egui_pop::api::stories::Story;
use egui_pop::api::strips::{Indicator, Tone};
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::model::ids::{PluginId, StoryId};
use egui_pop::model::panels::Area;
use serde_json::json;

use super::section;
use crate::shell::SHELL;

pub const STRIPS_PAGE: PluginId = PluginId::from_static("gallery.strips");

/// The strips page and its indicator.
#[derive(Debug, Default)]
pub struct StripsPage;

impl Plugin for StripsPage {
    fn id(&self) -> PluginId {
        STRIPS_PAGE
    }

    fn title(&self) -> String {
        "Strips".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.indicator(
            Indicator::new("gallery.jobs", "No job", "gallery.job.").reduce(|state, event| {
                match event.name.as_str() {
                    "gallery.job.started" => {
                        state.text = "Exporting".into();
                        state.tone = Tone::Accent;
                        state.progress = Some(0.0);
                        // Busy since the job started: a spinner after
                        // the delay.
                        state.busy = event.data["at"].as_f64();
                    }
                    "gallery.job.progress" => {
                        let p = event.data["progress"].as_f64().unwrap_or(0.0) as f32;
                        state.progress = Some(p);
                        state.text = format!("Exporting {}%", (p * 100.0).round());
                    }
                    "gallery.job.done" => {
                        state.text = "Exported".into();
                        state.tone = Tone::Success;
                        state.progress = None;
                        state.busy = None;
                    }
                    _ => {}
                }
                state.detail = format!("last event: {}", event.name);
            }),
        );
        ctx.story(
            Story::new(StoryId::from_static("strips"), "Strips", show)
                .description("The activity bar beside the side bar (folding it to its strip), and status bar indicators fed by the host's events.")
                .group("Framework", 103),
        )?;
        Ok(())
    }
}

fn show(ui: &mut Ui, app: &AppHandle) {
    section(ui, "A job, as the host would publish it");
    ui.horizontal_wrapped(|ui| {
        if ui.button("Start").clicked() {
            app.publish(
                "gallery",
                None,
                "gallery.job.started",
                json!({ "at": egui_pop::core::motion::now(ui.ctx()) }),
            );
        }
        for p in [0.25, 0.5, 0.75] {
            if ui.button(format!("{}%", p * 100.0)).clicked() {
                app.publish(
                    "gallery",
                    None,
                    "gallery.job.progress",
                    json!({ "progress": p }),
                );
            }
        }
        if ui.button("Finish").clicked() {
            app.publish("gallery", None, "gallery.job.done", json!({}));
        }
    });
    ui.weak("The indicator at the status bar's end follows these events: nothing polls, the journal feeds it.");
    section(ui, "The side bar");
    let folded = app.layout().folded.contains(&Area::StartSide);
    ui.horizontal(|ui| {
        if ui
            .button(if folded {
                "Unfold the side bar"
            } else {
                "Fold the side bar (Ctrl+B)"
            })
            .clicked()
        {
            app.fold_area(Area::StartSide, !folded);
        }
    });
    ui.weak("The activity bar shows a button per panel of the start side: the shown one folds the side bar, another shows its panel.");
    section(ui, "Indicators");
    for (ind, state) in app.indicators() {
        ui.monospace(format!("{}: {} ({:?})", ind.id, state.text, state.tone));
    }
}
