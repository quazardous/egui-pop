//! The theme editor, as a plugin ([`ThemeEditorPlugin`]): a theme's
//! tokens edited live, grouped by role, each colour with its picker and
//! the contrast of the pairs it takes part in, started from any built-in
//! theme, exported and imported as a token file, every change undoable.
//!
//! [`show`] draws the editor in any `Ui` (a panel, a window, a settings
//! page); the state lives in the plugin's service ([`THEME_EDITOR`]),
//! over the theme service: each change is written to the application's
//! theme (its base and overrides) and installed before the next frame.
//! When the theme changes elsewhere (a theme switcher), the editor starts
//! again from it.
//!
//! The model is [`ThemeDraft`] (no
//! egui): a base theme, the tokens changed over it and their history.
//! A file is an ordinary `.tokens.json` that `$extends` the base.
//!
//! | Step | Arguments | Default |
//! |---|---|---|
//! | `theme.token` | `path`, `from`, `to` (raw DTCG values, `null` for the base theme's), `cause` (`edit`, `reset`, `undo`, `redo`, `agent`) | writes `to` |
//! | `theme.load` | `base`, `overrides`, `cause` (`start`, `import`) | loads them (the history starts again) |
//!
//! A before-hook vetoes a change (the token keeps `from`) or redirects
//! it (another `to`, checked again); a run-hook's result is the value
//! written. A colour dragged in its picker is shown live and becomes one
//! change when the pointer is released; a value typed in becomes one
//! when its field loses the focus.
//!
//! Commands, for agents (hidden from the palette): `theme.read` (the
//! draft, or one token with its contrast readouts), `theme.set` (`path`,
//! `value`, `null` for the base theme's), `theme.undo`, `theme.redo`,
//! `theme.start` (`base`), `theme.export` (`name`) and `theme.import`
//! (`text`).
//!
//! Owned: egui_thematic (MIT) edits egui's `Visuals`, not tokens with
//! aliases and contrast pairs; egui's colour picker (MIT OR Apache-2.0)
//! is adopted for colours; egui_colors (MIT) generates palettes and is
//! not used (see docs/theme-editor.md).
//!
//! ```
//! use egui_pop_api::theme_editor::{THEME_EDITOR, ThemeEditorPlugin, Cause};
//! use egui_pop_api::App;
//!
//! let app = App::builder()
//!     .plugin(ThemeEditorPlugin)
//!     .build(&egui::Context::default())
//!     .unwrap();
//! let handle = app.handle();
//! let editor = handle.service(&THEME_EDITOR).unwrap();
//! editor
//!     .set(handle, "accent.default", Some(serde_json::json!("#2060c0")), Cause::Agent)
//!     .unwrap();
//! assert_eq!(handle.theme().spec().overrides.len(), 1);
//! editor.undo(handle, Cause::Agent).unwrap();
//! assert!(handle.theme().spec().overrides.is_empty());
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use egui::{Id, RichText, Ui, WidgetInfo, WidgetType};
use egui_pop_core::lock;
use egui_pop_core::strings;
use egui_pop_core::theme::{Tokens, color32};
use egui_pop_core::tokens::contract::{self, Tier};
use egui_pop_core::tokens::contrast::{self, Level, Pair, PairKind};
use egui_pop_core::tokens::{
    BUILT_IN_THEMES, Color, ResolvedTokens, ThemeDraft, ThemeSpec, TokenEdit, Value, theme_label,
};
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::ids::{CommandId, PluginId, StepId};
use serde_json::{Value as Json, json};

use crate::app::AppHandle;
use crate::services::ServiceToken;
use crate::steps::{Outcome, StepSpec};
use crate::strings as txt;
use crate::{Ctx, Plugin, PluginError};

/// The plugin's id.
pub const THEME_EDITOR_PLUGIN: PluginId = PluginId::from_static("egui_pop.theme_editor");

/// The editor's state, as a service.
pub const THEME_EDITOR: ServiceToken<ThemeEditor> = ServiceToken::new("egui_pop.theme_editor");

/// A token changes.
pub const STEP_THEME_TOKEN: StepId = StepId::from_static("theme.token");
/// The editor starts from a theme, or from a file.
pub const STEP_THEME_LOAD: StepId = StepId::from_static("theme.load");

pub const CMD_THEME_READ: CommandId = CommandId::from_static("theme.read");
pub const CMD_THEME_SET: CommandId = CommandId::from_static("theme.set");
pub const CMD_THEME_UNDO: CommandId = CommandId::from_static("theme.undo");
pub const CMD_THEME_REDO: CommandId = CommandId::from_static("theme.redo");
pub const CMD_THEME_START: CommandId = CommandId::from_static("theme.start");
pub const CMD_THEME_EXPORT: CommandId = CommandId::from_static("theme.export");
pub const CMD_THEME_IMPORT: CommandId = CommandId::from_static("theme.import");

/// Why a token changes, as the `theme.token` step says it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cause {
    /// The user, in the editor.
    Edit,
    /// The user went back to the base theme's value.
    Reset,
    Undo,
    Redo,
    /// An agent or the application, through a command or the API.
    Agent,
}

impl Cause {
    pub fn name(self) -> &'static str {
        match self {
            Cause::Edit => "edit",
            Cause::Reset => "reset",
            Cause::Undo => "undo",
            Cause::Redo => "redo",
            Cause::Agent => "agent",
        }
    }
}

/// The level a theme is checked at: AAA text (7:1) for a high-contrast
/// theme, AA otherwise.
pub fn level_of(base: &str) -> Level {
    if base.contains("high-contrast") {
        Level::High
    } else {
        Level::Normal
    }
}

