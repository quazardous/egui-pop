//! The administration app (`docs/design/admin-forms.md`), titled
//! "Settings": every setting, and later every tool and task that is not
//! the work surface, as full pages generated from declarations. Nothing
//! here draws a component's own widgets: a component declares a
//! [`Manifest`] and the shell renders it with egui-pop's bricks.
//!
//! - **Full page**: an overlay of the whole main window, or "Pop out" to
//!   a window of its own; the mode used last is remembered (the
//!   `admin.window` setting). The window is an application window
//!   ([`crate::windows`], id [`APP_WINDOW`]): it repaints on its own, gets
//!   the application's title bar, keeps its size and place, and closing
//!   the main window while it is open moves it into the main window.
//!   Where the backend has no windows of its own, the overlay.
//! - **Two panes**: on the left, a tree of the scopes with their
//!   sections nested under them; on the right, the section's page, which
//!   wraps to the width it gets. The divider between them is dragged (or
//!   moved by its arrow keys) and its width kept ([`NAV_WIDTH`]). The
//!   search above the tree searches every scope.
//! - **Rows**: each setting's editor, a modified bar, an origin tag
//!   (`library`, `workspace`, `command line`), its reset (of the layer
//!   shown), and a "Set for" menu when it may be set in several value
//!   scopes. Applied live, undone with the settings' history.
//! - **Generated**: the declared settings become pages with no
//!   hand-written page ([`egui_pop_model::admin::generate`]); manifests
//!   add scopes and sections ([`Admin::manifest`]).
//! - **Value scopes**: [`Admin::offer`] opens the library or workspace
//!   layer, [`Admin::load_layer`] fills it, [`Admin::set_run`] records a
//!   command line override ([`egui_pop_model::admin::layers`]).
//! - **Records and statuses**: lists and tables read their rows from a
//!   `read_only` command or a `records` hook, statuses their value from a
//!   `read_only` command or a `status` hook; read once, and again after
//!   a command that changes something or an event they name.
//! - **Clauses**: a node's `when` hides it and its `enabled` greys it
//!   (with its reason), read against the application's context keys and
//!   the section's values as `form.<field id>`.
//! - **Commands**: `admin.open` (`{"path": "scope/section/key"}`, or
//!   `{"key": "<setting id>"}`, and `{"mode": "overlay" | "window"}`),
//!   `admin.close`, `admin.popOut`, `admin.set` and `admin.reset` (a value
//!   in a scope, for agents); `{"wizard": "<id>"}` opens a wizard. The `admin` collector shows the tree.
//! - **Settings files**: `settings.export` and `settings.import` (with
//!   `dryRun`), and their wizards in General › Advanced ([`transfer`]).
//!
//! - **Wizards**: multi-step flows declared in a manifest
//!   ([`egui_pop_model::admin::wizard`]) fill the page while they run
//!   ([`wizard`]): `admin.wizard.open` and its sibling commands drive one.
//!
//! `Ctrl+,` and `app.settings` open it ([`crate::settings_window`]).
//! The shortcuts editor, the gestures, the Fonts page and the custom
//! settings pages are its pages, as registered widget kinds (`shortcuts`,
//! `gestures`, `fonts`, `settings-page`); the settings window of before
//! opens from General › Advanced.

mod shell;
pub mod transfer;
pub mod wizard;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use egui::Ui;
use egui_pop_core::lock;
use egui_pop_model::admin::generate::{self, Entry};
use egui_pop_model::admin::hooks::{Hook, Hooks};
use egui_pop_model::admin::layers::Layers;
use egui_pop_model::admin::{Manifest, Tree, ValueScope};
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::diagnostics::Diagnostics;
use egui_pop_model::ids::{BlockId, CommandId, OwnerId, PluginId, SettingId};
use egui_pop_model::values::{Change, Descriptor, Set, Value};
use serde_json::{Value as Json, json};

use crate::app::AppHandle;
use crate::history::StoreScope;
use crate::services::ServiceToken;
use crate::zones::{self, Block};
use crate::{Ctx, Plugin, PluginError};

