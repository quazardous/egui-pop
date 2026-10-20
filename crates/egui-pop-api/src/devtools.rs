//! Devtools, as a plugin ([`DevtoolsPlugin`], opt-in): the debug
//! bridge's collectors shown in the application itself, in a panel of
//! the workbench (`egui_pop.devtools`, bottom), behind a setting
//! (`developer.devtools`, off by default) so end users never meet it.
//! `F12` (View > Devtools) opens it when the setting is on.
//!
//! Tabs: the key resolutions (each binding considered and its verdict),
//! the journal's last events, the steps and their hooks, the focus
//! chain and the overlays, the scale of each window, and every collector
//! as JSON with its actions (an action that mutates says so).
//!
//! Steps (ADR 0011): `devtools.call` (`{ "collector", "action",
//! "args" }`): a collector action run from the panel; a plugin may veto
//! it (a kiosk build, an action too dangerous to offer).

use std::sync::{Arc, Mutex};

use egui::Ui;
use egui_pop_core::lock;
use egui_pop_core::strings;
use egui_pop_core::widgets;
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::ids::{CommandId, PanelId, PluginId, SettingId, StepId};
use egui_pop_model::panels::{Area, PanelSpec};
use egui_pop_model::values::Descriptor;
use serde_json::{Value as Json, json};

use crate::app::{AppHandle, MENU_VIEW};
use crate::steps::StepSpec;
use crate::strings as txt;
use crate::{Ctx, Plugin, PluginError};

/// The setting that lets devtools open.
pub const SETTING_DEVTOOLS: SettingId = SettingId::from_static("developer.devtools");
/// The panel.
pub const PANEL_DEVTOOLS: PanelId = PanelId::from_static("egui_pop.devtools");
/// Opens or closes the panel.
pub const CMD_DEVTOOLS: CommandId = CommandId::from_static("app.devtools");
/// A collector action run from the panel.
pub const STEP_CALL: StepId = StepId::from_static("devtools.call");

/// The tabs.
pub const TABS: [&str; 6] = [
    txt::DEVTOOLS_TAB_KEYS.fallback,
    txt::DEVTOOLS_TAB_JOURNAL.fallback,
    txt::DEVTOOLS_TAB_STEPS.fallback,
    txt::DEVTOOLS_TAB_FOCUS.fallback,
    txt::DEVTOOLS_TAB_SCALE.fallback,
    txt::DEVTOOLS_TAB_COLLECTORS.fallback,
];

#[derive(Debug, Default)]
struct State {
    tab: usize,
    collector: String,
    args: String,
    last: Option<String>,
}

/// The plugin.
#[derive(Debug, Default)]
pub struct DevtoolsPlugin {
    state: Arc<Mutex<State>>,
}

/// Whether the setting lets devtools open.
pub fn enabled(app: &AppHandle) -> bool {
    lock(&app.store())
        .get(&format!("settings/{SETTING_DEVTOOLS}"))
        .and_then(|e| e.value.as_bool())
        .unwrap_or(false)
}

impl Plugin for DevtoolsPlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("egui_pop.devtools")
    }

    fn title(&self) -> String {
        "Developer".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.setting(
            SETTING_DEVTOOLS,
            Descriptor::bool("Devtools", false).description(
                "Lets the devtools panel open (F12): key resolutions, the journal, steps, focus, scale and every debug collector.",
            ),
        )?;
        ctx.step(
            StepSpec::new(
                STEP_CALL,
                "Devtools runs a debug collector's action (`collector`, `action`, `args`).",
            )
            .args(json!({ "collector": "commands", "action": "run", "args": {} })),
        )?;
        ctx.command(
            CommandSpec::new(CMD_DEVTOOLS, "Devtools", "View")
                .description(
                    "Opens or closes the devtools panel (when the Devtools setting is on).",
                )
                .key("F12")
                .when("devtoolsEnabled")
                .menu(MENU_VIEW, "8_devtools", 0)
                .no_agent(),
            |call| {
                let shown = call.app.panel_visible(&PANEL_DEVTOOLS);
                if shown {
                    call.app.set_panel_visible(&PANEL_DEVTOOLS, false);
                } else {
                    call.app.focus_panel(&PANEL_DEVTOOLS);
                }
                Ok(json!({ "open": !shown }))
            },
        )?;
        let state = self.state.clone();
        ctx.panel(
            PanelSpec::new(PANEL_DEVTOOLS, "Devtools", Area::Bottom)
                .min_size(120.0)
                .default_size(260.0)
                .icon("command")
                .closed(),
            move |ui, app| show(ui, app, &state),
        )?;
        // The setting is followed every frame, the panel shown or not.
        ctx.block(
            crate::zones::OVERLAYS,
            crate::zones::Block::add(
                egui_pop_model::ids::BlockId::from_static("egui_pop.devtools.follow"),
                90,
                |_, app| follow(app),
            ),
        );
        Ok(())
    }
}