/// One contrast pair a token takes part in, measured.
#[derive(Clone, Debug, PartialEq)]
pub struct Readout {
    pub pair: Pair,
    /// `None` when one side is not a colour.
    pub ratio: Option<f32>,
    /// WCAG AA's ratio for the pair's kind (4.5 for text, 3 for a part).
    pub aa: f32,
    /// WCAG AAA's ratio (7 for text); `None` for interface parts, which
    /// have no AAA level.
    pub aaa: Option<f32>,
    /// The ratio the theme needs ([`level_of`] its base).
    pub required: f32,
}

impl Readout {
    pub fn passes_aa(&self) -> bool {
        self.ratio.is_some_and(|r| r >= self.aa)
    }

    pub fn passes_aaa(&self) -> Option<bool> {
        self.aaa.map(|a| self.ratio.is_some_and(|r| r >= a))
    }

    /// Whether it reaches what the theme needs.
    pub fn passes(&self) -> bool {
        self.ratio.is_some_and(|r| r >= self.required)
    }

    pub fn to_json(&self) -> Json {
        json!({
            "foreground": self.pair.foreground,
            "background": self.pair.background,
            "kind": match self.pair.kind { PairKind::Text => "text", PairKind::Ui => "ui" },
            "ratio": self.ratio.map(|r| (r * 100.0).round() / 100.0),
            "aa": self.passes_aa(),
            "aaa": self.passes_aaa(),
            "required": self.required,
            "passes": self.passes(),
        })
    }
}

/// The tokens whose value comes from `path`: itself, and every token
/// aliasing it, directly or not.
pub fn dependents(tokens: &ResolvedTokens, path: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for t in tokens.iter() {
        let mut at = Some(t);
        let mut hops = 0;
        while let Some(token) = at {
            if token.path == path {
                out.insert(t.path.clone());
                break;
            }
            hops += 1;
            at = token
                .alias
                .as_deref()
                .filter(|_| hops < 64)
                .and_then(|a| tokens.get(a));
        }
    }
    out
}

/// The contrast pairs a token takes part in, itself or through the
/// tokens aliasing it, measured in `tokens`.
pub fn contrast_of(tokens: &ResolvedTokens, path: &str) -> Vec<Readout> {
    let deps = dependents(tokens, path);
    let level = level_of(&tokens.spec().base);
    contrast::PAIRS
        .iter()
        .filter(|p| deps.contains(p.foreground) || deps.contains(p.background))
        .map(|p| Readout {
            pair: *p,
            ratio: contrast::pair_ratio(tokens, p),
            aa: Level::Normal.required(p.kind),
            aaa: (p.kind == PairKind::Text).then(|| Level::High.required(p.kind)),
            required: level.required(p.kind),
        })
        .collect()
}

/// A gesture in progress: a token shown live, recorded at its end.
#[derive(Debug)]
struct Gesture {
    path: String,
    from: Option<Json>,
    widget: Id,
    /// Whether the widget is also moved by keys (a number field): the
    /// gesture then ends when it lets the focus go, not at the release.
    keys: bool,
}

#[derive(Debug, Default)]
struct State {
    draft: ThemeDraft,
    /// The base and overrides last written to the theme service; `None`
    /// before the first look at it.
    applied: Option<(String, BTreeMap<String, Json>)>,
    selected: Option<String>,
    filter: String,
    gesture: Option<Gesture>,
    /// The value field being typed into: its token and text.
    typing: Option<(String, String)>,
    /// The export and import text, and the theme's name in it.
    file_text: String,
    file_name: String,
    /// What the last action said (an import's result, a refusal).
    message: Option<(bool, String)>,
    resolved: Option<Arc<ResolvedTokens>>,
}

/// The theme editor's state, shared by its view and its commands.
#[derive(Debug, Default)]
pub struct ThemeEditor {
    state: Mutex<State>,
}

impl ThemeEditor {
    /// Follows the application's theme: when its base or overrides are
    /// not what the editor wrote last, the editor starts again from them.
    pub fn sync(&self, app: &AppHandle) {
        let spec = app.theme().spec();
        let mut s = lock(&self.state);
        let now = (spec.base.clone(), spec.overrides.clone());
        if s.applied.as_ref() != Some(&now) {
            s.draft = ThemeDraft::from_spec(&spec);
            s.applied = Some(now);
            s.gesture = None;
            s.typing = None;
        }
    }

    /// Shows a token in the editor (as a click in its list does).
    pub fn select(&self, path: &str) {
        let mut s = lock(&self.state);
        s.selected = Some(path.to_string());
        s.typing = None;
    }

    /// The token shown in the editor.
    pub fn selected(&self) -> Option<String> {
        lock(&self.state).selected.clone()
    }

    /// The draft as it is.
    pub fn draft(&self) -> ThemeDraft {
        lock(&self.state).draft.clone()
    }

    /// The application's theme, resolved with the draft (cached).
    pub fn resolved(&self, app: &AppHandle) -> Arc<ResolvedTokens> {
        let service = app.theme();
        let spec = service.spec();
        let mut s = lock(&self.state);
        if let Some(r) = s.resolved.as_ref().filter(|r| *r.spec() == spec) {
            return r.clone();
        }
        let r = Arc::new(service.themes().resolve(&spec));
        s.resolved = Some(r.clone());
        r
    }

