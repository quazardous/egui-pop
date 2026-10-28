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
//! Tips also come by themselves ([`Tips::set_auto`]), chosen by rules
//! ([`pick`]: not dismissed, their command never used, their surface
//! holding, not shown in the last day, the one shown longest ago), as a
//! card that never takes the focus, beside the element a tip is about
//! (its `target`, named with [`mark`] or [`target`]) with a halo around
//! it. Commands `tips.show`, `tips.next`, `tips.gotIt`, `tips.notNow`
//! (Escape), `tips.off` and, for agents, `tips.current`.
//!
//! Tips are also declared in files, `tips/<id>.md` ([`file`]): a header
//! (`id`, `surface`, `command`, `target`, `contextual`), a `# Title` and
//! the text, read with [`Tips::add_files`] (the files' text, built into
//! the binary with `include_str!`) or [`Tips::add_dir`]. A text shows
//! `**bold**` and `` `code` ``; `{key}` is the key bound to the tip's
//! command in the keymap in force, `{key:<command>}` another command's
//! ([`markup`]), so a rebound key shows rebound.
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

mod browser;
pub mod file;
pub mod lint;
pub mod markup;
pub mod pick;
mod screen;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Mutex;

use egui::Ui;
use egui::accesskit::Role;
use egui_pop_core::icon::{Icon, Size, Tone};
use egui_pop_core::kit::{self, Kit};
use egui_pop_core::lock;
use egui_pop_core::strings;
use egui_pop_model::clock::SharedClock;
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::ids::{CommandId, OwnerId, PluginId, SettingId, StepId};
use egui_pop_model::keymap::Layer;
use egui_pop_model::values::{Change, Descriptor, Set, Value};
use egui_pop_model::when::{When, WhenContext};
use serde_json::{Value as Json, json};

use crate::app::AppHandle;
use crate::hooks::On;
use crate::host::Seq;
use crate::services::ServiceToken;
use crate::steps::{Outcome, StepSpec};
use crate::strings as txt;
use crate::zones::{self, Block};
use crate::{Ctx, Plugin, PluginError};
use egui_pop_model::ids::BlockId;

pub use browser::{browser, groups};
pub use screen::{halo_color, mark, target, target_rect, targets_seen};

/// The plugin's id.
pub const TIPS_PLUGIN: PluginId = PluginId::from_static("egui_pop.tips");

/// The tips registered, as a service.
pub const TIPS: ServiceToken<Tips> = ServiceToken::new("egui_pop.tips");

/// Whether tips are shown at all.
pub const SETTING_TIPS_SHOW: SettingId = SettingId::from_static("tips.show");
/// The tips not to show again: their ids, separated by commas.
pub const SETTING_TIPS_DISMISSED: SettingId = SettingId::from_static("tips.dismissed");
/// When each tip was shown last: `id@seconds` (since 1970), separated
/// by commas.
pub const SETTING_TIPS_SEEN: SettingId = SettingId::from_static("tips.seen");
/// The commands a person used: their ids, separated by commas.
pub const SETTING_TIPS_USED: SettingId = SettingId::from_static("tips.used");
/// The origins (plugins) whose tips are off, separated by commas.
pub const SETTING_TIPS_OFF: SettingId = SettingId::from_static("tips.off");

/// A tip is dismissed.
pub const STEP_TIPS_DISMISS: StepId = StepId::from_static("tips.dismiss");

pub const CMD_TIPS_RESET: CommandId = CommandId::from_static("tips.reset");
pub const CMD_TIPS_LIST: CommandId = CommandId::from_static("tips.list");
pub const CMD_TIPS_DISMISS: CommandId = CommandId::from_static("tips.dismiss");
pub const CMD_TIPS_SHOW: CommandId = CommandId::from_static("tips.show");
pub const CMD_TIPS_NEXT: CommandId = CommandId::from_static("tips.next");
pub const CMD_TIPS_GOT_IT: CommandId = CommandId::from_static("tips.gotIt");
pub const CMD_TIPS_NOT_NOW: CommandId = CommandId::from_static("tips.notNow");
pub const CMD_TIPS_OFF: CommandId = CommandId::from_static("tips.off");
pub const CMD_TIPS_CURRENT: CommandId = CommandId::from_static("tips.current");
pub const CMD_TIPS_BROWSE: CommandId = CommandId::from_static("tips.browse");
pub const CMD_TIPS_ORIGIN: CommandId = CommandId::from_static("tips.origin");

