//! The plugin: [`I18nPlugin`] connects an [`I18n`] to an application.
//!
//! - It installs the interface's strings provider
//!   (`egui_pop_core::strings`), so egui-pop's own text and the titles of
//!   what plugins declare are translated, and keeps it current.
//! - It declares the `appearance.language` setting (`system`, then each
//!   locale with a catalogue, shown by its own name), kept in the host's
//!   store like every setting. When it changes, the locale switches, the
//!   `locale.changed` signal (`{ "from", "to", "setting" }`) goes to the
//!   host's journal, and the next frame shows the new language. `system`
//!   reads an environment variable first when one is named
//!   ([`I18nPlugin::env`]), then the system's language.
//! - It declares the `i18n.resolve` step (ADR 0011): every lookup a
//!   plugin hooks runs through it, with `{ "key", "locale", "args" }`; a
//!   before-hook may redirect it (another key or locale), a run-hook
//!   replace it (its result is the text, `null` for none), an after-hook
//!   observe it. Without hooks, lookups do not run the step. With hooks,
//!   each distinct lookup runs it once, at the next frame, and its answer
//!   is kept until the locale or the catalogues change. [`ShowKeys`] is
//!   such a hook: it shows keys instead of text.
//! - It adds the `i18n` debug collector: the locale, its chain, the
//!   locales, the keys missing per locale; actions `set_locale` and
//!   `clear_missing`.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError, RwLock};

use egui_pop_api::hooks::On;
use egui_pop_api::steps::{Outcome, StepSpec};
use egui_pop_api::zones::{self, Block};
use egui_pop_api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop_core::debug::FnCollector;
use egui_pop_core::lock;
use egui_pop_core::strings;
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::{BlockId, PluginId, SettingId, StepId};
use egui_pop_model::values::{Change, Descriptor, Set, Value};
use serde_json::{Value as Json, json};

use crate::i18n::{Catalogue, I18n, Provider, json_args};
use crate::locale;

/// The plugin's id.
pub const PLUGIN: PluginId = PluginId::from_static("egui_pop.i18n");
/// The language setting: `system` or a locale.
pub const SETTING_LANGUAGE: SettingId = SettingId::from_static("appearance.language");
/// The setting's value that follows the system.
pub const SYSTEM: &str = "system";
/// Shows keys instead of text, through [`ShowKeys`] when the
/// application has it.
pub const SETTING_SHOW_KEYS: SettingId = SettingId::from_static("developer.show_keys");
/// [`ShowKeys`]' id.
pub const SHOW_KEYS: PluginId = PluginId::from_static("egui_pop.i18n.show_keys");
/// The step every hooked lookup runs through.
pub const STEP_RESOLVE: StepId = StepId::from_static("i18n.resolve");
/// The signal published when the locale changes.
pub const LOCALE_CHANGED: &str = "locale.changed";

static GLOBAL: RwLock<Option<I18n>> = RwLock::new(None);

/// The catalogues the last connected plugin uses (an empty English one
/// before): what [`crate::tr!`] reads.
pub fn global() -> I18n {
    GLOBAL
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
        .unwrap_or_default()
}

fn set_global(i18n: Option<I18n>) {
    *GLOBAL.write().unwrap_or_else(PoisonError::into_inner) = i18n;
}

fn ctx_id() -> egui::Id {
    egui::Id::new("egui_pop_i18n::I18n")
}

/// The catalogues the plugin installed in an egui context.
pub fn of(ctx: &egui::Context) -> Option<I18n> {
    ctx.data(|d| d.get_temp::<I18n>(ctx_id()))
}

/// The text of `key` in the context's language, with Fluent arguments;
/// else the key itself. Without the plugin, the interface's strings
/// provider answers (or the key shows).
///
/// ```
/// use egui_pop_i18n::{args, t};
///
/// let ctx = egui::Context::default();
/// assert_eq!(t(&ctx, "greeting", &args![name = "Ada"]), "greeting");
/// ```
pub fn t(ctx: &egui::Context, key: &str, args: &crate::Args<'_>) -> String {
    match of(ctx) {
        Some(i) => i.t(key, args),
        None => strings::lookup(ctx, key, ""),
    }
}