    /// Writes the draft to the application's theme.
    fn write(&self, app: &AppHandle) {
        let mut s = lock(&self.state);
        let base = s.draft.base().to_string();
        let overrides = s.draft.overrides().clone();
        s.applied = Some((base.clone(), overrides.clone()));
        drop(s);
        app.theme().update(|spec| {
            spec.base = base;
            spec.overrides = overrides;
        });
    }

    /// Runs the `theme.token` step for an edit; the edit to make, or why
    /// none.
    fn step(&self, app: &AppHandle, edit: TokenEdit, cause: Cause) -> Result<TokenEdit, String> {
        let mut args = edit.to_json();
        args["cause"] = json!(cause.name());
        match app.step(&STEP_THEME_TOKEN, args, |_, a| Ok(a["to"].clone())) {
            Outcome::Done(Ok(v)) => {
                let to = (!v.is_null()).then_some(v);
                if to == edit.to {
                    return Ok(edit);
                }
                let themes = app.theme();
                let draft = self.draft();
                draft
                    .edit(themes.themes(), &edit.path, to)?
                    .ok_or_else(|| "redirected to the value it has".to_string())
            }
            Outcome::Done(Err(e)) => Err(e),
            Outcome::Vetoed { by, reason } => Err(format!("vetoed by {by}: {reason}")),
        }
    }

    /// Sets a token (`None`: the base theme's value) through the
    /// `theme.token` step, recorded for undo and written to the theme.
    /// `Ok(None)`: nothing changed.
    pub fn set(
        &self,
        app: &AppHandle,
        path: &str,
        to: Option<Json>,
        cause: Cause,
    ) -> Result<Option<TokenEdit>, String> {
        self.sync(app);
        let edit = {
            let themes = app.theme();
            let draft = self.draft();
            draft.edit(themes.themes(), path, to)?
        };
        let Some(edit) = edit else {
            return Ok(None);
        };
        let edit = self.step(app, edit, cause)?;
        lock(&self.state).draft.apply(edit.clone());
        self.write(app);
        Ok(Some(edit))
    }

    /// Takes the last change back, through the `theme.token` step.
    /// `Ok(None)`: nothing to undo.
    pub fn undo(&self, app: &AppHandle, cause: Cause) -> Result<Option<TokenEdit>, String> {
        self.history(app, cause, false)
    }

    /// Makes the last undone change again.
    pub fn redo(&self, app: &AppHandle, cause: Cause) -> Result<Option<TokenEdit>, String> {
        self.history(app, cause, true)
    }

    fn history(
        &self,
        app: &AppHandle,
        cause: Cause,
        redo: bool,
    ) -> Result<Option<TokenEdit>, String> {
        self.sync(app);
        let next = {
            let s = lock(&self.state);
            if redo {
                s.draft.redo_edit()
            } else {
                s.draft.undo_edit()
            }
        };
        let Some(next) = next else {
            return Ok(None);
        };
        let made = self.step(app, next.clone(), cause)?;
        {
            let mut s = lock(&self.state);
            if made == next {
                if redo {
                    s.draft.redo();
                } else {
                    s.draft.undo();
                }
            } else {
                // Redirected: another change, recorded as a new one.
                s.draft.apply(made.clone());
            }
        }
        self.write(app);
        Ok(Some(made))
    }

    /// Runs the `theme.load` step and loads what it gives.
    fn load(&self, app: &AppHandle, draft: ThemeDraft, cause: &str) -> Result<(), String> {
        let args = json!({
            "base": draft.base(),
            "overrides": draft.overrides(),
            "cause": cause,
        });
        let v = match app.step(&STEP_THEME_LOAD, args, |_, a| Ok(a.clone())) {
            Outcome::Done(Ok(v)) => v,
            Outcome::Done(Err(e)) => return Err(e),
            Outcome::Vetoed { by, reason } => return Err(format!("vetoed by {by}: {reason}")),
        };
        let base = v["base"]
            .as_str()
            .ok_or("the step gave no `base`")?
            .to_string();
        let themes = app.theme();
        if themes.themes().get(&base).is_none() {
            return Err(format!("no theme `{base}`"));
        }
        let mut spec = ThemeSpec::new(base);
        if let Some(o) = v["overrides"].as_object() {
            spec.overrides = o.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        }
        {
            let mut s = lock(&self.state);
            s.draft = ThemeDraft::from_spec(&spec);
            s.gesture = None;
            s.typing = None;
        }
        self.write(app);
        Ok(())
    }

    /// Starts again from a theme the application knows (its tokens as
    /// they are, the history emptied), through the `theme.load` step.
    pub fn start(&self, app: &AppHandle, base: &str) -> Result<(), String> {
        self.load(app, ThemeDraft::new(base), "start")
    }

    /// The draft as the text of a `.tokens.json` file named `name`.
    pub fn export(&self, app: &AppHandle, name: &str) -> String {
        self.sync(app);
        self.draft().export(app.theme().themes(), name)
    }

    /// Loads a token file through the `theme.load` step; the tokens left
    /// out (unknown to its base, or with a value of another type).
    pub fn import(&self, app: &AppHandle, text: &str) -> Result<Vec<String>, String> {
        let (draft, skipped) = {
            let themes = app.theme();
            ThemeDraft::import(themes.themes(), "import", text).map_err(|e| e.to_string())?
        };
        self.load(app, draft, "import")?;
        Ok(skipped)
    }

