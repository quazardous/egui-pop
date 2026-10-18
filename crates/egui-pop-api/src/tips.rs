//! Tips, as a plugin ([`TipsPlugin`]): a tip of the day and contextual
//! hints, which plugins register ([`Tips::add`]), each dismissible and
//! remembered in the settings (`tips.dismissed`, the ids of the tips not
//! to show again; `tips.show` turns them all off).
//!
//! A tip's title and text are translated by keys derived from its id,
//! `tip-<id>.title` and `tip-<id>.text` (the id's dots become dashes), in
//! the application's catalogues; the English given is the fallback.
//!
//! - [`tip_of_the_day`] draws a card with the next tip not dismissed:
//!   the previous and next ones, "Don't show again", close (for this
//!   session);
//! - [`hint`] draws one contextual tip where it is called, beside what
//!   it is about, with "Got it".
//!
//! | Step | Arguments | Default |
//! |---|---|---|
//! | `tips.dismiss` | `tip`, `cause` (`user`, `agent`) | adds it to `tips.dismissed` |
//!
//! Commands: `tips.reset` (every tip shown again, in the Help menu's
//! category) and, for agents, `tips.list` and `tips.dismiss` (`tip`).
//!
//! Owned: egui has tooltips (`on_hover_text`), which stay as they are;
//! no crate offers remembered, translated tips (see docs/tips.md).
//!
//! ```
//! use egui_pop_api::tips::{TIPS, Tip, TipsPlugin};
//! use egui_pop_api::App;
//!
//! let app = App::builder()
//!     .plugin(TipsPlugin)
//!     .build(&egui::Context::default())
//!     .unwrap();
//! let tips = app.handle().service(&TIPS).unwrap();
//! tips.add(Tip::new("photos.rate", "Rate quickly", "Press 1 to 5 to rate the selected photos."));
//! assert_eq!(tips.of_the_day(app.handle()).map(|t| t.id), Some("photos.rate".into()));
//! tips.dismiss(app.handle(), "photos.rate").unwrap();
//! assert!(tips.of_the_day(app.handle()).is_none());
//! ```

use std::collections::BTreeSet;
use std::sync::Mutex;

use egui::Ui;
use egui::accesskit::Role;
use egui_pop_core::icon::{Icon, Size, Tone};
use egui_pop_core::kit::{self, Kit};
use egui_pop_core::lock;
use egui_pop_core::strings;
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::ids::{CommandId, PluginId, SettingId, StepId};
use egui_pop_model::values::{Change, Descriptor, Set, Value};
use serde_json::{Value as Json, json};

use crate::app::AppHandle;
use crate::services::ServiceToken;
use crate::steps::{Outcome, StepSpec};
use crate::strings as txt;
use crate::{Ctx, Plugin, PluginError};

/// The plugin's id.
pub const TIPS_PLUGIN: PluginId = PluginId::from_static("egui_pop.tips");

/// The tips registered, as a service.
pub const TIPS: ServiceToken<Tips> = ServiceToken::new("egui_pop.tips");

/// Whether tips are shown at all.
pub const SETTING_TIPS_SHOW: SettingId = SettingId::from_static("tips.show");
/// The tips not to show again: their ids, separated by commas.
pub const SETTING_TIPS_DISMISSED: SettingId = SettingId::from_static("tips.dismissed");

/// A tip is dismissed.
pub const STEP_TIPS_DISMISS: StepId = StepId::from_static("tips.dismiss");

pub const CMD_TIPS_RESET: CommandId = CommandId::from_static("tips.reset");
pub const CMD_TIPS_LIST: CommandId = CommandId::from_static("tips.list");
pub const CMD_TIPS_DISMISS: CommandId = CommandId::from_static("tips.dismiss");

/// The kind of declaration a tip's keys are derived from (`tip-<id>`).
pub const TIP_KIND: &str = "tip";

fn setting_path(id: &SettingId) -> String {
    format!("settings/{id}")
}