/// [`t`] with the English shown when no catalogue has the key.
pub fn text(ctx: &egui::Context, key: &str, english: &str, args: &crate::Args<'_>) -> String {
    match of(ctx) {
        Some(i) => i.text(key, english, args),
        None => strings::lookup(ctx, key, english),
    }
}

/// Switches the application's language through its setting
/// (`system` or a locale with a catalogue).
pub fn set_language(app: &AppHandle, value: &str) -> Result<(), String> {
    lock(&app.store())
        .set(Set::new(
            &setting_path(),
            Change::To(Value::Choice(value.into())),
            Origin::User,
        ))
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn setting_path() -> String {
    format!("settings/{SETTING_LANGUAGE}")
}

fn show_keys_path() -> String {
    format!("settings/{SETTING_SHOW_KEYS}")
}

fn show_keys_on(app: &AppHandle) -> bool {
    lock(&app.store())
        .get(&show_keys_path())
        .and_then(|e| e.value.as_bool())
        .unwrap_or(false)
}

#[derive(Debug, Default)]
struct Follow {
    /// The setting's value last applied.
    applied: Option<String>,
    /// Whether keys were shown, as last applied.
    keys: bool,
    /// The catalogues' generation the provider was installed at.
    installed: Option<u64>,
}

/// Translations for an application, as a plugin (see the module).
///
/// ```
/// use egui_pop_api::App;
/// use egui_pop_i18n::{I18nPlugin, plugin};
///
/// let ctx = egui::Context::default();
/// let app = App::builder().plugin(I18nPlugin::new()).build(&ctx).unwrap();
/// plugin::set_language(app.handle(), "fr").unwrap();
/// let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
///     egui_pop_core::strings::lookup(ui.ctx(), "warm-up", "");
/// });
/// out.textures_delta.clear();
/// ```
#[derive(Debug)]
pub struct I18nPlugin {
    i18n: I18n,
    catalogues: Vec<&'static [Catalogue]>,
    dir: Option<PathBuf>,
    env: Option<String>,
    language: String,
    follow: Arc<Mutex<Follow>>,
}

impl Default for I18nPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl I18nPlugin {
    /// With egui-pop's own catalogues (English and French).
    pub fn new() -> Self {
        Self {
            i18n: I18n::new(locale::ENGLISH),
            catalogues: vec![crate::i18n::EGUI_POP],
            dir: None,
            env: None,
            language: SYSTEM.into(),
            follow: Arc::default(),
        }
    }

    /// Adds the application's catalogues (loaded at connect).
    pub fn catalogues(mut self, catalogues: &'static [Catalogue]) -> Self {
        self.catalogues.push(catalogues);
        self
    }

    /// Reads `dir/<locale>/*.ftl` at connect, over the built-in
    /// catalogues (a missing directory is fine).
    pub fn dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.dir = Some(dir.into());
        self
    }

    /// An environment variable naming a locale, read before the system's
    /// language when the setting is `system` (`MYAPP_LANG=fr`).
    pub fn env(mut self, var: &str) -> Self {
        self.env = Some(var.into());
        self
    }

    /// The language setting's default: [`SYSTEM`] (the default) or a
    /// locale (tests and screenshots pin `en`).
    pub fn language(mut self, value: &str) -> Self {
        self.language = value.into();
        self
    }

    /// The catalogues, to keep (they are shared).
    pub fn i18n(&self) -> I18n {
        self.i18n.clone()
    }
}

impl Plugin for I18nPlugin {
    fn id(&self) -> PluginId {
        PLUGIN
    }

    fn title(&self) -> String {
        "Language".into()
    }