    /// The draft, or one token, as agents read it.
    pub fn read(&self, app: &AppHandle, path: Option<&str>) -> Result<Json, String> {
        self.sync(app);
        let tokens = self.resolved(app);
        let draft = self.draft();
        let Some(path) = path else {
            let level = level_of(draft.base());
            return Ok(json!({
                "base": draft.base(),
                "overrides": draft.overrides(),
                "history": draft.history().iter().map(TokenEdit::to_json).collect::<Vec<_>>(),
                "can_undo": draft.can_undo(),
                "can_redo": draft.can_redo(),
                "level": match level { Level::High => "high", Level::Normal => "normal" },
                "failing": contrast::check(&tokens, level).iter().map(ToString::to_string).collect::<Vec<_>>(),
                "problems": tokens.problems().iter().map(ToString::to_string).collect::<Vec<_>>(),
            }));
        };
        let t = tokens
            .get(path)
            .ok_or_else(|| format!("no token `{path}`"))?;
        Ok(json!({
            "path": t.path,
            "type": t.ty,
            "tier": t.tier().name(),
            "value": t.value.to_json(),
            "alias": t.alias,
            "own": draft.value(path),
            "description": description(path, t.description.as_deref()),
            "contrast": contrast_of(&tokens, path).iter().map(Readout::to_json).collect::<Vec<_>>(),
        }))
    }
}

fn description(path: &str, own: Option<&str>) -> String {
    contract::required(path)
        .map(|r| r.description.to_string())
        .or_else(|| own.map(String::from))
        .unwrap_or_default()
}

/// Declares the editor's service, steps and agent commands.
#[derive(Debug, Default)]
pub struct ThemeEditorPlugin;

impl Plugin for ThemeEditorPlugin {
    fn id(&self) -> PluginId {
        THEME_EDITOR_PLUGIN
    }

    fn title(&self) -> String {
        "Theme editor".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        const CAT: &str = "Theme";
        ctx.service(&THEME_EDITOR, ThemeEditor::default())?;
        ctx.step(
            StepSpec::new(
                STEP_THEME_TOKEN,
                "A theme token changes in the theme editor (`path`, `from`, `to`: raw DTCG values, null for the base theme's, `cause`); a veto keeps `from`, a redirect or a run-hook gives another value.",
            )
            .args(json!({
                "path": "accent.default",
                "from": null,
                "to": "#2060c0",
                "cause": "edit",
            })),
        )?;
        ctx.step(
            StepSpec::new(
                STEP_THEME_LOAD,
                "The theme editor starts from a theme or a file (`base`, `overrides`, `cause`: start or import); a veto keeps the theme, a redirect gives another base or other overrides.",
            )
            .args(json!({ "base": "pop-light", "overrides": {}, "cause": "start" })),
        )?;
        let editor = |call: &crate::Call<'_>| {
            call.app
                .service(&THEME_EDITOR)
                .ok_or_else(|| "the theme editor is not connected".to_string())
        };
        ctx.command(
            CommandSpec::new(CMD_THEME_READ, "Read the theme", CAT)
                .description("The theme being edited: its base, the tokens changed, the history and the failing contrast pairs; with `path`, one token, its value, alias and contrast readouts.")
                .hidden()
                .args(json!({ "path": "accent.default" })),
            move |call| {
                let path = call.args.get("path").and_then(Json::as_str);
                editor(call)?.read(call.app, path)
            },
        )?;
        ctx.command(
            CommandSpec::new(CMD_THEME_SET, "Set a theme token", CAT)
                .description("Sets a token of the theme being edited (`path`, `value`: a raw DTCG value or an alias of another token, null for the base theme's), through the theme.token step; undoable.")
                .hidden()
                .args(json!({ "path": "accent.default", "value": "#2060c0" })),
            move |call| {
                let path = call
                    .args
                    .get("path")
                    .and_then(Json::as_str)
                    .ok_or("`path` is missing")?;
                let value = call.args.get("value").cloned().filter(|v| !v.is_null());
                let edit = editor(call)?.set(call.app, path, value, Cause::Agent)?;
                Ok(edit.map_or(json!({ "changed": false }), |e| {
                    json!({ "changed": true, "edit": e.to_json() })
                }))
            },
        )?;
        for (id, title, description, redo) in [
            (
                CMD_THEME_UNDO,
                "Undo a theme change",
                "Takes the theme editor's last change back, through the theme.token step.",
                false,
            ),
            (
                CMD_THEME_REDO,
                "Redo a theme change",
                "Makes the theme editor's last undone change again.",
                true,
            ),
        ] {
            ctx.command(
                CommandSpec::new(id, title, CAT)
                    .description(description)
                    .hidden(),
                move |call| {
                    let e = editor(call)?;
                    let made = if redo {
                        e.redo(call.app, Cause::Agent)?
                    } else {
                        e.undo(call.app, Cause::Agent)?
                    };
                    Ok(made.map_or(Json::Null, |m| m.to_json()))
                },
            )?;
        }
        ctx.command(
            CommandSpec::new(CMD_THEME_START, "Start a theme from", CAT)
                .description("Starts the theme editor again from a theme (`base`), its history emptied, through the theme.load step.")
                .hidden()
                .args(json!({ "base": "pop-light" })),
            move |call| {
                let base = call
                    .args
                    .get("base")
                    .and_then(Json::as_str)
                    .ok_or("`base` is missing")?;
                editor(call)?.start(call.app, base)?;
                Ok(json!({ "base": base }))
            },
        )?;
        ctx.command(
            CommandSpec::new(CMD_THEME_EXPORT, "Export the theme", CAT)
                .description("The theme being edited as a .tokens.json file (`name`): $extends its base, the tokens changed with their types.")
                .hidden()
                .args(json!({ "name": "my-theme" })),
            move |call| {
                let name = call
                    .args
                    .get("name")
                    .and_then(Json::as_str)
                    .unwrap_or("my-theme");
                Ok(json!({ "name": name, "text": editor(call)?.export(call.app, name) }))
            },
        )?;
        ctx.command(
            CommandSpec::new(CMD_THEME_IMPORT, "Import a theme", CAT)
                .description("Loads a .tokens.json file (`text`) into the theme editor, through the theme.load step; says which tokens were left out.")
                .hidden()
                .args(json!({ "text": "{ \"$extends\": \"pop-light\" }" })),
            move |call| {
                let text = call
                    .args
                    .get("text")
                    .and_then(Json::as_str)
                    .ok_or("`text` is missing")?;
                let skipped = editor(call)?.import(call.app, text)?;
                Ok(json!({ "skipped": skipped }))
            },
        )?;
        Ok(())
    }
}