pub use egui_pop_model::admin as model;
pub use wizard::{
    CMD_WIZARD_BACK, CMD_WIZARD_CANCEL, CMD_WIZARD_FINISH, CMD_WIZARD_GO, CMD_WIZARD_NEXT,
    CMD_WIZARD_OPEN, CMD_WIZARD_SET, CMD_WIZARD_SKIP,
};

/// Opens the administration app (`{"path"}`, `{"key"}`, `{"mode"}`).
pub const CMD_OPEN: CommandId = CommandId::from_static("admin.open");
/// Closes it.
pub const CMD_CLOSE: CommandId = CommandId::from_static("admin.close");
/// Moves it between the overlay and a window of its own.
pub const CMD_POP_OUT: CommandId = CommandId::from_static("admin.popOut");
/// Sets a setting's value in a value scope (`{"key", "scope", "value"}`).
pub const CMD_SET: CommandId = CommandId::from_static("admin.set");
/// Takes a value scope's value of a setting back (`{"key", "scope"}`).
pub const CMD_RESET: CommandId = CommandId::from_static("admin.reset");

/// Whether it opens in a window of its own: the mode used last.
pub const WINDOW: SettingId = SettingId::from_static("admin.window");

/// The id of its application window ([`crate::windows`]), in the window
/// mode: `appWindow == 'settings'` in `when` clauses, `window: "settings"`
/// for agents, its size and place kept under it.
pub const APP_WINDOW: &str = "settings";

/// The service.
pub const ADMIN: ServiceToken<Admin> = ServiceToken::new("admin");

/// The focus surface the pages are drawn in: its local scope is the
/// settings' history.
pub const SURFACE: &str = "adminWindow";

/// A registered widget kind: draws a node of its kind with its options
/// (the declared exception to "components never draw").
pub type WidgetKindFn = Arc<dyn Fn(&mut Ui, &AppHandle, &Json) + Send + Sync>;

/// A widget kind's search: the items of a node of its kind matching the
/// query (every word), each as its key and its title. The key is what
/// `scope/section/<key>` lands on: the widget marks the item while its
/// section shows ([`Admin::marked`]) and scrolls to it once
/// ([`Admin::take_key`]).
pub type WidgetSearchFn =
    Arc<dyn Fn(&egui::Context, &AppHandle, &Json, &str) -> Vec<(String, String)> + Send + Sync>;

#[derive(Debug, Default)]
struct State {
    open: bool,
    scope: Option<String>,
    section: Option<String>,
    /// An item to scroll to once (a deep link's key).
    key: Option<String>,
    /// A record a deep link named (its key's value): marked while its
    /// section shows.
    row: Option<String>,
    /// The item a deep link or a search hit landed on (a setting id, or
    /// a widget's item key): marked while its section shows.
    mark: Option<String>,
    filter: String,
    /// The value scope a row writes to, when its "Set for" menu chose.
    target: BTreeMap<String, ValueScope>,
    /// Its application window opened, not seen closed since.
    window_shown: bool,
}

/// The administration app's state, manifests, hooks, widget kinds and
/// value layers.
#[derive(Default)]
pub struct Admin {
    state: Mutex<State>,
    layers: Mutex<Layers>,
    manifests: Mutex<Vec<Manifest>>,
    hooks: Mutex<Hooks>,
    widgets: Mutex<BTreeMap<String, WidgetKindFn>>,
    searches: Mutex<BTreeMap<String, WidgetSearchFn>>,
    diagnostics: Mutex<Diagnostics>,
    reads: Mutex<Reads>,
    /// The wizard running, if any.
    wizard: Mutex<Option<wizard::Running>>,
    /// What the last wizard finished ran.
    finished: Mutex<Option<Json>>,
    /// A path field's Browse waiting for its choice: the draft key.
    browsing: Mutex<Option<(String, crate::host::Pending)>>,
    /// Settings export and import: retired settings, the last import.
    transfer: transfer::State,
}

/// The answers of the `read_only` commands statuses and records read:
/// asked once, kept until an event says they changed (a command that
/// changes something ran, or an event a node names in `refresh`), never
/// read per frame.
#[derive(Debug, Default)]
struct Reads {
    answers: BTreeMap<String, Result<Json, String>>,
    /// A read asked: its command, its arguments, and how many runs of
    /// that command the history held then (its answer is a later one).
    asked: BTreeMap<String, (CommandId, Json, usize)>,
    /// The last journal event looked at.
    seq: Option<crate::host::Seq>,
}