/// The context key set while a tip's card is shown.
pub const CONTEXT_TIP_SHOWN: &str = "tipShown";

/// How long after the start the first tip shows by itself, in seconds.
pub const FIRST_AFTER: f64 = 5.0;

/// The kind of declaration a tip's keys are derived from (`tip-<id>`).
pub const TIP_KIND: &str = "tip";

fn setting_path(id: &SettingId) -> String {
    format!("settings/{id}")
}

pub use file::TipError;

/// A tip: a title and a text, in English (translated by its id's keys).
#[derive(Clone, Debug, PartialEq)]
pub struct Tip {
    pub id: String,
    pub title: String,
    /// The text: paragraphs on lines of their own, `**bold**`,
    /// `` `code` ``, `{key}` and `{key:<command>}`.
    pub text: String,
    /// Shown only where [`hint`] is called, never as the tip of the day.
    pub contextual: bool,
    /// Where the tip makes sense, a `when` clause (empty: anywhere).
    pub surface: String,
    /// The command it teaches: `{key}` shows its binding.
    pub command: Option<CommandId>,
    /// The name of what it is about, as drawn ([`target`]).
    pub target: Option<String>,
    /// Who declared it, for grouping (empty: the id's first segment).
    pub origin: String,
}

impl Tip {
    pub fn new(id: &str, title: &str, text: &str) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            text: text.into(),
            contextual: false,
            surface: String::new(),
            command: None,
            target: None,
            origin: String::new(),
        }
    }

    /// Kept for [`hint`]: not in the tip of the day's turn.
    pub fn contextual(mut self) -> Self {
        self.contextual = true;
        self
    }

    /// Where the tip makes sense, a `when` clause.
    pub fn surface(mut self, when: &str) -> Self {
        self.surface = when.into();
        self
    }

    /// The command it teaches.
    pub fn command(mut self, command: CommandId) -> Self {
        self.command = Some(command);
        self
    }

    /// The name of what it is about ([`target`]).
    pub fn target(mut self, name: &str) -> Self {
        self.target = Some(name.into());
        self
    }

    /// Who declared it (a plugin's id).
    pub fn origin(mut self, origin: &str) -> Self {
        self.origin = origin.into();
        self
    }

    /// Who declared it: the origin given, else the id's first segment.
    pub fn origin_name(&self) -> &str {
        if self.origin.is_empty() {
            self.id.split('.').next().unwrap_or(&self.id)
        } else {
            &self.origin
        }
    }

    /// Reads a tip from a file's text ([`file`]).
    pub fn parse(file: &str, source: &str) -> Result<Tip, TipError> {
        file::parse(file, source)
    }

    /// The title in the interface's language.
    pub fn title_in(&self, ctx: &egui::Context) -> String {
        strings::declared(ctx, TIP_KIND, &self.id, Some("title"), &self.title)
    }

    /// The text in the interface's language, placeholders left.
    pub fn text_in(&self, ctx: &egui::Context) -> String {
        strings::declared(ctx, TIP_KIND, &self.id, Some("text"), &self.text)
    }

    /// The text as shown: in the interface's language, `{key}` and
    /// `{key:<command>}` replaced by the bindings in force (as
    /// `` `Ctrl+K` ``), a command without one by its title in bold. A
    /// translation writes them `{ $key }` and `{ $key-<command> }` (the
    /// command's dots as dashes), Fluent's own placeholders.
    pub fn text_shown(&self, ctx: &egui::Context, app: &AppHandle) -> String {
        let shown = |c: &str| CommandId::new(c).ok().and_then(|c| key_text(ctx, app, &c));
        let own = self.command.as_ref().and_then(|c| shown(c.as_str()));
        let mut args: Vec<(String, String)> = Vec::new();
        if let Some(k) = &own {
            args.push(("key".into(), k.clone()));
        }
        for c in markup::named_commands(&self.text) {
            if let Some(k) = shown(&c) {
                args.push((format!("key-{}", strings::slug(&c)), k));
            }
        }
        let args: Vec<(&str, &str)> = args.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        let text = strings::lookup_args(
            ctx,
            &strings::declared_key(TIP_KIND, &self.id, Some("text")),
            &self.text,
            &args,
        );
        markup::resolve(&text, own.as_deref(), shown)
    }

    fn to_json(&self, dismissed: bool) -> Json {
        json!({
            "id": self.id,
            "title": self.title,
            "text": self.text,
            "contextual": self.contextual,
            "surface": self.surface,
            "command": self.command,
            "target": self.target,
            "origin": self.origin_name(),
            "dismissed": dismissed,
        })
    }
}