/// A group of tokens, by role.
struct Group {
    title: &'static strings::StrKey,
    paths: Vec<String>,
    open: bool,
}

/// The tokens by role: the system tier's roles, the components', then
/// the palette.
fn groups(tokens: &ResolvedTokens) -> Vec<Group> {
    let system = |test: &dyn Fn(&str) -> bool| -> Vec<String> {
        contract::contract()
            .filter(|r| r.tier == Tier::System && test(r.path))
            .map(|r| r.path.to_string())
            .collect()
    };
    let starts = |prefixes: &'static [&'static str]| {
        move |p: &str| prefixes.iter().any(|x| p.starts_with(x))
    };
    let accent = |p: &str| p.starts_with("accent.") || p == "selection" || p == "focus.ring";
    let states = |p: &str| ["danger", "warning", "success", "info"].contains(&p);
    vec![
        Group {
            title: &txt::THEME_EDITOR_SURFACES,
            paths: system(&starts(&["surface."])),
            open: true,
        },
        Group {
            title: &txt::THEME_EDITOR_TEXT,
            paths: system(&starts(&["text."])),
            open: true,
        },
        Group {
            title: &txt::THEME_EDITOR_ACCENT,
            paths: system(&accent),
            open: true,
        },
        Group {
            title: &txt::THEME_EDITOR_STATES,
            paths: system(&states),
            open: true,
        },
        Group {
            title: &txt::THEME_EDITOR_BORDERS,
            paths: system(&starts(&["border."])),
            open: false,
        },
        Group {
            title: &txt::THEME_EDITOR_SHAPE,
            paths: system(&starts(&[
                "radius.", "space.", "size.", "font.", "opacity.", "motion.",
            ])),
            open: false,
        },
        Group {
            title: &txt::THEME_EDITOR_COMPONENTS,
            paths: contract::contract()
                .filter(|r| r.tier == Tier::Component)
                .map(|r| r.path.to_string())
                .collect(),
            open: false,
        },
        Group {
            title: &txt::THEME_EDITOR_PALETTE,
            paths: tokens
                .iter()
                .filter(|t| t.tier() == Tier::Reference)
                .map(|t| t.path.clone())
                .collect(),
            open: false,
        },
    ]
}

/// A raw value typed in: an alias `{path}`, else a value of the type.
fn parse_typed(ty: &str, text: &str) -> Option<Json> {
    let text = text.trim();
    if text.starts_with('{') && text.ends_with('}') {
        return Some(json!(text));
    }
    let number = |t: &str| t.parse::<f64>().ok();
    let v = match ty {
        "color" => json!(Color::from_hex(text)?.to_hex()),
        "dimension" => {
            json!({ "value": number(text.trim_end_matches("px").trim())?, "unit": "px" })
        }
        "duration" => json!({ "value": number(text.trim_end_matches("ms").trim())?, "unit": "ms" }),
        "number" => json!(number(text)?),
        "fontFamily" => json!(
            text.split(',')
                .map(|f| f.trim().to_string())
                .filter(|f| !f.is_empty())
                .collect::<Vec<_>>()
        ),
        _ => serde_json::from_str(text).ok()?,
    };
    Some(v)
}

/// A token's value as typed in its field: an alias as written, else
/// its value.
fn typed_text(draft: &ThemeDraft, t: &egui_pop_core::tokens::Token) -> String {
    match draft.value(&t.path).and_then(Json::as_str) {
        Some(own) if own.starts_with('{') => own.to_string(),
        _ => match &t.alias {
            Some(a) => format!("{{{a}}}"),
            None => t.value.display(),
        },
    }
}

/// Draws the theme editor (it needs [`ThemeEditorPlugin`]).
pub fn show(ui: &mut Ui, app: &AppHandle) {
    let Some(editor) = app.service(&THEME_EDITOR) else {
        ui.weak(strings::tr(ui.ctx(), &txt::THEME_EDITOR_MISSING));
        return;
    };
    editor.sync(app);
    let ctx = ui.ctx().clone();
    let tokens = editor.resolved(app);
    toolbar(ui, app, &editor);
    ui.separator();
    ui.horizontal_top(|ui| {
        let list_width = (ui.available_width() * 0.38).clamp(200.0, 320.0);
        ui.vertical(|ui| {
            ui.set_width(list_width);
            list(ui, &editor, &tokens);
        });
        ui.separator();
        ui.vertical(|ui| {
            let selected = lock(&editor.state).selected.clone();
            match selected.as_deref().and_then(|p| tokens.get(p)) {
                Some(t) => detail(ui, app, &editor, &tokens, t),
                None => {
                    ui.weak(strings::tr(&ctx, &txt::THEME_EDITOR_PICK));
                }
            }
        });
    });
    ui.separator();
    file(ui, app, &editor);
    let problems = tokens.problems();
    if !problems.is_empty() {
        let t = Tokens::of(&ctx);
        ui.colored_label(
            t.status.warning,
            strings::tr(&ctx, &txt::THEME_EDITOR_PROBLEMS),
        );
        for p in problems {
            ui.colored_label(t.status.warning, p.to_string());
        }
    }
    finish_gesture(ui, app, &editor);
}