fn read_key(command: &str, args: &Json) -> String {
    format!("{command} {args}")
}

impl std::fmt::Debug for Admin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Admin")
            .field("state", &self.state)
            .field("layers", &self.layers)
            .finish_non_exhaustive()
    }
}

impl Admin {
    fn new() -> Admin {
        Admin {
            layers: Mutex::new(Layers::new()),
            ..Admin::default()
        }
    }

    pub fn is_open(&self) -> bool {
        lock(&self.state).open
    }

    /// Opens it on a place: `scope`, `scope/section`, `scope/section/key`
    /// or a setting id (`None`: where it was).
    pub fn open(&self, app: &AppHandle, place: Option<&str>) -> Result<(), String> {
        if let Some(link) = place.filter(|p| !p.is_empty()) {
            let tree = self.tree(app);
            let p = tree
                .resolve(link)
                .ok_or_else(|| format!("no place `{link}` in the settings"))?;
            let mut s = lock(&self.state);
            s.scope = Some(p.scope);
            s.section = p.section;
            s.row = None;
            s.mark.clone_from(&p.key);
            s.key = p.key;
            s.filter.clear();
        }
        lock(&self.state).open = true;
        self.refresh();
        Ok(())
    }

    /// Closes it, and the wizard running with it.
    pub fn close(&self) {
        lock(&self.wizard).take();
        let mut s = lock(&self.state);
        s.open = false;
        s.filter.clear();
        s.mark = None;
    }

    /// Whether its application window was opened and not seen closed
    /// since.
    fn window_shown(&self) -> bool {
        lock(&self.state).window_shown
    }

    fn set_window_shown(&self, shown: bool) {
        lock(&self.state).window_shown = shown;
    }

    /// The scope, section and search shown.
    pub fn shown(&self) -> (Option<String>, Option<String>, String) {
        let s = lock(&self.state);
        (s.scope.clone(), s.section.clone(), s.filter.clone())
    }

    /// Adds a manifest (checked when the tree is built: what does not
    /// resolve is left out and reported).
    pub fn manifest(&self, manifest: Manifest) {
        lock(&self.manifests).push(manifest);
    }

    /// Registers a hook a manifest names; `false` when the name is taken.
    pub fn hook(&self, name: &str, hook: Hook) -> bool {
        lock(&self.hooks).register(name, hook)
    }

    /// Registers a widget kind: draws a node of its kind with its options
    /// (`shortcuts`, `gestures`, `fonts` and `settings-page` are
    /// egui-pop's own). One that edits settings names them in its
    /// options' `settings`, so the generated pages leave them out.
    pub fn widget_kind(
        &self,
        id: &str,
        draw: impl Fn(&mut Ui, &AppHandle, &Json) + Send + Sync + 'static,
    ) {
        lock(&self.widgets).insert(id.into(), Arc::new(draw));
    }

    pub(crate) fn widget(&self, id: &str) -> Option<WidgetKindFn> {
        lock(&self.widgets).get(id).cloned()
    }

    /// Registers a widget kind's search, so the Settings app's search
    /// finds the items inside a node of that kind (the shortcuts editor's
    /// commands, by name and by key). The settings a widget names in its
    /// options' `settings` are searched without it.
    pub fn widget_search(
        &self,
        kind: &str,
        search: impl Fn(&egui::Context, &AppHandle, &Json, &str) -> Vec<(String, String)>
        + Send
        + Sync
        + 'static,
    ) {
        lock(&self.searches).insert(kind.into(), Arc::new(search));
    }

    pub(crate) fn search_of(&self, kind: &str) -> Option<WidgetSearchFn> {
        lock(&self.searches).get(kind).cloned()
    }

    /// The item a deep link or a search hit landed on, marked while its
    /// section shows: a setting id, or a widget's item key.
    pub fn marked(&self) -> Option<String> {
        lock(&self.state).mark.clone()
    }