    fn priority(&self) -> i32 {
        // Before the plugins whose text it translates are drawn; first
        // among peers so its setting leads its section.
        100
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let i18n = self.i18n.clone();
        if i18n.locales().is_empty() {
            for c in &self.catalogues {
                i18n.add_catalogues(c).map_err(PluginError::Failed)?;
            }
            if let Some(dir) = &self.dir {
                i18n.add_dir(dir).map_err(PluginError::Failed)?;
            }
            // An application's text over egui-pop's: kept, and said.
            for (locale, key) in i18n.overridden() {
                ctx.app().publish(
                    "egui_pop.i18n",
                    None,
                    "i18n.overridden",
                    serde_json::json!({ "locale": locale, "key": key }),
                );
            }
        }
        let offered = i18n.offered();
        // Each language shows by its own name, whatever the locale.
        let names: String = offered
            .iter()
            .filter(|(tag, _)| {
                !i18n
                    .keys(locale::ENGLISH)
                    .contains(&strings::setting_choice_key(SETTING_LANGUAGE.as_str(), tag))
            })
            .map(|(tag, name)| {
                format!(
                    "{} = {name}\n",
                    strings::setting_choice_key(SETTING_LANGUAGE.as_str(), tag)
                )
            })
            .collect();
        if !names.is_empty() {
            i18n.add(locale::ENGLISH, &names)
                .map_err(PluginError::Failed)?;
        }
        let mut choices = vec![SYSTEM.to_string()];
        choices.extend(offered.into_iter().map(|(tag, _)| tag));
        let default = if choices.contains(&self.language) {
            self.language.as_str()
        } else {
            SYSTEM
        };
        let choices: Vec<&str> = choices.iter().map(String::as_str).collect();
        ctx.setting(
            SETTING_LANGUAGE,
            Descriptor::choice("Language", &choices, default)
                .description("The language of the interface; System follows the system's."),
        )?
        .setting(
            SETTING_SHOW_KEYS,
            Descriptor::bool("Show translation keys", false).description(
                "Shows each text's translation key instead of the text, to find where it comes from.",
            ),
        )?;
        ctx.step(
            StepSpec::new(
                STEP_RESOLVE,
                "A translated text is looked up (`key`, `locale`, `args`); the result is the text, null for none.",
            )
            .args(json!({ "key": "egui-pop-retry", "locale": "fr", "args": {} })),
        )?;
        let (snap, act) = (i18n.clone(), i18n.clone());
        let store = ctx.app().store();
        ctx.collector(
            FnCollector::new(
                "i18n",
                "Translations: the locale, its fallback chain, the locales, the keys missing per locale.",
            )
            .snapshot(move |_| {
                json!({
                    "locale": snap.locale(),
                    "chain": snap.chain(),
                    "locales": snap.locales(),
                    "missing": snap.missing(),
                })
            })
            .action(
                "set_locale",
                "Switches the language setting (`locale`: `system` or a locale).",
                true,
                move |_, args| {
                    let to = args["locale"].as_str().ok_or("`locale` is missing")?;
                    lock(&store)
                        .set(Set::new(
                            &setting_path(),
                            Change::To(Value::Choice(to.into())),
                            Origin::Agent("debug bridge".into()),
                        ))
                        .map_err(|e| e.to_string())?;
                    Ok(json!({ "setting": to }))
                },
            )
            .args(json!({ "locale": "fr" }))
            .action(
                "clear_missing",
                "Forgets the missing keys seen so far.",
                true,
                move |_, _| {
                    act.clear_missing();
                    Ok(Json::Null)
                },
            ),
        )?;
        let (follow, env, f_i18n) = (self.follow.clone(), self.env.clone(), i18n.clone());
        ctx.block(
            zones::OVERLAYS,
            Block::add(
                BlockId::from_static("egui_pop.i18n.follow"),
                95,
                move |_, app| {
                    self::follow(app, &f_i18n, &follow, env.as_deref());
                },
            ),
        );
        for on in [On::Started, On::Connected, On::Disconnected] {
            let i18n = i18n.clone();
            ctx.on(on, move |app, _| i18n.set_hooked(hooked(app)));
        }
        let app = ctx.app();
        app.egui()
            .data_mut(|d| d.insert_temp(ctx_id(), i18n.clone()));
        strings::install(&app.egui(), Arc::new(Provider::new(i18n.clone())));
        set_global(Some(i18n));
        Ok(())
    }