fn toolbar(ui: &mut Ui, app: &AppHandle, editor: &ThemeEditor) {
    let ctx = ui.ctx().clone();
    ui.horizontal_wrapped(|ui| {
        let base = editor.draft().base().to_string();
        let start = strings::tr(&ctx, &txt::THEME_EDITOR_START);
        let label = ui.label(&start);
        let mut picked = base.clone();
        egui::ComboBox::from_id_salt("egui_pop.theme_editor.start")
            .selected_text(theme_label(&base))
            .show_ui(ui, |ui| {
                for name in BUILT_IN_THEMES {
                    ui.selectable_value(&mut picked, (*name).to_string(), theme_label(name));
                }
            })
            .response
            .labelled_by(label.id);
        if picked != base {
            say(editor, editor.start(app, &picked).map(|()| None));
        }
        let draft = editor.draft();
        for (redo, icon_key, can, next) in [
            (
                false,
                &txt::THEME_EDITOR_UNDO,
                draft.can_undo(),
                draft.undo_edit(),
            ),
            (
                true,
                &txt::THEME_EDITOR_REDO,
                draft.can_redo(),
                draft.redo_edit(),
            ),
        ] {
            let text = strings::tr(&ctx, icon_key);
            let r = ui.add_enabled(can, egui::Button::new(&text));
            let r = match next {
                Some(e) => r.on_hover_text(strings::tr_args(
                    &ctx,
                    &txt::THEME_EDITOR_CHANGE_OF,
                    &[("token", &e.path)],
                )),
                None => r,
            };
            if r.clicked() {
                let out = if redo {
                    editor.redo(app, Cause::Redo)
                } else {
                    editor.undo(app, Cause::Undo)
                };
                say(editor, out.map(|_| None));
            }
        }
        let changed = draft.overrides().len();
        ui.weak(strings::tr_args(
            &ctx,
            &txt::THEME_EDITOR_CHANGED_COUNT,
            &[("count", &changed.to_string())],
        ));
    });
    ui.horizontal(|ui| {
        let hint = strings::tr(&ctx, &txt::THEME_EDITOR_FILTER);
        let mut s = lock(&editor.state);
        let r = ui.add(
            egui::TextEdit::singleline(&mut s.filter)
                .hint_text(&hint)
                .desired_width(240.0),
        );
        drop(s);
        r.widget_info(|| WidgetInfo::labeled(WidgetType::TextEdit, true, &hint));
    });
}

/// Records what an action said, shown under the file area.
fn say(editor: &ThemeEditor, out: Result<Option<String>, String>) {
    let mut s = lock(&editor.state);
    s.message = match out {
        Ok(None) => None,
        Ok(Some(m)) => Some((true, m)),
        Err(e) => Some((false, e)),
    };
}

fn list(ui: &mut Ui, editor: &ThemeEditor, tokens: &ResolvedTokens) {
    let ctx = ui.ctx().clone();
    let (filter, selected, draft) = {
        let s = lock(&editor.state);
        (s.filter.to_lowercase(), s.selected.clone(), s.draft.clone())
    };
    let edited = strings::tr(&ctx, &txt::THEME_EDITOR_EDITED);
    egui::ScrollArea::vertical()
        .id_salt("egui_pop.theme_editor.list")
        .max_height(ui.available_height().max(240.0))
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for group in groups(tokens) {
                let paths: Vec<&String> = group
                    .paths
                    .iter()
                    .filter(|p| filter.is_empty() || p.contains(&filter))
                    .collect();
                if paths.is_empty() {
                    continue;
                }
                let title = strings::tr(&ctx, group.title);
                egui::CollapsingHeader::new(&title)
                    .id_salt(("egui_pop.theme_editor.group", group.title.key))
                    .default_open(group.open || !filter.is_empty())
                    .open((!filter.is_empty()).then_some(true))
                    .show(ui, |ui| {
                        for path in paths {
                            ui.horizontal(|ui| {
                                let fill = tokens.color(path);
                                swatch(ui, fill.map(color32));
                                let own = draft.value(path).is_some();
                                let text = if own {
                                    RichText::new(path.as_str()).strong()
                                } else {
                                    RichText::new(path.as_str())
                                };
                                let is = selected.as_deref() == Some(path.as_str());
                                let r = ui.selectable_label(is, text);
                                if own {
                                    ui.weak(&edited);
                                }
                                if r.clicked() {
                                    let mut s = lock(&editor.state);
                                    s.selected = Some(path.clone());
                                    s.typing = None;
                                }
                            });
                        }
                    });
            }
        });
}

/// A small swatch (nothing for a token that is not a colour).
fn swatch(ui: &mut Ui, fill: Option<egui::Color32>) {
    let t = Tokens::of(ui.ctx());
    let side = ui.spacing().interact_size.y * 0.7;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
    if let Some(fill) = fill {
        ui.painter().rect(
            rect,
            egui::CornerRadius::same(2),
            fill,
            egui::Stroke::new(1.0, t.border.strong),
            egui::StrokeKind::Inside,
        );
    }
}