    /// Whether `key` is the item to scroll to, taken once: the widget
    /// that draws it scrolls to it.
    pub fn take_key(&self, key: &str) -> bool {
        let mut s = lock(&self.state);
        if s.key.as_deref() == Some(key) {
            s.key = None;
            true
        } else {
            false
        }
    }

    /// Whether `key` is still waiting to be scrolled to.
    pub(crate) fn key_is(&self, key: &str) -> bool {
        lock(&self.state).key.as_deref() == Some(key)
    }

    pub(crate) fn hooks(&self) -> std::sync::MutexGuard<'_, Hooks> {
        lock(&self.hooks)
    }

    /// The diagnostics of the last tree built.
    pub fn diagnostics(&self) -> Diagnostics {
        lock(&self.diagnostics).clone()
    }

    /// The resolved tree: the declared settings' pages (less the settings
    /// a manifest places), then every manifest, checked.
    pub fn tree(&self, app: &AppHandle) -> Tree {
        let entries = app.settings();
        let mut manifests = lock(&self.manifests).clone();
        // The pages the settings window drew itself: shortcuts, gestures,
        // fonts, custom pages, and the window behind Advanced.
        if app
            .service(&crate::settings_window::SETTINGS_WINDOW)
            .is_some()
        {
            manifests.insert(0, crate::settings_window::admin_manifest(app));
        }
        let placed: std::collections::BTreeSet<&str> =
            manifests.iter().flat_map(|m| m.bound_settings()).collect();
        let mut tree = Tree::default();
        tree.merge(generate::settings(
            entries
                .iter()
                .filter(|e| !placed.contains(e.id.as_str()))
                .map(|e| Entry {
                    id: e.id.as_str(),
                    section: &e.section,
                }),
        ));
        let mut d = Diagnostics::new();
        let mut scopes: Vec<String> = tree.scopes.iter().map(|s| s.meta.id.clone()).collect();
        for m in &manifests {
            scopes.extend(m.scopes.iter().map(|s| s.meta.id.clone()));
        }
        let exists = |id: &str| entries.iter().any(|e| e.id.as_str() == id);
        for mut m in manifests {
            if m.version > model::FORMAT {
                d.warn(
                    "admin-newer",
                    None,
                    "manifest",
                    format!(
                        "manifest format {} is newer than {}: left out",
                        m.version,
                        model::FORMAT
                    ),
                );
                continue;
            }
            for x in m.check(&scopes, &exists).all() {
                d.push_quiet(x.clone());
            }
            tree.merge(m);
        }
        // Settings export and import: their wizards offer the places of
        // the tree built so far.
        let mut m = transfer::manifest(self, app, &tree);
        for x in m.check(&scopes, &exists).all() {
            d.push_quiet(x.clone());
        }
        tree.merge(m);
        *lock(&self.diagnostics) = d;
        tree
    }

    /// The value layers.
    pub fn layers(&self) -> Layers {
        lock(&self.layers).clone()
    }

    /// Offers the library or workspace layer for editing (the application
    /// opened one), or takes it back.
    pub fn offer(&self, scope: ValueScope, on: bool) {
        lock(&self.layers).offer(scope, on);
    }

    /// The editable value scopes offered (`app` always).
    pub fn offered(&self) -> Vec<ValueScope> {
        lock(&self.layers).offered().to_vec()
    }

    /// Replaces a layer's values (setting id to JSON), as the application
    /// keeps them with its library or workspace; the effective values
    /// change at once.
    pub fn load_layer(&self, app: &AppHandle, scope: ValueScope, values: BTreeMap<String, Json>) {
        let store = app.store();
        let values = values
            .into_iter()
            .map(|(k, v)| (SettingId::new(&k).map_or(k, |id| id.path()), v))
            .collect();
        let writes = {
            let s = lock(&store);
            lock(&self.layers).load(scope, values, &|p| s.get(p).map(|e| e.value.to_json()))
        };
        for (path, v) in writes {
            write(app, &path, &v);
        }
    }

    /// A layer's values, by setting id: what the application saves.
    pub fn export_layer(&self, scope: ValueScope) -> BTreeMap<String, Json> {
        lock(&self.layers)
            .export(scope)
            .into_iter()
            .map(|(p, v)| (p.strip_prefix("settings/").unwrap_or(&p).to_string(), v))
            .collect()
    }

    /// A command line override (`--set key=value`): shown as the `command
    /// line` origin, never edited.
    pub fn set_run(&self, app: &AppHandle, id: &SettingId, value: Json) -> Result<(), String> {
        let path = id.path();
        let now = current(app, &path)?;
        let w = lock(&self.layers).set_run(&path, value, &now);
        if let Some(v) = w {
            write(app, &path, &v);
        }
        Ok(())
    }

    /// Sets a setting's value in a value scope; `app` writes the store as
    /// the settings form does (an undo step) unless a layer above holds
    /// it.
    pub fn set_in(
        &self,
        app: &AppHandle,
        scope: ValueScope,
        id: &SettingId,
        value: Json,
    ) -> Result<(), String> {
        let path = id.path();
        let now = current(app, &path)?;
        if !scope.editable() {
            return Err("the command line's values are not edited here".into());
        }
        if scope != ValueScope::App && !self.offered().contains(&scope) {
            return Err(format!("no {} is open", scope.name()));
        }
        let (w, plain) = {
            let mut l = lock(&self.layers);
            let plain = scope == ValueScope::App && l.origin(&path) == ValueScope::App;
            (l.set(scope, &path, value, &now), plain)
        };
        match (w, plain) {
            (Some(v), true) => {
                let ty = descriptor(app, &path)?.ty;
                let v = Value::from_json(ty, &v)?;
                lock(&app.store())
                    .set(Set::new(&path, Change::To(v), Origin::User).by(&OwnerId::FRAMEWORK))
                    .map(|_| ())
                    .map_err(|e| format!("{e:?}"))
            }
            (Some(v), false) => {
                write(app, &path, &v);
                Ok(())
            }
            (None, _) => Ok(()),
        }
    }

    /// Takes a value scope's value of a setting back: `app` goes back to
    /// the default, a layer above to what is under it.
    pub fn clear_in(
        &self,
        app: &AppHandle,
        scope: ValueScope,
        id: &SettingId,
    ) -> Result<(), String> {
        let path = id.path();
        let now = current(app, &path)?;
        let default = descriptor(app, &path)?.default.to_json();
        let plain = scope == ValueScope::App && lock(&self.layers).origin(&path) == ValueScope::App;
        if plain {
            return lock(&app.store())
                .set(Set::new(&path, Change::Reset, Origin::User).by(&OwnerId::FRAMEWORK))
                .map(|_| ())
                .map_err(|e| format!("{e:?}"));
        }
        let w = lock(&self.layers).clear(scope, &path, &now, &default);
        if let Some(v) = w {
            write(app, &path, &v);
        }
        Ok(())
    }

    /// A `read_only` command's answer: `None` while it is being read (the
    /// command is asked for, run before the next frame; its result is
    /// taken from the runs).
    pub fn read(
        &self,
        app: &AppHandle,
        command: &str,
        args: &Json,
    ) -> Option<Result<Json, String>> {
        let args = if args.is_null() {
            json!({})
        } else {
            args.clone()
        };
        let key = read_key(command, &args);
        let mut r = lock(&self.reads);
        if let Some(a) = r.answers.get(&key) {
            return Some(a.clone());
        }
        if let Some((id, args, before)) = r.asked.get(&key).cloned() {
            // Its run, by its arguments, else the last of the command (a
            // host may record them completed with their defaults); not
            // one from before it was asked.
            let runs = app.history();
            // (A full history drops runs: the count proves nothing then.)
            if runs.len() < egui_pop_model::commands::HISTORY
                && runs.iter().filter(|x| x.id == id).count() == before
            {
                return None;
            }
            let run = runs
                .iter()
                .rev()
                .find(|x| x.id == id && x.args == args)
                .or_else(|| runs.iter().rev().find(|x| x.id == id))?
                .clone();
            r.asked.remove(&key);
            r.answers.insert(key, run.result.clone());
            return Some(run.result);
        }
        let answer = match CommandId::new(command) {
            Err(e) => Err(format!("{e:?}")),
            Ok(id) => match app.command(&id) {
                None => Err(format!("no command `{command}`")),
                Some(spec) if !spec.read_only => {
                    Err(format!("`{command}` is not read-only: it is not read here"))
                }
                Some(_) => {
                    let before = app.history().iter().filter(|x| x.id == id).count();
                    app.run(id.clone(), args.clone());
                    r.asked.insert(key, (id, args, before));
                    return None;
                }
            },
        };
        r.answers.insert(key, answer.clone());
        Some(answer)
    }

    /// Forgets the answers read, and the reads still in flight (their
    /// answer may predate the change): they are read again when shown.
    pub fn refresh(&self) {
        let mut r = lock(&self.reads);
        r.answers.clear();
        r.asked.clear();
    }

    /// Looks at the journal since the last look: a command that changes
    /// something, or an event matching one of `patterns` (`name`, or
    /// `prefix.*`), and the answers are read again.
    pub(crate) fn watch(&self, app: &AppHandle, patterns: &[String]) {
        let since = lock(&self.reads).seq;
        let events = app.events_since(since.unwrap_or(0));
        let Some(last) = events.last().map(|e| e.seq) else {
            return;
        };
        lock(&self.reads).seq = Some(last);
        if since.is_none() {
            return;
        }
        let matches = |name: &str| {
            patterns.iter().any(|p| match p.strip_suffix('*') {
                Some(prefix) => name.starts_with(prefix),
                None => name == p,
            })
        };
        let changed = events.iter().any(|e| {
            matches(&e.name)
                || (e.name == "command.done"
                    && e.data["command"]
                        .as_str()
                        .and_then(|c| CommandId::new(c).ok())
                        .and_then(|c| app.command(&c))
                        .is_some_and(|s| !s.read_only))
        });
        if changed {
            self.refresh();
        }
    }

    /// The row's target scope ("Set for").
    pub(crate) fn target(&self, path: &str) -> Option<ValueScope> {
        lock(&self.state).target.get(path).copied()
    }

    pub(crate) fn set_target(&self, path: &str, scope: ValueScope) {
        lock(&self.state).target.insert(path.into(), scope);
    }

    /// The user's settings document as saved: the `app` values in place of
    /// those a layer above holds.
    pub(crate) fn app_doc(&self, app: &AppHandle, values: &mut BTreeMap<String, Json>) {
        let store = app.store();
        let s = lock(&store);
        lock(&self.layers).app_doc(values, &|p| {
            s.get(p).map(|e| e.descriptor.default.to_json())
        });
    }
}

