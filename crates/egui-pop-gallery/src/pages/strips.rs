//! The strips page: the activity bar beside the side bar, folding, and
//! status bar items fed by the journal: a pretend job publishing its
//! progress (with a hover card following it), problems at the bar's
//! start (error and warning counts, a popover), and a node connection of
//! high priority (a hover card with a link, a popover with a small
//! form).

use egui::Ui;
use egui_pop::api::stories::Story;
use egui_pop::api::strips::{self, Alignment, Indicator, Tone};
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::core::hover_card::Card;
use egui_pop::model::commands::CommandSpec;
use egui_pop::model::ids::{CommandId, PluginId, StoryId};
use egui_pop::model::panels::Area;
use serde_json::{Value as Json, json};

use super::section;
use crate::shell::SHELL;

pub const STRIPS_PAGE: PluginId = PluginId::from_static("gallery.strips");

/// Clears the pretend problems.
const CLEAR_PROBLEMS: CommandId = CommandId::from_static("gallery.problems.clear");
/// Reconnects the pretend node.
const RECONNECT: CommandId = CommandId::from_static("gallery.node.reconnect");
/// Where the new items show: this page only, so the other pages' bars
/// stay as they are.
const HERE: &str = "galleryPage == 'strips'";

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
            Indicator::new("gallery.jobs", "No job", "gallery.job.")
                .reduce(|state, event| {
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
                })
                .hover(|state, _| {
                    // Live: built from the state each time it shows.
                    let mut card = Card::new().text("**Export** of the pretend job");
                    if state.progress.is_some() || state.busy.is_some() {
                        card = card.progress("Exporting", state.progress);
                    }
                    card.row("Last", state.detail.trim_start_matches("last event: "))
                }),
        );
        ctx.command(
            CommandSpec::new(CLEAR_PROBLEMS, "Clear the problems", "Gallery")
                .description("Clears the strips page's pretend problems."),
            |call| {
                call.app
                    .publish("gallery", None, "gallery.problem.clear", Json::Null);
                Ok(Json::Null)
            },
        )?
        .command(
            CommandSpec::new(RECONNECT, "Reconnect the node", "Gallery")
                .description("Reconnects the strips page's pretend node."),
            |call| {
                call.app
                    .publish("gallery", None, "gallery.node.connected", Json::Null);
                Ok(Json::Null)
            },
        )?;
        ctx.indicator(
            Indicator::new("gallery.problems", "No problem", "gallery.problem.")
                .when(HERE)
                .align(Alignment::Start)
                .reduce(|state, event| {
                    match event.name.as_str() {
                        "gallery.problem.error" => state.errors += 1,
                        "gallery.problem.warning" => state.warnings += 1,
                        "gallery.problem.clear" => {
                            state.errors = 0;
                            state.warnings = 0;
                        }
                        _ => {}
                    }
                    (state.text, state.tone) = if state.errors > 0 {
                        ("Problems".into(), Tone::Danger)
                    } else if state.warnings > 0 {
                        ("Problems".into(), Tone::Warning)
                    } else {
                        ("No problem".into(), Tone::Neutral)
                    };
                })
                .popover(problems_popover)
                .hover(|state, _| {
                    Card::new()
                        .text("**Problems** found by the pretend checks:\n- errors stop an export\n- warnings do not")
                        .row("Errors", state.errors.to_string())
                        .row("Warnings", state.warnings.to_string())
                        .separator()
                        .link("Clear them", CLEAR_PROBLEMS, Json::Null)
                }),
        )
        .indicator(
            Indicator::new("gallery.node", "Node: connected", "gallery.node.")
                .when(HERE)
                .priority(10)
                .weight(-10)
                .initial(strips::IndicatorState {
                    text: "Node: connected".into(),
                    tone: Tone::Success,
                    ..Default::default()
                })
                .reduce(|state, event| {
                    if event.name == "gallery.node.connected" {
                        state.text = "Node: connected".into();
                        state.tone = Tone::Success;
                    }
                })
                .popover(node_popover)
                .hover(|state, _| {
                    Card::new()
                        .text(format!("The render node is `{}`", state.text.trim_start_matches("Node: ")))
                        .row("Host", "node-1.local")
                        .row("Latency", "12 ms")
                        .link("Reconnect", RECONNECT, Json::Null)
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
    ui.weak("The indicator at the status bar's end follows these events: nothing polls, the journal feeds it. Its hover card follows them too.");
    section(ui, "Live states");
    ui.horizontal_wrapped(|ui| {
        if ui.button("Add an error").clicked() {
            app.publish("gallery", None, "gallery.problem.error", Json::Null);
        }
        if ui.button("Add a warning").clicked() {
            app.publish("gallery", None, "gallery.problem.warning", Json::Null);
        }
        if ui.button("Clear the problems").clicked() {
            app.run(CLEAR_PROBLEMS, Json::Null);
        }
    });
    ui.weak("The problems sit at the bar's start, after the breadcrumb: counts with their icons, never colour alone. Turning to errors pulses once and is announced once.");
    section(ui, "Popovers and hover cards");
    ui.horizontal_wrapped(|ui| {
        if ui.button("Open the problems popover").clicked() {
            app.run(strips::CMD_OPEN_ITEM, json!({ "id": "gallery.problems" }));
        }
        if ui.button("Show the node's card").clicked() {
            app.run(strips::CMD_HOVER_ITEM, json!({ "id": "gallery.node" }));
        }
    });
    ui.weak("A click (or Enter) on an item opens its popover; Escape or a click outside closes it. Hovering shows its card, whose links run commands; Ctrl+K Ctrl+I opens it on the focused item. The node has the highest priority: on a narrow window it stays, the others go into the bar's `…` menu.");
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

/// The problems' popover: what is wrong, and ways out.
fn problems_popover(ui: &mut Ui, app: &AppHandle) {
    let state = app
        .indicators()
        .into_iter()
        .find(|(i, _)| i.id == "gallery.problems")
        .map(|(_, s)| s)
        .unwrap_or_default();
    ui.strong("Problems");
    ui.label(format!(
        "{} errors, {} warnings from the pretend checks.",
        state.errors, state.warnings
    ));
    ui.horizontal(|ui| {
        if ui.button("Dismiss all").clicked() {
            app.run(CLEAR_PROBLEMS, Json::Null);
        }
        if ui.button("Add a warning").clicked() {
            app.publish("gallery", None, "gallery.problem.warning", Json::Null);
        }
    });
}

/// The node's popover: a small connection form.
fn node_popover(ui: &mut Ui, app: &AppHandle) {
    let id = egui::Id::new("gallery.node.host");
    let mut host: String = ui
        .data(|d| d.get_temp(id))
        .unwrap_or_else(|| "node-1.local".to_string());
    ui.strong("Render node");
    ui.horizontal(|ui| {
        ui.label("Host");
        ui.text_edit_singleline(&mut host);
    });
    if ui.button("Connect").clicked() {
        app.run(RECONNECT, Json::Null);
        app.run(
            strips::CMD_OPEN_ITEM,
            json!({ "id": "gallery.node", "open": false }),
        );
    }
    ui.data_mut(|d| d.insert_temp(id, host));
}