/// Keeps the `devtoolsEnabled` context key in line with the setting, and
/// closes the panel when the setting goes off.
pub(crate) fn follow(app: &AppHandle) {
    let on = enabled(app);
    let key = if on { "true" } else { "false" };
    if app.context().get("devtoolsEnabled") != key {
        app.set_context("devtoolsEnabled", key);
        if !on && app.panel_visible(&PANEL_DEVTOOLS) {
            app.set_panel_visible(&PANEL_DEVTOOLS, false);
        }
    }
}

fn show(ui: &mut Ui, app: &AppHandle, state: &Arc<Mutex<State>>) {
    follow(app);
    if !enabled(app) {
        ui.weak(strings::tr(ui.ctx(), &txt::DEVTOOLS_OFF));
        return;
    }
    let mut s = lock(state);
    ui.horizontal(|ui| {
        for (i, name) in txt::DEVTOOLS_TABS.iter().enumerate() {
            let name = strings::tr(ui.ctx(), name);
            if widgets::selectable_label(ui, s.tab == i, name).clicked() {
                s.tab = i;
            }
        }
    });
    ui.separator();
    egui::ScrollArea::vertical()
        .id_salt("egui_pop.devtools.scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| match s.tab {
            0 => keys(ui, app),
            1 => journal(ui, app),
            2 => steps(ui, app),
            3 => focus(ui, app),
            4 => scale(ui, app),
            _ => collectors(ui, app, &mut s),
        });
}

fn keys(ui: &mut Ui, app: &AppHandle) {
    if let Some(p) = app.pending_keys() {
        ui.strong(strings::tr_args(
            ui.ctx(),
            &txt::DEVTOOLS_WAITING,
            &[("keys", &p.to_string())],
        ));
    }
    let traces = app.key_traces();
    if traces.is_empty() {
        ui.weak(strings::tr(ui.ctx(), &txt::DEVTOOLS_PRESS_A_KEY));
    }
    for t in traces.iter().rev().take(16) {
        ui.monospace(t.to_string());
    }
}

fn journal(ui: &mut Ui, app: &AppHandle) {
    let last = app.last_event();
    for e in app.events_since(last.saturating_sub(48)).iter().rev() {
        ui.monospace(format!(
            "#{} {} {}{} {}",
            e.seq,
            e.source,
            e.name,
            e.cause
                .map(|c| format!(" (after #{c})"))
                .unwrap_or_default(),
            e.data
        ));
    }
}