fn current(app: &AppHandle, path: &str) -> Result<Json, String> {
    lock(&app.store())
        .get(path)
        .map(|e| e.value.to_json())
        .ok_or_else(|| format!("no setting at `{path}`"))
}

fn descriptor(app: &AppHandle, path: &str) -> Result<Descriptor, String> {
    lock(&app.store())
        .get(path)
        .map(|e| e.descriptor.clone())
        .ok_or_else(|| format!("no setting at `{path}`"))
}

/// Writes an effective value a layer change gives: no undo step (the
/// layers would not follow it back).
fn write(app: &AppHandle, path: &str, v: &Json) {
    let store = app.store();
    let mut s = lock(&store);
    let Some(ty) = s.get(path).map(|e| e.descriptor.ty) else {
        return;
    };
    if let Ok(v) = Value::from_json(ty, v) {
        let _ = s.set(
            Set::new(path, Change::To(v), Origin::User)
                .force()
                .untracked()
                .by(&OwnerId::FRAMEWORK),
        );
    }
}

/// Draws a setting a custom page draws as the Settings app's row, while
/// the app draws the page; `false` elsewhere.
pub(crate) fn setting_row(ui: &mut Ui, app: &AppHandle, id: &SettingId) -> bool {
    shell::setting_row(ui, app, id)
}