    fn disconnect(&mut self, ctx: &mut Ctx<'_>) {
        let egui = ctx.app().egui();
        egui.data_mut(|d| d.remove::<I18n>(ctx_id()));
        strings::install(&egui, Arc::new(strings::English));
        set_global(None);
        *self.follow.lock().unwrap_or_else(PoisonError::into_inner) = Follow::default();
    }
}

/// Whether a plugin hooks the `i18n.resolve` step.
fn hooked(app: &AppHandle) -> bool {
    app.steps()
        .iter()
        .any(|s| s.spec.id == STEP_RESOLVE && !s.hooks.is_empty())
}

/// Each frame: the setting applied, hooked lookups resolved, the
/// provider current.
fn follow(app: &AppHandle, i18n: &I18n, follow: &Mutex<Follow>, env: Option<&str>) {
    let mut f = follow.lock().unwrap_or_else(PoisonError::into_inner);
    let setting = lock(&app.store())
        .get(&setting_path())
        .and_then(|e| e.value.as_choice().map(str::to_string))
        .unwrap_or_else(|| SYSTEM.into());
    if f.applied.as_deref() != Some(setting.as_str()) {
        let env = env.and_then(|v| std::env::var(v).ok());
        let chosen = if setting == SYSTEM {
            locale::choose(env.as_deref(), None, locale::system().as_deref())
        } else {
            locale::choose(None, Some(&setting), None)
        };
        let from = i18n.locale();
        if chosen != from {
            i18n.set_locale(&chosen);
            app.publish(
                "egui_pop",
                None,
                LOCALE_CHANGED,
                json!({ "from": from, "to": i18n.locale(), "setting": setting }),
            );
        }
        f.applied = Some(setting);
    }
    let keys = show_keys_on(app);
    if keys != f.keys {
        f.keys = keys;
        if app.plugins().iter().any(|p| p.id == SHOW_KEYS) {
            app.set_plugin_enabled(SHOW_KEYS, keys);
        }
    }
    let pending = std::mem::take(&mut i18n.resolve().pending);
    let mut answers = Vec::new();
    for (id, asked) in pending {
        let lookup = |args: &Json| {
            let key = args["key"].as_str().unwrap_or_default();
            let locale = args["locale"].as_str().unwrap_or(locale::ENGLISH);
            let owned = json_args(&args["args"]);
            let fargs: Vec<(&str, fluent_bundle::FluentValue<'_>)> =
                owned.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
            i18n.get_in(locale, key, &fargs)
        };
        let out = app.step(&STEP_RESOLVE, asked.clone(), |_, args| {
            Ok(lookup(args).map_or(Json::Null, Json::String))
        });
        let answer = match out {
            Outcome::Done(Ok(Json::String(s))) => Some(s),
            Outcome::Done(Ok(Json::Null)) => None,
            Outcome::Done(Ok(other)) => Some(other.to_string()),
            Outcome::Done(Err(_)) | Outcome::Vetoed { .. } => lookup(&asked),
        };
        answers.push((id, answer));
    }
    i18n.answer(answers);
    let generation = i18n.generation();
    if f.installed != Some(generation) {
        let ctx = app.egui();
        ctx.data_mut(|d| d.insert_temp(ctx_id(), i18n.clone()));
        strings::install(&ctx, Arc::new(Provider::new(i18n.clone())));
        f.installed = Some(generation);
        ctx.request_repaint();
    }
}

/// A debug aid: every lookup shows its key instead of its text, through
/// a run-hook on `i18n.resolve`. Connected while the
/// `developer.show_keys` setting is on.
#[derive(Clone, Copy, Debug, Default)]
pub struct ShowKeys;

impl Plugin for ShowKeys {
    fn id(&self) -> PluginId {
        SHOW_KEYS
    }

    fn title(&self) -> String {
        "Translation keys".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![PLUGIN]
    }

    fn enabled(&self, ctx: &Ctx<'_>) -> bool {
        show_keys_on(&ctx.app())
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.replace(STEP_RESOLVE, |_, args| Ok(args["key"].clone()));
        Ok(())
    }
}