fn detail(
    ui: &mut Ui,
    app: &AppHandle,
    editor: &ThemeEditor,
    tokens: &ResolvedTokens,
    t: &egui_pop_core::tokens::Token,
) {
    let ctx = ui.ctx().clone();
    let draft = editor.draft();
    let path = t.path.as_str();
    ui.heading(path);
    let d = description(path, t.description.as_deref());
    if !d.is_empty() {
        ui.weak(d);
    }
    ui.label(strings::tr_args(
        &ctx,
        &txt::THEME_EDITOR_TYPE,
        &[("type", &t.ty)],
    ));
    let own = draft.value(path).is_some();
    let base = theme_label(draft.base());
    ui.label(if own {
        strings::tr(&ctx, &txt::THEME_EDITOR_OWN)
    } else {
        strings::tr_args(&ctx, &txt::THEME_EDITOR_FROM, &[("theme", &base)])
    });
    if let Some(a) = &t.alias {
        ui.label(strings::tr_args(
            &ctx,
            &txt::THEME_EDITOR_ALIAS_OF,
            &[("token", a)],
        ));
    }
    ui.horizontal(|ui| {
        value_widget(ui, app, editor, t);
    });
    ui.horizontal(|ui| {
        let label = strings::tr(&ctx, &txt::THEME_EDITOR_VALUE);
        ui.label(&label);
        let mut text = {
            let s = lock(&editor.state);
            match &s.typing {
                Some((p, text)) if p == path => text.clone(),
                _ => typed_text(&draft, t),
            }
        };
        let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(220.0));
        let name = strings::tr_args(&ctx, &txt::THEME_EDITOR_VALUE_OF, &[("token", path)]);
        egui_pop_core::a11y::name(&r, WidgetType::TextEdit, &name);
        if r.changed() || r.has_focus() {
            lock(&editor.state).typing = Some((path.to_string(), text.clone()));
        }
        let enter = r.lost_focus();
        if enter {
            lock(&editor.state).typing = None;
            if text.trim() != typed_text(&draft, t) {
                let out = match parse_typed(&t.ty, &text) {
                    Some(v) => editor.set(app, path, Some(v), Cause::Edit),
                    None => Err(strings::tr_args(
                        &ctx,
                        &txt::THEME_EDITOR_NOT_A_VALUE,
                        &[("type", &t.ty)],
                    )),
                };
                say(editor, out.map(|_| None));
            }
        }
        let reset = strings::tr_args(&ctx, &txt::THEME_EDITOR_RESET, &[("theme", &base)]);
        if ui.add_enabled(own, egui::Button::new(&reset)).clicked() {
            say(
                editor,
                editor.set(app, path, None, Cause::Reset).map(|_| None),
            );
        }
    });
    contrast_readouts(ui, tokens, path);
}

/// The widget a token's type is edited with: a colour picker, a number.
fn value_widget(
    ui: &mut Ui,
    app: &AppHandle,
    editor: &ThemeEditor,
    t: &egui_pop_core::tokens::Token,
) {
    let ctx = ui.ctx().clone();
    let path = t.path.as_str();
    let name = strings::tr_args(&ctx, &txt::THEME_EDITOR_ADJUST_OF, &[("token", path)]);
    let (response, to, keys) = match &t.value {
        Value::Color(c) => {
            let mut c32 = color32(*c);
            let r = egui::color_picker::color_edit_button_srgba(
                ui,
                &mut c32,
                egui::color_picker::Alpha::OnlyBlend,
            );
            let [r8, g8, b8, a8] = c32.to_srgba_unmultiplied();
            let picked = Color {
                r: f32::from(r8) / 255.0,
                g: f32::from(g8) / 255.0,
                b: f32::from(b8) / 255.0,
                a: f32::from(a8) / 255.0,
            };
            r.widget_info(|| WidgetInfo::labeled(WidgetType::ColorButton, true, &name));
            ui.monospace(c.to_hex());
            let to = json!(picked.to_hex());
            (r, to, false)
        }
        Value::Dimension(v) | Value::Number(v) | Value::Duration(v) => {
            let mut x = *v;
            let unit = match &t.value {
                Value::Dimension(_) => " px",
                Value::Duration(_) => " ms",
                _ => "",
            };
            let speed = if matches!(t.value, Value::Number(_)) {
                0.01
            } else {
                0.25
            };
            let r = ui.add(egui::DragValue::new(&mut x).speed(speed).suffix(unit));
            r.widget_info(|| WidgetInfo::labeled(WidgetType::DragValue, true, &name));
            let to = match &t.value {
                Value::Dimension(_) => json!({ "value": x, "unit": "px" }),
                Value::Duration(_) => json!({ "value": x, "unit": "ms" }),
                _ => json!(x),
            };
            (r, to, true)
        }
        other => {
            ui.monospace(other.display());
            return;
        }
    };
    if response.changed() {
        let mut s = lock(&editor.state);
        if s.gesture.as_ref().is_none_or(|g| g.path != path) {
            let from = s.draft.value(path).cloned();
            s.gesture = Some(Gesture {
                path: path.to_string(),
                from,
                widget: response.id,
                keys,
            });
        }
        s.draft.preview(path, Some(to));
        drop(s);
        editor.write(app);
    }
}