/// A command's key as a tip shows it: its first binding in force as
/// code, else its title in bold; `None` for a command not declared.
fn key_text(ctx: &egui::Context, app: &AppHandle, id: &CommandId) -> Option<String> {
    if let Some(k) = app.keys_for(id).into_iter().next() {
        return Some(format!("`{k}`"));
    }
    let (spec, _) = app.commands().into_iter().find(|(s, _)| s.id == *id)?;
    let title = strings::declared(
        ctx,
        strings::kind::COMMAND,
        id.as_str(),
        Some("title"),
        &spec.title,
    );
    Some(format!("**{title}**"))
}

#[derive(Debug, Default)]
struct State {
    tips: Vec<Tip>,
    /// Which tip of the day shows, among those left.
    at: usize,
    /// The tip of the day closed for this session.
    closed: bool,
    /// The tip shown as a card by itself (or by `tips.show`).
    current: Option<String>,
    /// Tips show by themselves.
    auto: bool,
    /// The interface's time at the first pass.
    started: Option<f64>,
    /// The first tip by itself came (or its time passed).
    first_done: bool,
    /// The surfaces entered in this session.
    entered: BTreeSet<String>,
    /// The journal read up to here, for the commands used.
    read_to: Seq,
    clock: Clock,
}

/// Where "now" comes from: the system's time, or a test's clock.
#[derive(Default)]
struct Clock(Option<SharedClock>);