/// A value scope from its name.
pub fn scope_named(name: &str) -> Option<ValueScope> {
    ValueScope::ALL.into_iter().find(|s| s.name() == name)
}

/// Whether the application's administration app is open.
pub fn is_open(app: &AppHandle) -> bool {
    app.service(&ADMIN).is_some_and(|a| a.is_open())
}

/// Where the width of the tree of sections is kept, in points (dragged,
/// not typed: no setting of its own in the settings).
pub const NAV_WIDTH: &str = "settings/admin.nav_width";

/// The tree's width when none is kept.
pub const NAV_WIDTH_DEFAULT: f32 = 220.0;

/// The width of the tree of sections, as kept (the room in the page may
/// hold it smaller).
pub fn nav_width(app: &AppHandle) -> f32 {
    lock(&app.store())
        .get(NAV_WIDTH)
        .and_then(|e| e.value.as_f64())
        .map_or(NAV_WIDTH_DEFAULT, |w| w as f32)
}

/// Keeps the width of the tree of sections.
pub fn want_nav_width(app: &AppHandle, width: f32) {
    let set = Set::new(
        NAV_WIDTH,
        Change::To(Value::Float(f64::from(width))),
        Origin::User,
    )
    .by(&OwnerId::FRAMEWORK)
    .untracked();
    if let Err(e) = lock(&app.store()).set(set) {
        log::warn!(target: "egui_pop", "{NAV_WIDTH}: {e:?}");
    }
}