/// Ends a gesture once the pointer is up and its widget has let the
/// focus go: one change through the step, from where it began.
fn finish_gesture(ui: &Ui, app: &AppHandle, editor: &ThemeEditor) {
    let busy = ui.input(|i| i.pointer.any_down());
    let mut s = lock(&editor.state);
    let Some(g) = s.gesture.as_ref() else {
        return;
    };
    if busy || (g.keys && ui.ctx().memory(|m| m.has_focus(g.widget))) {
        return;
    }
    let g = s.gesture.take().expect("checked above");
    let to = s.draft.value(&g.path).cloned();
    // Back to where it began: the step decides what is written.
    s.draft.preview(&g.path, g.from.clone());
    drop(s);
    let out = editor.set(app, &g.path, to, Cause::Edit);
    if out.is_err() {
        editor.write(app);
    }
    say(editor, out.map(|_| None));
}

fn contrast_readouts(ui: &mut Ui, tokens: &ResolvedTokens, path: &str) {
    let ctx = ui.ctx().clone();
    let t = Tokens::of(&ctx);
    let readouts = contrast_of(tokens, path);
    ui.add_space(t.space.m);
    ui.strong(strings::tr(&ctx, &txt::THEME_EDITOR_CONTRAST));
    if readouts.is_empty() {
        ui.weak(strings::tr(&ctx, &txt::THEME_EDITOR_NO_PAIRS));
        return;
    }
    let pass = strings::tr(&ctx, &txt::THEME_EDITOR_PASS);
    let fail = strings::tr(&ctx, &txt::THEME_EDITOR_FAIL);
    let none = strings::tr(&ctx, &txt::THEME_EDITOR_NOT_APPLICABLE);
    let verdict = |ui: &mut Ui, ok: Option<bool>| match ok {
        Some(true) => {
            ui.colored_label(t.status.success, &pass);
        }
        Some(false) => {
            ui.colored_label(t.status.danger, &fail);
        }
        None => {
            ui.weak(&none);
        }
    };
    egui::Grid::new(("egui_pop.theme_editor.contrast", path))
        .num_columns(5)
        .striped(true)
        .show(ui, |ui| {
            ui.strong(strings::tr(&ctx, &txt::THEME_EDITOR_PAIR));
            ui.strong(strings::tr(&ctx, &txt::THEME_EDITOR_RATIO));
            ui.strong("AA");
            ui.strong("AAA");
            ui.strong(strings::tr(&ctx, &txt::THEME_EDITOR_THEME_NEEDS));
            ui.end_row();
            for r in &readouts {
                ui.label(strings::tr_args(
                    &ctx,
                    &txt::THEME_EDITOR_PAIR_ON,
                    &[
                        ("foreground", r.pair.foreground),
                        ("background", r.pair.background),
                    ],
                ));
                ui.monospace(r.ratio.map_or("-".into(), |x| format!("{x:.2}:1")));
                verdict(ui, Some(r.passes_aa()));
                verdict(ui, r.passes_aaa());
                let needs = format!("{}:1", r.required);
                if r.passes() {
                    ui.colored_label(t.status.success, needs);
                } else {
                    ui.colored_label(t.status.danger, needs);
                }
                ui.end_row();
            }
        });
}

fn file(ui: &mut Ui, app: &AppHandle, editor: &ThemeEditor) {
    let ctx = ui.ctx().clone();
    let title = strings::tr(&ctx, &txt::THEME_EDITOR_FILE);
    egui::CollapsingHeader::new(&title)
        .id_salt("egui_pop.theme_editor.file")
        .default_open(false)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let label = strings::tr(&ctx, &txt::THEME_EDITOR_NAME);
                let l = ui.label(&label);
                let mut name = {
                    let s = lock(&editor.state);
                    if s.file_name.is_empty() {
                        "my-theme".to_string()
                    } else {
                        s.file_name.clone()
                    }
                };
                if ui
                    .add(egui::TextEdit::singleline(&mut name).desired_width(160.0))
                    .labelled_by(l.id)
                    .changed()
                {
                    lock(&editor.state).file_name = name.clone();
                }
                if ui
                    .button(strings::tr(&ctx, &txt::THEME_EDITOR_EXPORT))
                    .clicked()
                {
                    let text = editor.export(app, &name);
                    lock(&editor.state).file_text = text;
                }
                if ui
                    .button(strings::tr(&ctx, &txt::THEME_EDITOR_IMPORT))
                    .clicked()
                {
                    let text = lock(&editor.state).file_text.clone();
                    let out = editor.import(app, &text).map(|skipped| {
                        let base = theme_label(editor.draft().base());
                        let mut m = strings::tr_args(
                            &ctx,
                            &txt::THEME_EDITOR_IMPORTED,
                            &[("theme", &base)],
                        );
                        if !skipped.is_empty() {
                            m.push(' ');
                            m.push_str(&strings::tr_args(
                                &ctx,
                                &txt::THEME_EDITOR_SKIPPED,
                                &[("tokens", &skipped.join(", "))],
                            ));
                        }
                        Some(m)
                    });
                    say(editor, out);
                }
            });
            let label = strings::tr(&ctx, &txt::THEME_EDITOR_FILE_TEXT);
            let mut text = lock(&editor.state).file_text.clone();
            let r = ui.add(
                egui::TextEdit::multiline(&mut text)
                    .code_editor()
                    .desired_rows(6)
                    .desired_width(f32::INFINITY),
            );
            r.widget_info(|| WidgetInfo::labeled(WidgetType::TextEdit, true, &label));
            if r.changed() {
                lock(&editor.state).file_text = text;
            }
        });
    let message = lock(&editor.state).message.clone();
    if let Some((ok, m)) = message {
        let t = Tokens::of(&ctx);
        ui.colored_label(
            if ok {
                t.status.success
            } else {
                t.status.danger
            },
            m,
        );
    }
}
