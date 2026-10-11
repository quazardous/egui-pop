//! The debug page: the bridge's collectors and their snapshots, the last
//! key resolutions, a call log, and demos of what the collectors see (a
//! popover, an announcement, a panel that panics).

use std::cell::RefCell;
use std::rc::Rc;

use egui::{RichText, Ui};
use egui_pop::api::stories::Story;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::core::a11y::{self, Politeness};
use egui_pop::core::overlay::{self, Anchor, Kind, OverlaySpec};
use egui_pop::core::placement::Side;
use egui_pop::core::{fault, focus};
use egui_pop::model::ids::{PluginId, StoryId};
use egui_pop::theme::Tokens;
use serde_json::Value as Json;

use crate::shell::SHELL;

pub const DEBUG: PluginId = PluginId::from_static("gallery.debug");

#[derive(Debug)]
struct State {
    collector: String,
    popover: bool,
    panic: bool,
    feedback: Option<Json>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            collector: "keys".into(),
            popover: false,
            panic: false,
            feedback: None,
        }
    }
}

/// The debug page.
#[derive(Debug, Default)]
pub struct DebugPage;

impl Plugin for DebugPage {
    fn id(&self) -> PluginId {
        DEBUG
    }

    fn title(&self) -> String {
        "Debug".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let state = Rc::new(RefCell::new(State::default()));
        let s = state.clone();
        ctx.story(
            Story::new(
                StoryId::from_static("debug"),
                "Debug bridge",
                move |ui, app| show(ui, app, &mut s.borrow_mut()),
            )
            .description(
                "What the debug bridge's collectors see, the last key resolutions, and calls.",
            )
            .group("Framework", 120)
            .playground(move |ui, app| playground(ui, app, &mut state.borrow_mut())),
        )?;
        Ok(())
    }
}

fn show(ui: &mut Ui, app: &AppHandle, s: &mut State) {
    let t = Tokens::of(ui.ctx());
    let describe = app.debug_describe();
    let collectors = describe.as_array().cloned().unwrap_or_default();
    ui.horizontal_wrapped(|ui| {
        ui.label("Collector");
        for c in &collectors {
            let name = c["name"].as_str().unwrap_or_default().to_string();
            if ui.selectable_label(s.collector == name, &name).clicked() {
                s.collector = name;
            }
        }
    });
    if let Some(c) = collectors
        .iter()
        .find(|c| c["name"] == s.collector.as_str())
    {
        ui.weak(c["description"].as_str().unwrap_or_default());
        ui.horizontal_wrapped(|ui| {
            for a in c["actions"].as_array().cloned().unwrap_or_default() {
                let name = a["name"].as_str().unwrap_or_default();
                let mutates = a["mutates"].as_bool().unwrap_or(false);
                let label = if mutates {
                    format!("{name} (mutates)")
                } else {
                    name.to_string()
                };
                if ui
                    .button(label)
                    .on_hover_text(a["description"].as_str().unwrap_or_default())
                    .clicked()
                {
                    let f = app.debug_call(&s.collector, name, &a["args"]);
                    s.feedback = serde_json::to_value(f).ok();
                }
            }
        });
    }
    let snapshot = app.debug_snapshot();
    let text = serde_json::to_string_pretty(&snapshot[s.collector.as_str()]).unwrap_or_default();
    egui::ScrollArea::vertical()
        .id_salt("snapshot")
        .max_height(220.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.label(RichText::new(text).monospace().size(t.font.small));
        });
    if let Some(f) = &s.feedback {
        ui.add_space(t.space.s);
        ui.strong("Last call");
        ui.label(
            RichText::new(serde_json::to_string(f).unwrap_or_default())
                .monospace()
                .size(t.font.small),
        );
    }
    ui.add_space(t.space.m);
    ui.strong("Key resolutions");
    let traces = app.key_traces();
    if traces.is_empty() {
        ui.weak("Press a key bound to a command (Alt+Down, Ctrl+Shift+P) and its resolution shows here.");
    }
    for trace in traces.iter().rev().take(3) {
        ui.label(
            RichText::new(trace.to_string())
                .monospace()
                .size(t.font.small),
        );
    }
    ui.add_space(t.space.m);
    ui.strong("Focus chain");
    let chain = focus::chain_names(ui.ctx());
    ui.monospace(if chain.is_empty() {
        "(nothing focused)".to_string()
    } else {
        chain.join(" > ")
    });
}

fn playground(ui: &mut Ui, app: &AppHandle, s: &mut State) {
    let _ = app;
    ui.label("Things the collectors see:");
    let r = ui.button("Open a popover");
    if r.clicked() {
        s.popover = true;
    }
    let spec = OverlaySpec::new("gallery.debug.popover", "Demo popover", Kind::Popover);
    overlay::show(
        ui.ctx(),
        &spec,
        &mut s.popover,
        Some(Anchor {
            rect: r.rect,
            side: Side::Bottom,
        }),
        |ui| {
            ui.label("Escape or a click outside closes me.");
            let _ = ui.button("A button inside");
        },
    );
    if ui.button("Announce \"Saved\"").clicked() {
        a11y::announce(ui.ctx(), "Saved", Politeness::Polite);
    }
    ui.checkbox(&mut s.panic, "Make the box below panic");
    let fail = s.panic;
    fault::boundary(ui, "gallery.debug.fragile", "The fragile box", |ui| {
        if fail {
            panic!("the fragile box was asked to panic");
        }
        ui.label("The fragile box is fine.");
    });
}