/// Whether it opens in a window of its own.
pub fn in_window(app: &AppHandle) -> bool {
    app.setting::<bool>(&WINDOW).unwrap_or(false)
}

/// The plugin: the `admin` service, its commands, its setting and its
/// overlay. Not in `DefaultPlugins` yet: the settings window keeps
/// `Ctrl+,` until the migration.
#[derive(Debug, Default)]
pub struct AdminPlugin;

fn admin(call: &crate::app::Call<'_>) -> Result<Arc<Admin>, String> {
    call.app.service(&ADMIN).ok_or_else(|| "no admin".into())
}

fn key_arg(call: &crate::app::Call<'_>) -> Result<SettingId, String> {
    let key = call
        .args
        .get("key")
        .and_then(Json::as_str)
        .ok_or("`key` is missing")?;
    SettingId::new(key).map_err(|e| format!("{e:?}"))
}

fn scope_arg(call: &crate::app::Call<'_>) -> Result<ValueScope, String> {
    let name = call
        .args
        .get("scope")
        .and_then(Json::as_str)
        .unwrap_or("app");
    scope_named(name).ok_or_else(|| format!("no value scope `{name}`"))
}

impl Plugin for AdminPlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("egui_pop.admin")
    }

    fn title(&self) -> String {
        "Settings app".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let service = Admin::new();
        crate::settings_window::register_kinds(&service);
        transfer::hooks(&service);
        ctx.service(&ADMIN, service)?;
        ctx.local_scope(
            SURFACE,
            StoreScope::new(egui_pop_model::history::SETTINGS, "Settings"),
        );
        ctx.setting(
            WINDOW,
            Descriptor::bool("Settings in their own window", false).description(
                "Opens the settings in a window of their own rather than over the main window.",
            ),
        )?;
        lock(&ctx.app().store())
            .declare(
                ctx.owner(),
                NAV_WIDTH,
                Descriptor::float(
                    "Settings sections width",
                    0.0,
                    4096.0,
                    f64::from(NAV_WIDTH_DEFAULT),
                ),
            )
            .map_err(|e| PluginError::Failed(format!("{NAV_WIDTH}: {e:?}")))?;
        ctx.command(
            CommandSpec::new(CMD_OPEN, "Settings", "File")
                .icon("settings")
                .description(
                    "Every setting as full pages: families and their sections in a tree on the side, one search.",
                )
                .args(json!({ "path": "", "key": "", "mode": "", "wizard": "" })),
            |call| {
                let a = admin(call)?;
                // A wizard: its own page.
                if let Some(id) = call
                    .args
                    .get("wizard")
                    .and_then(Json::as_str)
                    .filter(|w| !w.is_empty())
                {
                    return a.open_wizard(call.app, id, None, Default::default());
                }
                if let Some(mode) = call.args.get("mode").and_then(Json::as_str)
                    && !mode.is_empty()
                {
                    let window = match mode {
                        "window" => true,
                        "overlay" => false,
                        other => return Err(format!("no mode `{other}`")),
                    };
                    a.set_in(call.app, ValueScope::App, &WINDOW, json!(window))?;
                }
                // A path first, else a setting id alone.
                let place = ["path", "key"].iter().find_map(|k| {
                    call.args
                        .get(*k)
                        .and_then(Json::as_str)
                        .filter(|p| !p.is_empty())
                });
                a.open(call.app, place)?;
                let (scope, section, _) = a.shown();
                Ok(json!({ "open": true, "scope": scope, "section": section, "window": in_window(call.app) }))
            },
        )?;
        ctx.command(
            CommandSpec::new(CMD_CLOSE, "Close settings", "File").hidden(),
            |call| {
                admin(call)?.close();
                Ok(json!({ "open": false }))
            },
        )?;
        ctx.command(
            CommandSpec::new(CMD_POP_OUT, "Settings: pop out", "File")
                .description(
                    "Moves the settings between the main window and a window of their own.",
                )
                .hidden(),
            |call| {
                let window = !in_window(call.app);
                admin(call)?.set_in(call.app, ValueScope::App, &WINDOW, json!(window))?;
                Ok(json!({ "window": window }))
            },
        )?;
        ctx.command(
            CommandSpec::new(CMD_SET, "Set a setting in a value scope", "File")
                .hidden()
                .args(json!({ "key": "", "scope": "app", "value": null })),
            |call| {
                let id = key_arg(call)?;
                let scope = scope_arg(call)?;
                let value = call
                    .args
                    .get("value")
                    .cloned()
                    .ok_or("`value` is missing")?;
                admin(call)?.set_in(call.app, scope, &id, value)?;
                Ok(json!({ "key": id.as_str(), "scope": scope.name() }))
            },
        )?;
        ctx.command(
            CommandSpec::new(CMD_RESET, "Reset a setting in a value scope", "File")
                .hidden()
                .args(json!({ "key": "", "scope": "app" })),
            |call| {
                let id = key_arg(call)?;
                let scope = scope_arg(call)?;
                admin(call)?.clear_in(call.app, scope, &id)?;
                Ok(json!({ "key": id.as_str(), "scope": scope.name() }))
            },
        )?;
        wizard::commands(ctx)?;
        transfer::commands(ctx)?;
        ctx.block(
            zones::OVERLAYS,
            Block::add(BlockId::from_static("egui_pop.admin"), 6, shell::show),
        );
        let w = ctx.app().downgrade();
        ctx.collector(
            egui_pop_core::debug::FnCollector::new(
                "admin",
                "The administration app (Settings as full pages): open or not, the mode, the scope, section and search shown, the value scopes offered, the values set above the app layer and their origin, the resolved tree (scopes, sections, items) and the diagnostics of its manifests.",
            )
            .snapshot(move |_| match w.get() {
                Ok(app) => describe(&app),
                Err(_) => Json::Null,
            }),
        )?;
        Ok(())
    }
}

/// What the `admin` collector shows.
fn describe(app: &AppHandle) -> Json {
    let Some(a) = app.service(&ADMIN) else {
        return Json::Null;
    };
    let tree = a.tree(app);
    let (scope, section, filter) = a.shown();
    let layers = a.layers();
    let mut origins = BTreeMap::new();
    for (_, f) in tree.fields() {
        if let Some(id) = f.setting_id() {
            let o = layers.origin(&format!("settings/{id}"));
            if o != ValueScope::App {
                origins.insert(id.to_string(), o.name());
            }
        }
    }
    json!({
        "open": a.is_open(),
        "window": in_window(app),
        "scope": scope,
        "section": section,
        "filter": filter,
        "marked": a.marked(),
        "offered": a.offered().iter().map(|s| s.name()).collect::<Vec<_>>(),
        "origins": origins,
        "tree": tree,
        "wizard": a.wizard_state(app),
        "lastFinished": a.last_finished(),
        "hooks": a.hooks().list().into_iter().map(|(n, k)| json!({"name": n, "kind": k})).collect::<Vec<_>>(),
        "diagnostics": a.diagnostics().all().iter().map(|d| json!({"code": d.code, "subject": d.subject, "message": d.message})).collect::<Vec<_>>(),
    })
}
