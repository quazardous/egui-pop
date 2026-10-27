//! A minimal third-party extension of egui-pop: start a crate of your
//! own from it (see `README.md`).
//!
//! It brings a **tally**, a counter with a name, and shows every way an
//! extension plugs into egui-pop:
//!
//! - [`TallyPlugin`], a plugin: everything it registers through `Ctx` is
//!   its own and goes away when it is disconnected (ADR 0004);
//! - [`Tally`], a component: drawn from component tokens that fall back
//!   on system tokens ([`TallyTokens`]), named for screen readers, its
//!   text looked up by key with an English fallback ([`strings`]);
//! - its strings in English and French (`locales/`), added to the
//!   application's catalogues when `I18nPlugin` is connected;
//! - a gallery story ([`STORY`]) any gallery application lists;
//! - a step, [`STEP_CHANGE`]: every change of a tally runs through it,
//!   so other plugins may veto, redirect, replace or follow it
//!   (ADR 0011);
//! - two commands: [`CMD_INCREMENT`] for people (the palette) and
//!   [`CMD_SET`] for agents (arguments described, driven through the
//!   debug bridge).
//!
//! The tests (`src/tests.rs`) drive all of it headless with
//! egui_kittest. They name `crate::`, never this crate's name, so they
//! keep passing in a crate made from the template under another name.

pub mod strings;
mod tokens;
mod widget;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::sync::{Mutex, PoisonError};

use egui_pop::i18n::I18n;
use egui_pop::prelude::*;
use serde_json::{Value as Json, json};

pub use tokens::TallyTokens;
pub use widget::Tally;

/// The plugin's id. Every other id of the extension starts with it.
pub const PLUGIN: PluginId = PluginId::from_static("tally");
/// A tally changes: `tally` (its name), `from`, `to`, `cause`
/// (`decrease`, `increase`, `command`). The default writes `to`.
pub const STEP_CHANGE: StepId = StepId::from_static("tally.change");
/// Adds one to a tally (`tally`).
pub const CMD_INCREMENT: CommandId = CommandId::from_static("tally.increment");
/// Sets a tally (`tally`, `value`); offered to agents.
pub const CMD_SET: CommandId = CommandId::from_static("tally.set");
/// The extension's gallery story.
pub const STORY: StoryId = StoryId::from_static("tally");
/// The counts, shared by the widgets, the commands and the story.
pub const TALLIES: ServiceToken<Tallies> = ServiceToken::new("tally.tallies");

/// The Fluent catalogues, by locale.
pub const CATALOGUES: &[(&str, &str)] = &[
    ("en", include_str!("../locales/en/tally.ftl")),
    ("fr", include_str!("../locales/fr/tally.ftl")),
];

/// The tallies' counts, by name (zero until changed).
#[derive(Debug, Default)]
pub struct Tallies {
    counts: Mutex<BTreeMap<String, i64>>,
}

impl Tallies {
    /// A tally's count.
    pub fn get(&self, name: &str) -> i64 {
        let counts = self.counts.lock().unwrap_or_else(PoisonError::into_inner);
        counts.get(name).copied().unwrap_or_default()
    }

    fn set(&self, name: &str, value: i64) {
        let mut counts = self.counts.lock().unwrap_or_else(PoisonError::into_inner);
        counts.insert(name.to_string(), value);
    }
}

/// Changes a tally through [`STEP_CHANGE`]: hooks may veto it, redirect
/// it (another `to`) or replace the write.
pub fn change(app: &AppHandle, name: &str, to: i64, cause: &str) -> Outcome {
    let Some(tallies) = app.service(&TALLIES) else {
        return Outcome::Done(Err(strings::missing_english().into()));
    };
    let from = tallies.get(name);
    let args = json!({ "tally": name, "from": from, "to": to, "cause": cause });
    app.step(&STEP_CHANGE, args, move |_, args| {
        let name = args["tally"].as_str().ok_or("`tally` is missing")?;
        let to = args["to"].as_i64().ok_or("`to` is not a whole number")?;
        tallies.set(name, to);
        Ok(json!(to))
    })
}