/// A tip: a title and a text, in English (translated by its id's keys).
#[derive(Clone, Debug, PartialEq)]
pub struct Tip {
    pub id: String,
    pub title: String,
    pub text: String,
    /// Shown only where [`hint`] is called, never as the tip of the day.
    pub contextual: bool,
}

impl Tip {
    pub fn new(id: &str, title: &str, text: &str) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            text: text.into(),
            contextual: false,
        }
    }

    /// Kept for [`hint`]: not in the tip of the day's turn.
    pub fn contextual(mut self) -> Self {
        self.contextual = true;
        self
    }

    /// The title in the interface's language.
    pub fn title_in(&self, ctx: &egui::Context) -> String {
        strings::declared(ctx, TIP_KIND, &self.id, Some("title"), &self.title)
    }

    /// The text in the interface's language.
    pub fn text_in(&self, ctx: &egui::Context) -> String {
        strings::declared(ctx, TIP_KIND, &self.id, Some("text"), &self.text)
    }

    fn to_json(&self, dismissed: bool) -> Json {
        json!({
            "id": self.id,
            "title": self.title,
            "text": self.text,
            "contextual": self.contextual,
            "dismissed": dismissed,
        })
    }
}

#[derive(Debug, Default)]
struct State {
    tips: Vec<Tip>,
    /// Which tip of the day shows, among those left.
    at: usize,
    /// The tip of the day closed for this session.
    closed: bool,
}

/// The tips registered, and the tip of the day's place.
#[derive(Debug, Default)]
pub struct Tips {
    state: Mutex<State>,
}

impl Tips {
    /// Registers a tip (replacing one of the same id).
    pub fn add(&self, tip: Tip) {
        let mut s = lock(&self.state);
        match s.tips.iter_mut().find(|t| t.id == tip.id) {
            Some(t) => *t = tip,
            None => s.tips.push(tip),
        }
    }

    /// Every tip, in the order registered.
    pub fn all(&self) -> Vec<Tip> {
        lock(&self.state).tips.clone()
    }

    pub fn get(&self, id: &str) -> Option<Tip> {
        lock(&self.state).tips.iter().find(|t| t.id == id).cloned()
    }

    /// Whether tips are shown (the `tips.show` setting).
    pub fn enabled(&self, app: &AppHandle) -> bool {
        lock(&app.store())
            .get(&setting_path(&SETTING_TIPS_SHOW))
            .and_then(|e| e.value.as_bool())
            .unwrap_or(true)
    }

    /// The ids of the tips dismissed (the `tips.dismissed` setting).
    pub fn dismissed(&self, app: &AppHandle) -> BTreeSet<String> {
        lock(&app.store())
            .get(&setting_path(&SETTING_TIPS_DISMISSED))
            .and_then(|e| e.value.as_text().map(str::to_string))
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect()
    }

    pub fn is_dismissed(&self, app: &AppHandle, id: &str) -> bool {
        self.dismissed(app).contains(id)
    }