fn steps(ui: &mut Ui, app: &AppHandle) {
    let ctx = ui.ctx().clone();
    egui::Grid::new("egui_pop.devtools.steps")
        .num_columns(2)
        .striped(true)
        .show(ui, |ui| {
            for s in app.steps() {
                ui.monospace(s.spec.id.as_str());
                ui.label(if s.hooks.is_empty() {
                    strings::tr(&ctx, &txt::DEVTOOLS_NO_HOOKS)
                } else {
                    s.hooks
                        .iter()
                        .map(|h| {
                            strings::tr_args(
                                &ctx,
                                &txt::DEVTOOLS_HOOK_BY,
                                &[("kind", h.kind), ("owner", h.owner.as_str())],
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                });
                ui.end_row();
            }
        });
    ui.separator();
    for t in app.step_traces().iter().rev().take(12) {
        let how = match (&t.vetoed_by, &t.result) {
            (Some((by, why)), _) => format!("vetoed by {by}: {why}"),
            (None, Some(Ok(v))) => format!("ran by {}: {v}", t.ran_by.clone().unwrap_or_default()),
            (None, Some(Err(e))) => {
                format!("failed ({}): {e}", t.ran_by.clone().unwrap_or_default())
            }
            _ => String::new(),
        };
        ui.monospace(format!("{} {} {how}", t.step, t.args));
    }
}

fn focus(ui: &mut Ui, app: &AppHandle) {
    let snap = app.debug_snapshot();
    ui.strong(strings::tr(ui.ctx(), &txt::DEVTOOLS_FOCUS_CHAIN));
    ui.monospace(snap["focus"].to_string());
    ui.strong(strings::tr(ui.ctx(), &txt::DEVTOOLS_OVERLAYS));
    ui.monospace(snap["overlays"].to_string());
    ui.strong(strings::tr(ui.ctx(), &txt::DEVTOOLS_CONTEXT_KEYS));
    for (k, v, d) in app.when_context().iter() {
        ui.monospace(format!("{k} = {v} (depth {d})"));
    }
}

fn scale(ui: &mut Ui, app: &AppHandle) {
    let snap = app.debug_snapshot();
    ui.monospace(serde_json::to_string_pretty(&snap["screen"]).unwrap_or_default());
}

fn collectors(ui: &mut Ui, app: &AppHandle, s: &mut State) {
    let described = app.debug_describe();
    let names: Vec<String> = described
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|c| c["name"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    if s.collector.is_empty() || !names.contains(&s.collector) {
        s.collector = names.first().cloned().unwrap_or_default();
    }
    ui.horizontal_wrapped(|ui| {
        for n in &names {
            if widgets::selectable_label(ui, *n == s.collector, n).clicked() {
                s.collector.clone_from(n);
            }
        }
    });
    let Some(c) = described
        .as_array()
        .and_then(|a| a.iter().find(|c| c["name"] == s.collector.as_str()))
        .cloned()
    else {
        return;
    };
    ui.weak(c["description"].as_str().unwrap_or_default());
    if let Some(actions) = c["actions"].as_array().filter(|a| !a.is_empty()) {
        ui.horizontal(|ui| {
            let label = ui.label(strings::tr(ui.ctx(), &txt::DEVTOOLS_ARGUMENTS));
            ui.add(egui::TextEdit::singleline(&mut s.args).desired_width(280.0))
                .labelled_by(label.id);
        });
        ui.horizontal_wrapped(|ui| {
            for a in actions {
                let name = a["name"].as_str().unwrap_or_default().to_string();
                let mutates = a["mutates"].as_bool().unwrap_or(false);
                let label = if mutates {
                    strings::tr_args(ui.ctx(), &txt::DEVTOOLS_MUTATES, &[("action", &name)])
                } else {
                    name.clone()
                };
                if ui
                    .button(label)
                    .on_hover_text(a["description"].as_str().unwrap_or_default())
                    .clicked()
                {
                    let args: Json =
                        serde_json::from_str(&s.args).unwrap_or_else(|_| a["args"].clone());
                    let collector = s.collector.clone();
                    let out = app.step(
                        &STEP_CALL,
                        json!({ "collector": collector, "action": name, "args": args }),
                        |app, args| {
                            let f = app.debug_call(
                                args["collector"].as_str().unwrap_or_default(),
                                args["action"].as_str().unwrap_or_default(),
                                &args["args"],
                            );
                            if f.ok { Ok(f.data) } else { Err(f.message) }
                        },
                    );
                    s.last = Some(match out {
                        crate::steps::Outcome::Done(Ok(v)) => format!("ok: {v}"),
                        crate::steps::Outcome::Done(Err(e)) => format!("failed: {e}"),
                        crate::steps::Outcome::Vetoed { by, reason } => {
                            format!("vetoed by {by}: {reason}")
                        }
                    });
                }
            }
        });
        if let Some(l) = &s.last {
            ui.monospace(l);
        }
    }
    ui.separator();
    let snap = app.debug_snapshot();
    ui.monospace(serde_json::to_string_pretty(&snap[s.collector.as_str()]).unwrap_or_default());
}