/// The command's answer for a step's outcome.
fn reply(outcome: Outcome) -> Result<Json, String> {
    match outcome {
        Outcome::Done(result) => result,
        Outcome::Vetoed { reason, .. } => Err(format!("vetoed: {reason}")),
    }
}

fn tally_arg(args: &Json) -> Result<&str, String> {
    args["tally"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "`tally` is missing".to_string())
}

/// The extension's plugin. Add it to an application with
/// `AppBuilder::plugin`; add `I18nPlugin` too for its translations.
#[derive(Debug, Default)]
pub struct TallyPlugin;

impl Plugin for TallyPlugin {
    fn id(&self) -> PluginId {
        PLUGIN
    }

    fn title(&self) -> String {
        "Tally".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        // Its strings, when the application translates (`I18nPlugin`
        // connects first: its priority is higher). Without it, the
        // English written in the code shows.
        if let Some(i18n) = egui_pop::i18n::plugin::of(&ctx.app().egui()) {
            add_catalogues(&i18n).map_err(PluginError::Failed)?;
        }

        ctx.service(&TALLIES, Tallies::default())?;

        ctx.step(
            StepSpec::new(
                STEP_CHANGE,
                "A tally changes (`tally`, `from`, `to`, `cause`); the default writes `to`.",
            )
            .args(json!({ "tally": "apples", "from": 2, "to": 3, "cause": "increase" })),
        )?;

        ctx.command(
            CommandSpec::new(CMD_INCREMENT, "Increase a tally", "Tally")
                .description("Adds one to a tally (`tally`: its name).")
                // Declared once, by name: menus and the palette show it.
                .icon("plus")
                .args(json!({ "tally": "apples" })),
            |call| {
                let name = tally_arg(call.args)?;
                let tallies = call
                    .app
                    .service(&TALLIES)
                    .ok_or_else(|| strings::missing_english().to_string())?;
                reply(change(call.app, name, tallies.get(name) + 1, "command"))
            },
        )?;

        // An agent's command: no key, no menu; its arguments described
        // so an agent knows what to send (the debug bridge lists it).
        ctx.command(
            CommandSpec::new(CMD_SET, "Set a tally", "Tally")
                .description(
                    "Sets a tally to a value (`tally`: its name, `value`: a whole number).",
                )
                .args(json!({ "tally": "apples", "value": 3 }))
                .hidden(),
            |call| {
                let name = tally_arg(call.args)?;
                let value = call.args["value"]
                    .as_i64()
                    .ok_or("`value` is not a whole number")?;
                reply(change(call.app, name, value, "command"))
            },
        )?;

        ctx.story(
            Story::new(STORY, "Tally", |ui, app| {
                ui.add_space(4.0);
                Tally::new("apples", "Apples").show(ui, app);
                Tally::new("pears", "Pears").show(ui, app);
            })
            .description("A counter with a name: an extension's component, drawn from its tokens.")
            .group("Extensions", 0)
            .playground(|ui, app| {
                let tallies = app.service(&TALLIES);
                let count = tallies.map_or(0, |t| t.get("apples"));
                ui.label(format!("apples = {count}"));
                if ui.button("Reset apples").clicked() {
                    let _ = change(app, "apples", 0, "command");
                }
            }),
        )?;
        Ok(())
    }
}

/// Adds the extension's catalogues to an application's, once (a plugin
/// connected again finds them there).
pub fn add_catalogues(i18n: &I18n) -> Result<(), String> {
    for (locale, source) in CATALOGUES {
        if i18n.keys(locale).contains("tally-value") {
            continue;
        }
        i18n.add_owned(PLUGIN.as_str(), locale, source)
            .map_err(|e| format!("{locale}/tally.ftl: {e}"))?;
    }
    Ok(())
}