    fn write_dismissed(&self, app: &AppHandle, ids: &BTreeSet<String>) -> Result<(), String> {
        let text = ids.iter().cloned().collect::<Vec<_>>().join(",");
        lock(&app.store())
            .set(
                Set::new(
                    &setting_path(&SETTING_TIPS_DISMISSED),
                    Change::To(Value::Text(text)),
                    Origin::User,
                )
                .untracked(),
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    /// Dismisses a tip for good, through the `tips.dismiss` step.
    pub fn dismiss(&self, app: &AppHandle, id: &str) -> Result<(), String> {
        self.dismiss_by(app, id, "user")
    }

    fn dismiss_by(&self, app: &AppHandle, id: &str, cause: &str) -> Result<(), String> {
        if self.get(id).is_none() {
            return Err(format!("no tip `{id}`"));
        }
        let args = json!({ "tip": id, "cause": cause });
        match app.step(&STEP_TIPS_DISMISS, args, |app, a| {
            let tips = app
                .service(&TIPS)
                .ok_or("the tips plugin is not connected")?;
            let tip = a["tip"].as_str().ok_or("`tip` is missing")?;
            let mut ids = tips.dismissed(app);
            ids.insert(tip.to_string());
            tips.write_dismissed(app, &ids)?;
            Ok(json!(tip))
        }) {
            Outcome::Done(r) => r.map(|_| ()),
            Outcome::Vetoed { by, reason } => Err(format!("vetoed by {by}: {reason}")),
        }
    }

    /// Shows every tip again.
    pub fn reset(&self, app: &AppHandle) -> Result<(), String> {
        lock(&self.state).closed = false;
        self.write_dismissed(app, &BTreeSet::new())
    }

    /// The tips of the day still to show, in order.
    fn left(&self, app: &AppHandle) -> Vec<Tip> {
        let dismissed = self.dismissed(app);
        lock(&self.state)
            .tips
            .iter()
            .filter(|t| !t.contextual && !dismissed.contains(&t.id))
            .cloned()
            .collect()
    }

    /// The tip of the day: the one in turn among those not dismissed;
    /// none when tips are off, all dismissed or closed for the session.
    pub fn of_the_day(&self, app: &AppHandle) -> Option<Tip> {
        if !self.enabled(app) || lock(&self.state).closed {
            return None;
        }
        let left = self.left(app);
        let at = lock(&self.state).at;
        (!left.is_empty()).then(|| left[at % left.len()].clone())
    }

    /// Turns to the next tip of the day (`back`: the previous one).
    pub fn turn(&self, app: &AppHandle, back: bool) {
        let n = self.left(app).len().max(1);
        let mut s = lock(&self.state);
        s.at = if back {
            (s.at % n + n - 1) % n
        } else {
            (s.at + 1) % n
        };
    }

    /// Closes the tip of the day for this session.
    pub fn close(&self) {
        lock(&self.state).closed = true;
    }
}

/// Declares the tips' service, settings, step and commands.
#[derive(Debug, Default)]
pub struct TipsPlugin;

impl Plugin for TipsPlugin {
    fn id(&self) -> PluginId {
        TIPS_PLUGIN
    }

    fn title(&self) -> String {
        "Tips".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        const CAT: &str = "Help";
        ctx.service(&TIPS, Tips::default())?;
        ctx.setting(
            SETTING_TIPS_SHOW,
            Descriptor::bool("Show tips", true)
                .description("The tip of the day and the hints beside what they are about."),
        )?;
        ctx.setting(
            SETTING_TIPS_DISMISSED,
            Descriptor::text("Dismissed tips", "")
                .description("The tips not to show again, by id; empty to show them all again."),
        )?;
        ctx.step(
            StepSpec::new(
                STEP_TIPS_DISMISS,
                "A tip is dismissed for good (`tip`, `cause`: user or agent); a veto keeps it showing.",
            )
            .args(json!({ "tip": "photos.rate", "cause": "user" })),
        )?;
        ctx.command(
            CommandSpec::new(CMD_TIPS_RESET, "Show every tip again", CAT)
                .description("Forgets which tips were dismissed."),
            |call| {
                let tips = call
                    .app
                    .service(&TIPS)
                    .ok_or("the tips plugin is not connected")?;
                tips.reset(call.app)?;
                Ok(Json::Null)
            },
        )?;
        ctx.command(
            CommandSpec::new(CMD_TIPS_LIST, "List the tips", CAT)
                .description("Every tip registered: its id, English title and text, whether it is contextual and whether it was dismissed.")
                .hidden(),
            |call| {
                let tips = call
                    .app
                    .service(&TIPS)
                    .ok_or("the tips plugin is not connected")?;
                let dismissed = tips.dismissed(call.app);
                Ok(json!(
                    tips.all()
                        .iter()
                        .map(|t| t.to_json(dismissed.contains(&t.id)))
                        .collect::<Vec<_>>()
                ))
            },
        )?;
        ctx.command(
            CommandSpec::new(CMD_TIPS_DISMISS, "Dismiss a tip", CAT)
                .description(
                    "Dismisses a tip for good (`tip`, its id), through the tips.dismiss step.",
                )
                .hidden()
                .args(json!({ "tip": "photos.rate" })),
            |call| {
                let tips = call
                    .app
                    .service(&TIPS)
                    .ok_or("the tips plugin is not connected")?;
                let id = call
                    .args
                    .get("tip")
                    .and_then(Json::as_str)
                    .ok_or("`tip` is missing")?;
                tips.dismiss_by(call.app, id, "agent")?;
                Ok(json!({ "dismissed": id }))
            },
        )?;
        Ok(())
    }
}

/// A tip's card: an icon, a heading, the text, then the buttons
/// `actions` draws: a note for screen readers.
fn card(ui: &mut Ui, heading: &str, title: &str, text: &str, actions: impl FnOnce(&mut Ui)) {
    Kit::of(ui.ctx())
        .tip_card
        .show(ui, kit::State::REST, egui::Shadow::NONE, |ui| {
            ui.ctx().accesskit_node_builder(ui.unique_id(), |node| {
                node.set_role(Role::Note);
            });
            ui.horizontal(|ui| {
                ui.add(Icon::Info.size(Size::Inline).tone(Tone::Info));
                ui.weak(heading);
            });
            ui.strong(title);
            ui.label(text);
            ui.horizontal_wrapped(actions);
        });
}

/// Draws the tip of the day, when there is one to show (it needs
/// [`TipsPlugin`]). Returns whether a card was drawn.
pub fn tip_of_the_day(ui: &mut Ui, app: &AppHandle) -> bool {
    let Some(tips) = app.service(&TIPS) else {
        return false;
    };
    let Some(tip) = tips.of_the_day(app) else {
        return false;
    };
    let ctx = ui.ctx().clone();
    let left = tips.left(app).len();
    let at = lock(&tips.state).at % left.max(1) + 1;
    let heading = strings::tr(&ctx, &txt::TIPS_OF_THE_DAY);
    card(
        ui,
        &heading,
        &tip.title_in(&ctx),
        &tip.text_in(&ctx),
        |ui| {
            let several = left > 1;
            if ui
                .add_enabled(
                    several,
                    egui::Button::new(strings::tr(&ctx, &txt::TIPS_PREVIOUS)),
                )
                .clicked()
            {
                tips.turn(app, true);
            }
            if ui
                .add_enabled(
                    several,
                    egui::Button::new(strings::tr(&ctx, &txt::TIPS_NEXT)),
                )
                .clicked()
            {
                tips.turn(app, false);
            }
            ui.weak(strings::tr_args(
                &ctx,
                &txt::TIPS_AT,
                &[("at", &at.to_string()), ("count", &left.to_string())],
            ));
            if ui.button(strings::tr(&ctx, &txt::TIPS_DONT_SHOW)).clicked()
                && let Err(e) = tips.dismiss(app, &tip.id)
            {
                log::warn!("tips: {e}");
            }
            if ui.button(strings::tr(&ctx, &strings::CLOSE)).clicked() {
                tips.close();
            }
        },
    );
    true
}

/// Draws a contextual tip where it is called (it needs [`TipsPlugin`]),
/// unless tips are off or it was dismissed. Returns whether it was
/// drawn.
pub fn hint(ui: &mut Ui, app: &AppHandle, id: &str) -> bool {
    let Some(tips) = app.service(&TIPS) else {
        return false;
    };
    let Some(tip) = tips.get(id) else {
        return false;
    };
    if !tips.enabled(app) || tips.is_dismissed(app, id) {
        return false;
    }
    let ctx = ui.ctx().clone();
    let heading = strings::tr(&ctx, &txt::TIPS_HINT);
    card(
        ui,
        &heading,
        &tip.title_in(&ctx),
        &tip.text_in(&ctx),
        |ui| {
            let got = strings::tr(&ctx, &txt::TIPS_GOT_IT);
            let name = strings::tr_args(
                &ctx,
                &txt::TIPS_GOT_IT_NAME,
                &[("tip", &tip.title_in(&ctx))],
            );
            let r = ui.button(&got);
            egui_pop_core::a11y::name(&r, egui::WidgetType::Button, &name);
            if r.clicked()
                && let Err(e) = tips.dismiss(app, id)
            {
                log::warn!("tips: {e}");
            }
        },
    );
    true
}