impl std::fmt::Debug for Clock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.0.is_some() {
            "Clock(set)"
        } else {
            "Clock(system)"
        })
    }
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

    /// Registers tips from files' text, `(name, text)` (`photos.rate.md`
    /// and its text, built in with `include_str!`), as declared by
    /// `origin` (a plugin's id). Returns the files that could not be read;
    /// the others are registered.
    pub fn add_files(&self, origin: &str, files: &[(&str, &str)]) -> Vec<TipError> {
        let mut errors = Vec::new();
        for (name, text) in files {
            match file::parse(name, text) {
                Ok(t) => self.add(t.origin(origin)),
                Err(e) => errors.push(e),
            }
        }
        errors
    }

    /// Registers the tips of a folder's `*.md` files (a plugin's `tips/`
    /// folder shipped beside it), as declared by `origin`. Returns the
    /// files that could not be read.
    pub fn add_dir(&self, origin: &str, dir: &Path) -> Vec<TipError> {
        let (tips, errors) = file::read_dir(dir);
        for t in tips {
            self.add(t.origin(origin));
        }
        errors
    }

    /// Shows tips by themselves, as cards: once [`FIRST_AFTER`] seconds
    /// after the start, then the first time a tip's surface is entered.
    /// Off by default: the application turns it on.
    pub fn set_auto(&self, on: bool) {
        lock(&self.state).auto = on;
    }

    /// Whether tips show by themselves.
    pub fn auto(&self) -> bool {
        lock(&self.state).auto
    }

    /// Reads "now" from a clock (seconds since 1970), for tests.
    pub fn set_clock(&self, clock: SharedClock) {
        lock(&self.state).clock = Clock(Some(clock));
    }

    /// Now, in seconds since 1970.
    pub fn now(&self) -> f64 {
        if let Some(c) = &lock(&self.state).clock.0 {
            return c.now();
        }
        web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .map_or(0.0, |d| d.as_secs_f64())
    }

    fn text_setting(app: &AppHandle, id: &SettingId) -> String {
        lock(&app.store())
            .get(&setting_path(id))
            .and_then(|e| e.value.as_text().map(str::to_string))
            .unwrap_or_default()
    }

    fn write_setting(app: &AppHandle, id: &SettingId, text: String) -> Result<(), String> {
        lock(&app.store())
            .set(
                Set::new(
                    &setting_path(id),
                    Change::To(Value::Text(text)),
                    Origin::User,
                )
                .by(&OwnerId::FRAMEWORK)
                .untracked(),
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    /// The commands a person used (the `tips.used` setting).
    pub fn used(&self, app: &AppHandle) -> BTreeSet<String> {
        pick::read_ids(&Self::text_setting(app, &SETTING_TIPS_USED))
    }

    /// Records commands as used: their tips are not picked any more.
    pub fn mark_used<'a>(
        &self,
        app: &AppHandle,
        commands: impl IntoIterator<Item = &'a str>,
    ) -> Result<(), String> {
        let mut used = self.used(app);
        let before = used.len();
        used.extend(commands.into_iter().map(String::from));
        if used.len() == before {
            return Ok(());
        }
        Self::write_setting(app, &SETTING_TIPS_USED, pick::write_ids(&used))
    }

    /// When each tip was shown last (the `tips.seen` setting).
    pub fn seen(&self, app: &AppHandle) -> BTreeMap<String, f64> {
        pick::read_seen(&Self::text_setting(app, &SETTING_TIPS_SEEN))
    }

    fn mark_seen(&self, app: &AppHandle, id: &str) {
        let mut seen = self.seen(app);
        seen.insert(id.to_string(), self.now());
        if let Err(e) = Self::write_setting(app, &SETTING_TIPS_SEEN, pick::write_seen(&seen)) {
            log::warn!("tips: {e}");
        }
    }

    /// The origins whose tips are off (the `tips.off` setting).
    pub fn off(&self, app: &AppHandle) -> BTreeSet<String> {
        pick::read_ids(&Self::text_setting(app, &SETTING_TIPS_OFF))
    }

    /// Turns an origin's tips (a plugin's) on or off, the others kept.
    pub fn set_origin_on(&self, app: &AppHandle, origin: &str, on: bool) -> Result<(), String> {
        let mut off = self.off(app);
        let changed = if on {
            off.remove(origin)
        } else {
            off.insert(origin.to_string())
        };
        if !changed {
            return Ok(());
        }
        Self::write_setting(app, &SETTING_TIPS_OFF, pick::write_ids(&off))?;
        if !on && self.current().is_some_and(|t| t.origin_name() == origin) {
            self.set_current(app, None);
        }
        Ok(())
    }

    /// The tip the rules pick in a context, among those `keep` accepts
    /// ([`pick`]); none when tips are off.
    pub fn pick(
        &self,
        app: &AppHandle,
        cx: &WhenContext,
        keep: impl Fn(&Tip) -> bool,
    ) -> Option<Tip> {
        if !self.enabled(app) {
            return None;
        }
        let (dismissed, used, seen, off) = (
            self.dismissed(app),
            self.used(app),
            self.seen(app),
            self.off(app),
        );
        let holds = |w: &str| When::parse(w).is_ok_and(|w| w.eval(cx));
        let rules = pick::Rules {
            dismissed: &dismissed,
            used: &used,
            seen: &seen,
            off: &off,
            now: self.now(),
            holds: &holds,
        };
        let tips = self.all();
        rules.pick(&tips, keep).cloned()
    }

    /// Why a tip would not be picked now, or `None` when it would.
    pub fn refusal(&self, app: &AppHandle, id: &str) -> Option<String> {
        let tip = self.get(id)?;
        if !self.enabled(app) {
            return Some("tips are off".into());
        }
        let cx = app.when_context();
        let (dismissed, used, seen, off) = (
            self.dismissed(app),
            self.used(app),
            self.seen(app),
            self.off(app),
        );
        let holds = |w: &str| When::parse(w).is_ok_and(|w| w.eval(&cx));
        pick::Rules {
            dismissed: &dismissed,
            used: &used,
            seen: &seen,
            off: &off,
            now: self.now(),
            holds: &holds,
        }
        .refusal(&tip)
        .map(String::from)
    }

    /// The tip shown as a card, if any.
    pub fn current(&self) -> Option<Tip> {
        let id = lock(&self.state).current.clone()?;
        self.get(&id)
    }

    fn set_current(&self, app: &AppHandle, id: Option<String>) {
        let shown = id.is_some();
        lock(&self.state).current = id;
        app.set_context(CONTEXT_TIP_SHOWN, if shown { "true" } else { "false" });
        app.egui().request_repaint();
    }

    /// Shows a tip as a card, whatever the rules say, and records it as
    /// shown now.
    pub fn show(&self, app: &AppHandle, id: &str) -> Result<Tip, String> {
        let tip = self.get(id).ok_or_else(|| format!("no tip `{id}`"))?;
        self.mark_seen(app, id);
        self.set_current(app, Some(id.to_string()));
        app.publish("app", None, "tip.shown", json!({ "tip": id }));
        Ok(tip)
    }

    /// Shows the tip the rules pick in the current context (among those
    /// `keep` accepts); closes the card when there is none.
    pub fn show_next_where(&self, app: &AppHandle, keep: impl Fn(&Tip) -> bool) -> Option<Tip> {
        let cx = app.when_context();
        match self.pick(app, &cx, keep) {
            Some(t) => self.show(app, &t.id).ok(),
            None => {
                self.set_current(app, None);
                None
            }
        }
    }

    /// Shows the next tip the rules pick (the card's "Next tip"): the one
    /// shown was recorded, so it is not picked again today.
    pub fn show_next(&self, app: &AppHandle) -> Option<Tip> {
        self.show_next_where(app, |_| true)
    }

    /// Closes the card for now ("Not now", Escape): the tip may come
    /// back another day.
    pub fn not_now(&self, app: &AppHandle) {
        self.set_current(app, None);
    }

    /// Dismisses the tip shown for good ("Got it") and closes the card.
    pub fn got_it(&self, app: &AppHandle) -> Result<(), String> {
        let id = lock(&self.state).current.clone().ok_or("no tip is shown")?;
        self.dismiss(app, &id)?;
        self.set_current(app, None);
        Ok(())
    }

    /// Turns every tip off (the `tips.show` setting) and closes the card.
    pub fn turn_off(&self, app: &AppHandle) -> Result<(), String> {
        lock(&app.store())
            .set(
                Set::new(
                    &setting_path(&SETTING_TIPS_SHOW),
                    Change::To(Value::Bool(false)),
                    Origin::User,
                )
                .by(&OwnerId::FRAMEWORK)
                .untracked(),
            )
            .map_err(|e| e.to_string())?;
        self.set_current(app, None);
        Ok(())
    }

    /// Reads the journal for the commands a person ran since the last
    /// call (a person: the user or a device, not an agent or a script).
    fn read_journal(&self, app: &AppHandle) {
        let from = lock(&self.state).read_to;
        let events = app.events_since(from);
        let Some(last) = events.last().map(|e| e.seq) else {
            return;
        };
        lock(&self.state).read_to = last;
        let ran: Vec<&str> = events
            .iter()
            .filter(|e| {
                e.name == "command.done" && (e.source == "user" || e.source.starts_with("device:"))
            })
            .filter_map(|e| e.data["command"].as_str())
            .collect();
        if !ran.is_empty()
            && let Err(e) = self.mark_used(app, ran)
        {
            log::warn!("tips: {e}");
        }
    }

    /// Each pass: the commands used, then a tip by itself when its
    /// moment comes.
    fn tick(&self, app: &AppHandle) {
        self.read_journal(app);
        if !self.auto() || !self.enabled(app) || lock(&self.state).current.is_some() {
            return;
        }
        let time = app.egui().input(|i| i.time);
        let started = *lock(&self.state).started.get_or_insert(time);
        let cx = app.when_context();
        if cx.get("modalOpen") == "true" {
            return;
        }
        let surfaces: BTreeSet<String> = lock(&self.state)
            .tips
            .iter()
            .filter(|t| !t.contextual && !t.surface.is_empty())
            .map(|t| t.surface.clone())
            .collect();
        let entered: Vec<String> = {
            let mut s = lock(&self.state);
            let new: Vec<String> = surfaces
                .into_iter()
                .filter(|w| !s.entered.contains(w) && When::parse(w).is_ok_and(|c| c.eval(&cx)))
                .collect();
            s.entered.extend(new.iter().cloned());
            new
        };
        if !lock(&self.state).first_done {
            let left = started + FIRST_AFTER - time;
            if left > 0.0 {
                app.egui()
                    .request_repaint_after(std::time::Duration::from_secs_f64(left));
                return;
            }
            lock(&self.state).first_done = true;
            self.show_next(app);
        } else if !entered.is_empty() {
            self.show_next_where(app, |t| entered.contains(&t.surface));
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
                .by(&OwnerId::FRAMEWORK)
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
        let off = self.off(app);
        lock(&self.state)
            .tips
            .iter()
            .filter(|t| {
                !t.contextual && !dismissed.contains(&t.id) && !off.contains(t.origin_name())
            })
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
        ctx.setting(
            SETTING_TIPS_OFF,
            Descriptor::text("Plugins without tips", "").description(
                "The plugins whose tips are off, by id; the others still show theirs.",
            ),
        )?;
        ctx.setting(
            SETTING_TIPS_SEEN,
            Descriptor::text("Tips shown", "").description(
                "When each tip was shown last, so it does not show again the same day.",
            ),
        )?;
        ctx.setting(
            SETTING_TIPS_USED,
            Descriptor::text("Commands used", "")
                .description("The commands already used: their tips are not shown by themselves."),
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
        let shown = CONTEXT_TIP_SHOWN;
        ctx.command(
            CommandSpec::new(CMD_TIPS_SHOW, "Show a tip", CAT)
                .description("Shows the tip the rules pick here, as a card beside what it is about; `tip`, an id, shows that one.")
                .args(json!({ "tip": "photos.rate" })),
            |call| {
                let tips = service(call.app)?;
                let tip = match call.args.get("tip").and_then(Json::as_str) {
                    Some(id) => Some(tips.show(call.app, id)?),
                    None => tips.show_next(call.app),
                };
                Ok(json!({ "shown": tip.map(|t| t.id) }))
            },
        )?
        .command(
            CommandSpec::new(CMD_TIPS_NEXT, "Next tip", CAT)
                .description("Shows the next tip the rules pick in place of the one shown.")
                .when(shown),
            |call| {
                let tip = service(call.app)?.show_next(call.app);
                Ok(json!({ "shown": tip.map(|t| t.id) }))
            },
        )?
        .command(
            CommandSpec::new(CMD_TIPS_GOT_IT, "Got it", CAT)
                .description("Dismisses the tip shown for good.")
                .when(shown),
            |call| {
                service(call.app)?.got_it(call.app)?;
                Ok(Json::Null)
            },
        )?
        .command(
            CommandSpec::new(CMD_TIPS_NOT_NOW, "Not now", CAT)
                .description("Closes the tip shown; it may show again another day.")
                .when(shown)
                .hidden(),
            |call| {
                service(call.app)?.not_now(call.app);
                Ok(Json::Null)
            },
        )?
        .command(
            CommandSpec::new(CMD_TIPS_OFF, "Turn tips off", CAT)
                .description("Shows no more tips (the tips.show setting)."),
            |call| {
                service(call.app)?.turn_off(call.app)?;
                Ok(Json::Null)
            },
        )?
        .command(
            CommandSpec::new(CMD_TIPS_CURRENT, "The tip shown", CAT)
                .description("The tip shown as a card (its id, title, text and target, null when none), and where its card and its target are.")
                .hidden(),
            |call| {
                let tips = service(call.app)?;
                Ok(match tips.current() {
                    Some(t) => screen::describe(call.app, &t),
                    None => Json::Null,
                })
            },
        )?;
        ctx.command(
            CommandSpec::new(CMD_TIPS_BROWSE, "Tips…", CAT)
                .description("Every tip, by the plugin that declared it, each plugin's with its own switch.")
                .menu(crate::app::MENU_HELP, "1_tips", 0),
            |call| {
                let dialogs = call
                    .app
                    .service(&crate::dialogs::DIALOGS)
                    .ok_or("the dialogs plugin is not connected")?;
                let title = strings::tr(&call.app.egui(), &txt::TIPS_BROWSER);
                let id = dialogs.open_custom(
                    call.app,
                    crate::dialogs::Custom::closing(&title),
                    |ui, app, _| {
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| browser::browser(ui, app));
                    },
                )?;
                Ok(json!({ "dialog": id }))
            },
        )?
        .command(
            CommandSpec::new(CMD_TIPS_ORIGIN, "Turn a plugin's tips on or off", CAT)
                .description("Turns the tips of one plugin (`origin`, its id) on or off (`on`), the others kept.")
                .hidden()
                .args(json!({ "origin": "photos", "on": false })),
            |call| {
                let origin = call
                    .args
                    .get("origin")
                    .and_then(Json::as_str)
                    .ok_or("`origin` is missing")?;
                let on = call
                    .args
                    .get("on")
                    .and_then(Json::as_bool)
                    .ok_or("`on` is missing")?;
                service(call.app)?.set_origin_on(call.app, origin, on)?;
                Ok(json!({ "origin": origin, "on": on }))
            },
        )?;
        // Escape closes the card, unless something else wants it.
        ctx.keybinding(
            "Escape",
            CMD_TIPS_NOT_NOW,
            "tipShown && !modePushed && !overlayOpen && !textInputFocus && !workbenchZen",
            Layer::Default,
        )?;
        ctx.app().set_context(CONTEXT_TIP_SHOWN, "false");
        ctx.on(On::Pass, |app, _| {
            if let Some(tips) = app.service(&TIPS) {
                tips.tick(app);
            }
        });
        // Drawn in every window, shown in the active one.
        ctx.block(
            zones::WINDOW_OVERLAYS,
            Block::add(BlockId::from_static("egui_pop.tips"), 19, screen::show),
        );
        Ok(())
    }
}

/// The tips' service, or why it is missing.
fn service(app: &AppHandle) -> Result<std::sync::Arc<Tips>, String> {
    app.service(&TIPS)
        .ok_or_else(|| "the tips plugin is not connected".to_string())
}

/// A tip's card: an icon, a heading, the text (bold and code drawn),
/// then the buttons `actions` draws: a note for screen readers.
fn card(ui: &mut Ui, heading: &str, title: &str, text: &str, actions: impl FnOnce(&mut Ui)) {
    card_in(
        ui,
        egui::Shadow::NONE,
        heading,
        title,
        text,
        None::<fn(&mut Ui)>,
        actions,
    );
}

/// [`card`] with a shadow, and `corner` drawn at the end of the heading
/// (a close button).
fn card_in(
    ui: &mut Ui,
    shadow: egui::Shadow,
    heading: &str,
    title: &str,
    text: &str,
    corner: Option<impl FnOnce(&mut Ui)>,
    actions: impl FnOnce(&mut Ui),
) {
    Kit::of(ui.ctx())
        .tip_card
        .show(ui, kit::State::REST, shadow, |ui| {
            ui.ctx().accesskit_node_builder(ui.unique_id(), |node| {
                node.set_role(Role::Note);
            });
            ui.horizontal(|ui| {
                ui.add(Icon::Info.size(Size::Inline).tone(Tone::Info));
                ui.weak(heading);
                if let Some(corner) = corner {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), corner);
                }
            });
            ui.strong(title);
            let job = markup::job(text, ui.style(), ui.available_width());
            ui.label(job);
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
        &tip.text_shown(&ctx, app),
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
    if !tips.enabled(app) || tips.is_dismissed(app, id) || tips.off(app).contains(tip.origin_name())
    {
        return false;
    }
    let ctx = ui.ctx().clone();
    let heading = strings::tr(&ctx, &txt::TIPS_HINT);
    card(
        ui,
        &heading,
        &tip.title_in(&ctx),
        &tip.text_shown(&ctx, app),
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
