//! The application: an [`AppBuilder`] takes plugins and a theme, sorts and
//! connects the plugins, and gives an [`App`] that draws the shell (menu
//! bar, zones, panels, the command palette) and dispatches keys. It runs
//! in eframe (feature `eframe`) or headless, driven by tests.

use std::cell::{RefCell, RefMut};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::rc::{Rc, Weak};
use std::sync::{Arc, Mutex};

use egui::{Context, Id, Ui};
use egui_pop_core::binding::SharedStore;
use egui_pop_core::debug::{DebugBridge, Feedback, FnCollector};
use egui_pop_core::focus_mode::{self, FocusModes, Preference, SystemFocus};
use egui_pop_core::keys::KeyDispatcher;
use egui_pop_core::kit::KitSource;
use egui_pop_core::overlay;
use egui_pop_core::persist::{Persist, PersistError, Writer};
use egui_pop_core::state;
use egui_pop_core::strings;
use egui_pop_core::theme::{self, Counterpart, ThemeSpec, Themes};
use egui_pop_core::{a11y, fault, focus, lock};

/// The theme was installed in this context.
const THEME_INSTALLED: state::Slot<bool> = state::Slot::new("api.app.theme_installed");
/// The width the top bar's blocks took after the menus, last pass.
const TOP_BAR_RESERVE: state::Slot<f32> = state::Slot::new("api.app.top_bar_reserve");
use egui_pop_model::commands::{CommandSpec, Commands, Origin, Run};
use egui_pop_model::diagnostics::{Diagnostic, Diagnostics};
use egui_pop_model::ids::{
    CommandId, MenuId, OwnerId, PanelId, PluginId, SettingId, StepId, StoryId, ZoneId,
};
use egui_pop_model::keymap::{Binding, Keymap, Layer, Trace};
use egui_pop_model::menus::{Item, MenuItem, Menus, sort_items};
use egui_pop_model::order::{Node, order};
use egui_pop_model::panels::{Area, PanelSpec, Panels};
use egui_pop_model::registry::{OwnedList, Registry};
use egui_pop_model::values::{Change, Set, Store, Value, settings_schema};
use egui_pop_model::when::WhenContext;
use serde_json::{Value as Json, json};

use crate::ctx::{Ctx, CtxError};
use crate::hooks::{Alter, Hook, Lifecycle, LifecycleFn, On, PaletteEntry, SettingEntry};
use crate::host::{Dispatch, Event, Host, LocalHost, Pending, Reply, Seq, Waker, source_of};
use crate::plugin::{BuildError, Plugin, PluginError, PluginInfo};
use crate::services::{ServiceToken, Services};
use crate::steps::{HookFn, HookInfo, Outcome, StepHook, StepInfo, StepSpec, StepTrace};
use crate::stories::{Story, StoryInfo};
use crate::zones::{self, Block, DrawFn, ZoneInfo, ZoneTable, draw_chain};

/// The user gives a command new keys, or none (`{ "command", "keys" }`).
pub const STEP_REBIND: StepId = StepId::from_static("keys.rebind");
/// A keymap document is loaded into a layer (`{ "layer", "bindings" }`).
pub const STEP_LOAD_KEYMAP: StepId = StepId::from_static("keys.load");

/// The main menu bar.
pub const MENU_MAIN: MenuId = MenuId::from_static("app.main");
pub const MENU_FILE: MenuId = MenuId::from_static("app.menu.file");
pub const MENU_EDIT: MenuId = MenuId::from_static("app.menu.edit");
pub const MENU_VIEW: MenuId = MenuId::from_static("app.menu.view");
pub const MENU_HELP: MenuId = MenuId::from_static("app.menu.help");

/// The framework's commands.
pub use crate::palette::CMD_PALETTE;
pub const CMD_QUIT: CommandId = CommandId::from_static("app.quit");
/// Undo and redo: the keys, menu items and `when` checks are
/// egui-pop's, what an undo does is the host's ([`Host::undo`]), each
/// run going through the [`STEP_UNDO`] and [`STEP_REDO`] steps.
pub const CMD_UNDO: CommandId = CommandId::from_static("edit.undo");
pub const CMD_REDO: CommandId = CommandId::from_static("edit.redo");
/// The steps of an undo and a redo (ADR 0011): a before-hook may veto
/// one (a gesture in progress) or redirect it, a run-hook does it instead
/// of the host, an after-hook follows. Their arguments: `label` (what it
/// takes back, as the menu said) and `origin`. Their result:
/// `{"undone": label}` (`{"redone": label}`), the label `null` when there
/// was nothing. Besides the step's own signal, the journal says
/// `edit.undone` / `edit.redone` with the label, or `edit.undo_failed` /
/// `edit.redo_failed` with the error or the veto.
pub const STEP_UNDO: StepId = StepId::from_static("edit.undo");
pub const STEP_REDO: StepId = StepId::from_static("edit.redo");

/// Commands that run in the interface rather than through the host's
/// dispatch: their run calls the host itself (undo and redo call
/// `Host::undo` and `Host::redo`, the context stack's commands
/// `Host::contexts`, which a dispatch would hold).
const IN_INTERFACE: &[CommandId] = &[
    CMD_UNDO,
    CMD_REDO,
    crate::contexts::CMD_BACK,
    crate::contexts::CMD_UP,
    crate::contexts::CMD_ENTER,
    crate::contexts::CMD_READ,
    crate::contexts::CMD_REQUEST,
    crate::contexts::CMD_GO,
    crate::contexts::CMD_SET_MODE,
    crate::contexts::CMD_HOLD,
    crate::contexts::CMD_RELEASE,
];
pub const CMD_SAVE: CommandId = CommandId::from_static("app.save");

/// What an overlay is, as [`AppHandle::overlay_rects`] reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OverlayKind {
    /// The dialog shown (`dialogs`).
    Dialog,
    /// Another modal overlay (the settings window, the palette).
    Modal,
    Popover,
    /// A menu egui-pop opened as an overlay (`menus::popup_at`).
    Menu,
    /// The toast stack.
    Toasts,
    /// The workbench's drop hint, while a tab is dragged over another
    /// area (named `drop:<panel>:<area>`).
    DropHint,
    /// One of egui's popups: an open menu of the menu bar, a context
    /// menu, a combo box.
    Popup,
}

/// An overlay on screen: what it is, a name for it (`dialog:3`, the
/// overlay's label, `toasts`), where it was drawn last, in points.
#[derive(Clone, Debug, PartialEq)]
pub struct OverlayRect {
    pub kind: OverlayKind,
    pub id: String,
    pub rect: egui::Rect,
}

/// The theme, as a service: read it, ask for another (installed between
/// frames), and choose between its light and dark variants when it has
/// a counterpart (the [`SETTING_THEME_PREFERENCE`] setting).
pub struct ThemeService {
    spec: Mutex<ThemeSpec>,
    themes: Themes,
    kits: Vec<egui_pop_core::tokens::kit::KitInfo>,
    store: SharedStore,
    /// Whether each theme met is a dark one, by name.
    dark: Mutex<BTreeMap<String, bool>>,
}

impl fmt::Debug for ThemeService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ThemeService")
            .field("spec", &self.spec)
            .field("kits", &self.kits)
            .finish_non_exhaustive()
    }
}

impl ThemeService {
    pub fn spec(&self) -> ThemeSpec {
        lock(&self.spec).clone()
    }

    /// Asks for a theme; installed before the next frame.
    pub fn set(&self, spec: ThemeSpec) {
        *lock(&self.spec) = spec;
    }

    /// Changes the theme in place.
    pub fn update(&self, f: impl FnOnce(&mut ThemeSpec)) {
        f(&mut lock(&self.spec));
    }

    pub fn themes(&self) -> &Themes {
        &self.themes
    }

    /// The kits added (`AppBuilder::kit`, then the user's kits), each a
    /// theme of [`ThemeService::themes`] under its name: what a theme
    /// picker lists.
    pub fn kits(&self) -> &[egui_pop_core::tokens::kit::KitInfo] {
        &self.kits
    }

    /// The light or dark mode asked for (the [`SETTING_THEME_PREFERENCE`]
    /// setting): the system's by default. It chooses between the theme
    /// and its counterpart; a theme alone keeps its own mode.
    pub fn preference(&self) -> egui::ThemePreference {
        lock(&self.store)
            .get(&format!("settings/{SETTING_THEME_PREFERENCE}"))
            .and_then(|e| e.value.as_choice().and_then(parse_preference))
            .unwrap_or(egui::ThemePreference::System)
    }

    /// Asks for a light or dark mode, or the system's (written to the
    /// setting); applied before the next frame.
    pub fn set_preference(&self, preference: egui::ThemePreference) {
        let set = Set::new(
            &format!("settings/{SETTING_THEME_PREFERENCE}"),
            Change::To(Value::Choice(preference_name(preference).into())),
            Origin::User,
        )
        .by(&OwnerId::FRAMEWORK)
        .untracked();
        if let Err(e) = lock(&self.store).set(set) {
            log::warn!(target: "egui_pop", "theme preference: {e}");
        }
    }

    /// The theme's counterpart, its other variant, if it has one.
    pub fn counterpart(&self) -> Option<String> {
        self.themes.counterpart(&self.spec())
    }

    /// Whether a theme is a dark one (its panels are dark).
    pub fn is_dark(&self, name: &str) -> bool {
        if let Some(d) = lock(&self.dark).get(name) {
            return *d;
        }
        let d = self.themes.resolve(&ThemeSpec::new(name).alone()).is_dark();
        lock(&self.dark).insert(name.to_string(), d);
        d
    }

    fn mode_of(&self, name: &str) -> egui::Theme {
        if self.is_dark(name) {
            egui::Theme::Dark
        } else {
            egui::Theme::Light
        }
    }

    /// The theme shown with the preference: the spec's base, or its
    /// counterpart when the preference (or the system's mode, with
    /// `System`) asks for the other mode.
    pub fn shown(&self, ctx: &Context) -> String {
        let spec = self.spec();
        let Some(other) = self.themes.counterpart(&spec) else {
            return spec.base;
        };
        let base_mode = self.mode_of(&spec.base);
        if self.mode_of(&other) == base_mode {
            return spec.base;
        }
        let wanted = match self.preference() {
            egui::ThemePreference::Dark => egui::Theme::Dark,
            egui::ThemePreference::Light => egui::Theme::Light,
            egui::ThemePreference::System => ctx.system_theme().unwrap_or(base_mode),
        };
        if wanted == base_mode {
            spec.base
        } else {
            other
        }
    }

    /// Picks a theme, as a theme picker does: it is shown. Its
    /// counterpart comes from the token files; when the preference would
    /// show the counterpart, the preference becomes the picked theme's
    /// mode.
    pub fn pick(&self, ctx: &Context, name: &str) {
        self.update(|spec| {
            spec.base = name.to_string();
            spec.counterpart = Counterpart::Auto;
        });
        if self.shown(ctx) != name {
            self.set_preference(self.mode_of(name).into());
        }
    }

    /// Switches between the theme and its counterpart: the preference
    /// becomes the mode of the one not shown. The mode now shown, or why
    /// nothing changed (a theme alone).
    pub fn toggle(&self, ctx: &Context) -> Result<egui::Theme, String> {
        let spec = self.spec();
        let other = self
            .themes
            .counterpart(&spec)
            .filter(|o| self.mode_of(o) != self.mode_of(&spec.base))
            .ok_or_else(|| format!("`{}` has no light or dark counterpart", spec.base))?;
        let shown = self.shown(ctx);
        let next = if shown == spec.base { other } else { spec.base };
        let mode = self.mode_of(&next);
        self.set_preference(mode.into());
        Ok(mode)
    }
}

/// A preference's name in the setting: `system`, `light`, `dark`.
pub fn preference_name(p: egui::ThemePreference) -> &'static str {
    match p {
        egui::ThemePreference::System => "system",
        egui::ThemePreference::Light => "light",
        egui::ThemePreference::Dark => "dark",
    }
}

/// A preference from its name in the setting.
pub fn parse_preference(name: &str) -> Option<egui::ThemePreference> {
    match name {
        "system" => Some(egui::ThemePreference::System),
        "light" => Some(egui::ThemePreference::Light),
        "dark" => Some(egui::ThemePreference::Dark),
        _ => None,
    }
}

/// The setting choosing between a theme's light and dark variants:
/// `system`, `light`, `dark`.
pub const SETTING_THEME_PREFERENCE: SettingId =
    SettingId::from_static("appearance.theme_preference");

/// The command switching between the theme's light and dark variants.
pub const CMD_THEME_TOGGLE: CommandId = CommandId::from_static("view.theme.toggle_light_dark");

/// The theme service's token.
pub const THEME: ServiceToken<ThemeService> = ServiceToken::new("app.theme");

/// The focus mode (click or hover, the system's way and the user's
/// choice), as a service.
pub const FOCUS_MODE: ServiceToken<FocusModes> = ServiceToken::new("app.focus_mode");

/// The setting choosing the focus mode: `system`, `click`, `hover`.
pub const SETTING_FOCUS: SettingId = SettingId::from_static("appearance.focus");

/// The section the framework's settings are shown in.
const FRAMEWORK_SECTION: &str = "Appearance";

/// What a command handler receives.
#[derive(Debug)]
pub struct Call<'a> {
    pub args: &'a Json,
    pub origin: &'a Origin,
    pub app: &'a AppHandle,
}

/// A command handler.
pub type CommandFn = Rc<dyn Fn(&Call<'_>) -> Result<Json, String>>;

/// The registries plugins fill through `Ctx`.
pub(crate) struct Registries {
    /// Declarations, each with its in-process handler if it has one.
    pub commands: Commands<Option<CommandFn>>,
    pub keymap: Keymap,
    pub menus: Menus,
    pub panels: Panels,
    pub hooks: OwnedList<Hook>,
    pub lifecycle: OwnedList<(On, LifecycleFn)>,
    pub settings: Registry<SettingId, String>,
}

impl Registries {
    fn new() -> Self {
        Self {
            commands: Commands::new(),
            keymap: Keymap::new(),
            menus: Menus::new(),
            panels: Panels::new(),
            hooks: OwnedList::new(),
            lifecycle: OwnedList::new(),
            settings: Registry::new(),
        }
    }

    fn remove_owner(&mut self, owner: &OwnerId) {
        self.commands.remove_owner(owner);
        self.keymap.remove_owner(owner);
        self.menus.remove_owner(owner);
        self.panels.remove_owner(owner);
        self.hooks.remove_owner(owner);
        self.lifecycle.remove_owner(owner);
        self.settings.remove_owner(owner);
    }
}

/// Who owns the workspace's layer of keys (`AppHandle::load_keymap`,
/// `AppHandle::rebind_in`).
const WORKSPACE_KEYS: OwnerId = OwnerId::from_static("workspace");

/// The owner of the keys a layer loads or changes: the user's or the
/// workspace's; `None` for the others.
fn layer_owner(layer: Layer) -> Option<OwnerId> {
    match layer {
        Layer::User => Some(OwnerId::USER),
        Layer::Workspace => Some(WORKSPACE_KEYS),
        Layer::Suggestion | Layer::Default => None,
    }
}

/// How the host names where a binding comes from.
type KeySourceNames = Rc<dyn Fn(&Binding) -> Option<String>>;

/// Declarations derived from the registries, after the alter hooks.
#[derive(Clone, Debug, Default)]
pub(crate) struct Derived {
    pub menus: BTreeMap<MenuId, Vec<MenuItem>>,
    pub palette: Vec<PaletteEntry>,
    pub settings: Vec<SettingEntry>,
    /// Each owner's section title, before the hooks (a plugin's title).
    pub sections: BTreeMap<OwnerId, String>,
    pub commands: Vec<(CommandSpec, OwnerId)>,
    /// Every declared panel with its owner, in declaration order.
    pub panels: Vec<(egui_pop_model::panels::PanelSpec, OwnerId)>,
    /// Each command's live key bindings, as shown.
    pub keys: BTreeMap<CommandId, Vec<String>>,
    /// Each command's live bindings, with the layer each comes from.
    pub sources: BTreeMap<CommandId, Vec<Binding>>,
    /// Bindings losing to another on the same keys: loser, winner.
    pub conflicts: Vec<(Binding, Binding)>,
    /// The live bindings of the system scope (global shortcuts).
    pub system: Vec<Binding>,
    /// Each layer's bindings, as saved (what `export_keymap` gives).
    pub layers: BTreeMap<Layer, egui_pop_model::keymap::KeymapDoc>,
    /// Commands whose keys the user changed.
    pub rebound: BTreeSet<CommandId>,
    /// Commands whose keys were changed for the workspace.
    pub rebound_workspace: BTreeSet<CommandId>,
}

/// What a block or a handler asks of the application, done before the
/// next frame.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    Run {
        command: CommandId,
        args: Json,
        origin: Origin,
    },
    /// A command run in a given context (a menu opened there).
    RunIn {
        command: CommandId,
        args: Json,
        origin: Origin,
        context: WhenContext,
    },
    Enable(PluginId, bool),
    /// Gives a command keys of its own in a layer, the user's or the
    /// workspace's (none: no key).
    Rebind {
        command: CommandId,
        keys: Option<String>,
        layer: Layer,
    },
    /// Takes back what a layer (the user's or the workspace's) changed
    /// of a command's keys.
    ResetKeys {
        command: CommandId,
        layer: Layer,
    },
    /// Declares a command after build (`AppHandle::declare`).
    Declare {
        owner: OwnerId,
        spec: CommandSpec,
    },
    /// Removes a command declared after build.
    Undeclare(CommandId),
    /// Loads a keymap document into a layer.
    LoadKeymap {
        doc: egui_pop_model::keymap::KeymapDoc,
        layer: Layer,
    },
    /// A workbench change asked for while the areas were drawn.
    Workbench(crate::workbench::Change),
}

struct PanelView {
    owner: OwnerId,
    draw: Option<DrawFn>,
}

/// The screens auxiliary windows may be on, until egui names monitors.
pub(crate) const SCREENS: [&str; 1] = ["primary"];

struct StoryEntry {
    story: Story,
}

/// An event waiting for the host: source, cause, name, data.
type Held = (String, Option<Seq>, String, Json);

pub(crate) struct Shared {
    ctx: RefCell<Context>,
    store: SharedStore,
    services: RefCell<Services>,
    zones: RefCell<ZoneTable>,
    panels: RefCell<BTreeMap<PanelId, PanelView>>,
    stories: RefCell<Registry<StoryId, StoryEntry>>,
    requests: RefCell<Vec<Request>>,
    context: RefCell<WhenContext>,
    workbench: RefCell<crate::workbench::Workbench>,
    /// Indicators fed by the journal, and the last event fed.
    indicators: RefCell<OwnedList<(crate::strips::Indicator, crate::strips::IndicatorState)>>,
    indicators_seen: std::cell::Cell<Seq>,
    /// The areas are being drawn: workbench changes wait for the frame's
    /// end.
    drawing_areas: std::cell::Cell<bool>,
    /// The centre drawn without margins.
    edge_to_edge: std::cell::Cell<bool>,
    /// The main menu drawn as a bar under the title.
    menu_bar: std::cell::Cell<bool>,
    /// The shell panels' starting sizes (`AppBuilder::shell_panel_size`).
    shell_sizes: RefCell<Vec<(PanelId, f32)>>,
    /// The panels' commands and their shared texts, installed on each
    /// context drawing the application.
    pub(crate) panel_commands: RefCell<strings::PanelCommands>,
    /// The application's name (`AppBuilder::name`).
    name: RefCell<String>,
    diagnostics: RefCell<Diagnostics>,
    derived: RefCell<Derived>,
    plugins: RefCell<Vec<PluginInfo>>,
    history: RefCell<Vec<Run>>,
    bridge: RefCell<DebugBridge>,
    traces: RefCell<Vec<Trace>>,
    pending_keys: RefCell<Option<String>>,
    /// The host; out of its cell while it dispatches.
    host: RefCell<Option<Box<dyn Host>>>,
    /// Events published while the host was dispatching.
    held: RefCell<Vec<Held>>,
    /// The journal kept for a host that keeps none.
    journal: RefCell<crate::host::Journal>,
    /// Commands whose result is to come, by id: how many.
    running: RefCell<BTreeMap<CommandId, usize>>,
    /// When each running command's oldest run started (motion clock).
    running_since: RefCell<BTreeMap<CommandId, f64>>,
    /// Collectors added by plugins.
    collectors: RefCell<Vec<(OwnerId, String)>>,
    /// A shortcuts editor waits for keys: the dispatcher stands aside.
    capturing: std::cell::Cell<bool>,
    /// The shortcuts editor offers to change keys for the workspace.
    workspace_keys: std::cell::Cell<bool>,
    /// How the host names where a binding comes from.
    key_source_names: RefCell<Option<KeySourceNames>>,
    /// Declared steps (ADR 0011), their hooks, their last runs.
    steps: RefCell<Registry<StepId, StepSpec>>,
    step_hooks: RefCell<OwnedList<StepHook>>,
    step_traces: RefCell<std::collections::VecDeque<StepTrace>>,
    /// List providers, by list name, and what each gave last.
    lists: RefCell<OwnedList<crate::menus::Provider>>,
    list_traces: RefCell<BTreeMap<String, crate::menus::ListTrace>>,
    /// The menu open as a popup (`menus::popup_at`).
    popup: RefCell<Option<crate::menus::Popup>>,
    /// Documents written a short while after their last change, when a
    /// persistence directory was given.
    writer: RefCell<Option<Writer>>,
    /// State a module keeps on the application side, by type: what is
    /// neither `Send` nor a service (custom dialogs' bodies, the mount
    /// registry).
    locals: RefCell<BTreeMap<std::any::TypeId, Rc<dyn std::any::Any>>>,
    /// The context stack's projection is writing its keys.
    stack_writing: std::cell::Cell<bool>,
    /// The commands running now, innermost last, with their owners.
    running_commands: RefCell<Vec<(CommandId, Option<OwnerId>)>>,
}

/// Marks a command as running while it lives (what reports name, as
/// the context stack's hand writes).
struct RunningCommand(AppHandle);

impl RunningCommand {
    fn new(app: &AppHandle, command: &CommandId, owner: Option<&OwnerId>) -> Self {
        app.0
            .running_commands
            .borrow_mut()
            .push((command.clone(), owner.cloned()));
        RunningCommand(app.clone())
    }
}

impl Drop for RunningCommand {
    fn drop(&mut self) {
        self.0.0.running_commands.borrow_mut().pop();
    }
}

/// The application at run time, cheap to clone: blocks, panels, stories
/// and command handlers reach services, the store and the command runner
/// through it.
#[derive(Clone)]
pub struct AppHandle(Rc<Shared>);

impl fmt::Debug for AppHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppHandle").finish_non_exhaustive()
    }
}

/// A handle that does not keep the application alive (for collectors).
#[derive(Clone)]
pub(crate) struct WeakApp(Weak<Shared>);

impl WeakApp {
    pub(crate) fn get(&self) -> Result<AppHandle, String> {
        self.0
            .upgrade()
            .map(AppHandle)
            .ok_or_else(|| "the application is gone".to_string())
    }
}

impl AppHandle {
    pub(crate) fn downgrade(&self) -> WeakApp {
        WeakApp(self.weak())
    }

    fn new(ctx: &Context, host: Box<dyn Host>) -> Self {
        let store = host.store();
        AppHandle(Rc::new(Shared {
            ctx: RefCell::new(ctx.clone()),
            store,
            services: RefCell::new(Services::default()),
            zones: RefCell::new(ZoneTable::default()),
            panels: RefCell::new(BTreeMap::new()),
            stories: RefCell::new(Registry::new()),
            requests: RefCell::new(Vec::new()),
            context: RefCell::new(WhenContext::new()),
            workbench: RefCell::new(crate::workbench::Workbench::default()),
            indicators: RefCell::new(OwnedList::new()),
            indicators_seen: std::cell::Cell::new(0),
            drawing_areas: std::cell::Cell::new(false),
            edge_to_edge: std::cell::Cell::new(false),
            menu_bar: std::cell::Cell::new(true),
            shell_sizes: RefCell::default(),
            panel_commands: RefCell::default(),
            name: RefCell::default(),
            diagnostics: RefCell::new(Diagnostics::new()),
            derived: RefCell::new(Derived::default()),
            plugins: RefCell::new(Vec::new()),
            history: RefCell::new(Vec::new()),
            bridge: RefCell::new(DebugBridge::with_core_collectors()),
            traces: RefCell::new(Vec::new()),
            pending_keys: RefCell::new(None),
            host: RefCell::new(Some(host)),
            held: RefCell::new(Vec::new()),
            journal: RefCell::new(crate::host::Journal::default()),
            running: RefCell::new(BTreeMap::new()),
            running_since: RefCell::new(BTreeMap::new()),
            collectors: RefCell::new(Vec::new()),
            capturing: std::cell::Cell::new(false),
            workspace_keys: std::cell::Cell::new(false),
            key_source_names: RefCell::new(None),
            steps: RefCell::new(Registry::new()),
            step_hooks: RefCell::new(OwnedList::new()),
            step_traces: RefCell::new(std::collections::VecDeque::new()),
            lists: RefCell::new(OwnedList::new()),
            list_traces: RefCell::new(BTreeMap::new()),
            popup: RefCell::new(None),
            writer: RefCell::new(None),
            locals: RefCell::new(BTreeMap::new()),
            stack_writing: std::cell::Cell::new(false),
            running_commands: RefCell::new(Vec::new()),
        }))
    }

    /// The state of type `T` kept on the application side, made on first
    /// use.
    pub(crate) fn local<T: Default + 'static>(&self) -> Rc<T> {
        let id = std::any::TypeId::of::<T>();
        let any = self
            .0
            .locals
            .borrow_mut()
            .entry(id)
            .or_insert_with(|| Rc::new(T::default()) as Rc<dyn std::any::Any>)
            .clone();
        any.downcast::<T>()
            .unwrap_or_else(|_| unreachable!("kept under its own type"))
    }

    /// Records a diagnostic found at run time (ADR 0003).
    pub(crate) fn report(&self, d: Diagnostic) {
        self.0.diagnostics.borrow_mut().push(d);
    }

    fn weak(&self) -> Weak<Shared> {
        Rc::downgrade(&self.0)
    }

    /// The debug bridge's collectors, their descriptions and actions.
    pub fn debug_describe(&self) -> Json {
        self.0.bridge.borrow().describe()
    }

    /// Every collector's snapshot.
    pub fn debug_snapshot(&self) -> Json {
        self.0.bridge.borrow().snapshot(&self.egui())
    }

    /// Calls a collector's action; always returns a feedback block.
    /// `commands.run` is queued here (the App runs it before the next
    /// frame); `App::debug_call` runs it at once.
    pub fn debug_call(&self, collector: &str, action: &str, args: &Json) -> Feedback {
        self.0
            .bridge
            .borrow_mut()
            .call(&self.egui(), collector, action, args)
    }

    /// The feedback blocks of the last bridge calls.
    pub fn debug_log(&self) -> Vec<Feedback> {
        self.0.bridge.borrow().log().cloned().collect()
    }

    /// The last key resolutions, oldest first.
    pub fn key_traces(&self) -> Vec<Trace> {
        self.0.traces.borrow().clone()
    }

    /// The chords of a shortcut in progress.
    pub fn pending_keys(&self) -> Option<String> {
        self.0.pending_keys.borrow().clone()
    }

    /// The egui context the application draws in.
    pub fn egui(&self) -> Context {
        self.0.ctx.borrow().clone()
    }

    /// Records commands derived from a panel (see
    /// `strings::PanelCommand`), installed on the current context now and
    /// on each context the application is drawn in.
    pub(crate) fn add_panel_commands(
        &self,
        entries: impl IntoIterator<Item = (String, strings::PanelCommand)>,
    ) {
        let mut all = self.0.panel_commands.borrow_mut();
        Arc::make_mut(&mut all).extend(entries);
        strings::set_panel_commands(&self.egui(), all.clone());
    }

    /// The application's name (`AppBuilder::name`); empty when none was
    /// given.
    pub fn name(&self) -> String {
        self.0.name.borrow().clone()
    }

    /// The value store.
    pub fn store(&self) -> SharedStore {
        self.0.store.clone()
    }

    /// The persistence directory (`AppBuilder::persist`), if one was
    /// given.
    pub fn persist(&self) -> Option<Persist> {
        self.0.writer.borrow().as_ref().map(|w| w.persist().clone())
    }

    /// Writes a document under the persistence directory about a second
    /// after its last change (the debounced writer of
    /// `egui_pop_core::persist`): offering the same text again does
    /// nothing, a new one moves the deadline. `false` without a
    /// directory. Pending documents are written at the deadline, by
    /// [`AppHandle::flush_documents`] and by `App::save`.
    pub fn save_later(&self, name: &str, text: String) -> bool {
        let ctx = self.egui();
        let now = egui_pop_core::motion::now(&ctx);
        let mut w = self.0.writer.borrow_mut();
        let Some(w) = w.as_mut() else {
            return false;
        };
        if w.offer(name, text, now) {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(
                egui_pop_core::persist::DEBOUNCE,
            ));
        }
        true
    }

    /// Records that a document's text is on disk (just loaded):
    /// [`AppHandle::save_later`] writes it again only once it changes.
    pub fn saved_already(&self, name: &str, text: &str) {
        if let Some(w) = self.0.writer.borrow_mut().as_mut() {
            w.written(name, text);
        }
    }

    /// Writes every document waiting in the debounced writer now; what
    /// was written and how it went.
    pub fn flush_documents(&self) -> Vec<(String, Result<(), PersistError>)> {
        self.0
            .writer
            .borrow_mut()
            .as_mut()
            .map(Writer::flush)
            .unwrap_or_default()
    }

    /// The documents waiting in the debounced writer.
    pub fn pending_documents(&self) -> Vec<String> {
        self.0
            .writer
            .borrow()
            .as_ref()
            .map(|w| w.pending().into_iter().map(str::to_string).collect())
            .unwrap_or_default()
    }

    /// Appends an event to the host's journal (held until the host is
    /// free when published from a command it is running).
    pub fn publish(&self, source: &str, cause: Option<Seq>, name: &str, data: Json) {
        match self.0.host.borrow_mut().as_mut() {
            Some(h) => {
                self.publish_to(h.as_mut(), source, cause, name, data);
            }
            None => self
                .0
                .held
                .borrow_mut()
                .push((source.into(), cause, name.into(), data)),
        }
    }

    /// Publishes on the host's journal, or on egui-pop's when the host
    /// keeps none.
    fn publish_to(
        &self,
        h: &mut dyn Host,
        source: &str,
        cause: Option<Seq>,
        name: &str,
        data: Json,
    ) -> Seq {
        // The data goes to whichever keeps it: a host keeping no journal
        // gets it back unused, so it is cloned only for the fallback.
        match h.publish(source, cause, name, data.clone()) {
            Some(seq) => seq,
            None => self
                .0
                .journal
                .borrow_mut()
                .publish(source, cause, name, data),
        }
    }

    /// The journal's events after `seq` (empty while the host is
    /// dispatching), read through the host.
    pub fn events_since(&self, seq: Seq) -> Vec<Event> {
        self.0
            .host
            .borrow()
            .as_ref()
            .map(|h| {
                h.since(seq)
                    .unwrap_or_else(|| self.0.journal.borrow().since(seq))
            })
            .unwrap_or_default()
    }

    /// Reads the host (`None` while it dispatches a command).
    fn with_host<R>(&self, f: impl FnOnce(&dyn Host) -> R) -> Option<R> {
        self.0.host.borrow().as_deref().map(f)
    }

    /// Changes the host (`None` while it dispatches a command).
    pub(crate) fn with_host_mut<R>(
        &self,
        f: impl FnOnce(&mut (dyn Host + 'static)) -> R,
    ) -> Option<R> {
        let mut host = self.0.host.borrow_mut();
        host.as_mut().map(|h| f(h.as_mut()))
    }

    /// Whether there is something to undo, as the host says (`false`
    /// while the host dispatches a command).
    pub fn can_undo(&self) -> bool {
        self.with_host(|h| h.can_undo()).unwrap_or(false)
    }

    /// Whether there is something to redo.
    pub fn can_redo(&self) -> bool {
        self.with_host(|h| h.can_redo()).unwrap_or(false)
    }

    /// What an undo would take back ("exposure change").
    pub fn undo_label(&self) -> Option<String> {
        self.with_host(|h| h.undo_label()).flatten()
    }

    /// What a redo would do again.
    pub fn redo_label(&self) -> Option<String> {
        self.with_host(|h| h.redo_label()).flatten()
    }

    /// A command's title as menus show it now, in the interface's
    /// language: undo and redo name what they would do ("Undo exposure
    /// change").
    pub fn command_title(&self, spec: &CommandSpec) -> String {
        let ctx = self.egui();
        let title = strings::command_title(&ctx, spec.id.as_str(), &spec.title);
        let label = if spec.id == CMD_UNDO {
            self.undo_label()
        } else if spec.id == CMD_REDO {
            self.redo_label()
        } else {
            None
        };
        match label.filter(|l| !l.is_empty()) {
            Some(l) => strings::tr_args(
                &ctx,
                &crate::strings::COMMAND_WITH_CHANGE,
                &[("title", &title), ("change", &l)],
            ),
            None => title,
        }
    }

    /// A command's title as a menu, a toolbar or the tray shows it: its
    /// [`command_title`](Self::command_title) without the trailing hint of
    /// its arguments, which the palette keeps
    /// ([`strings::without_argument_hint`]).
    pub fn command_menu_title(&self, spec: &CommandSpec) -> String {
        strings::without_argument_hint(&self.command_title(spec)).to_string()
    }

    /// A settings section's title as shown: the owner's plugin title
    /// translated (`plugin-<id>.title`), or the section as a hook named
    /// it.
    pub(crate) fn section_title(&self, ctx: &Context, owner: &OwnerId, section: &str) -> String {
        let derived = self.0.derived.borrow();
        match derived.sections.get(owner) {
            Some(title) if title == section => strings::plugin_title(ctx, owner.as_str(), section),
            _ => section.to_string(),
        }
    }

    /// A panel's title as shown (its id when it is not declared).
    pub(crate) fn panel_title(&self, ctx: &Context, id: &PanelId) -> String {
        let title = self.workbench_mut().spec(id).map(|s| s.title.clone());
        match title {
            Some(t) => strings::panel_title(ctx, id.as_str(), &t),
            None => id.to_string(),
        }
    }

    /// Undoes through the [`STEP_UNDO`] step: its hooks, then (unless a
    /// run-hook replaces it) the host's undo; the journal says how it
    /// went. What `edit.undo` runs.
    pub fn undo(&self, origin: &Origin) -> Outcome {
        self.history_step(false, origin)
    }

    /// Redoes through the [`STEP_REDO`] step.
    pub fn redo(&self, origin: &Origin) -> Outcome {
        self.history_step(true, origin)
    }

    fn history_step(&self, redo: bool, origin: &Origin) -> Outcome {
        let (step, key, done, failed) = if redo {
            (&STEP_REDO, "redone", "edit.redone", "edit.redo_failed")
        } else {
            (&STEP_UNDO, "undone", "edit.undone", "edit.undo_failed")
        };
        let label = if redo {
            self.redo_label()
        } else {
            self.undo_label()
        };
        let args = json!({ "label": label, "origin": source_of(origin) });
        let who = origin.clone();
        let outcome = self.step(step, args, move |app, _| {
            let mut host = app.0.host.borrow_mut();
            let h = host.as_mut().ok_or("the host is dispatching a command")?;
            let r = if redo { h.redo(&who) } else { h.undo(&who) }?;
            Ok(json!({ key: r }))
        });
        let source = source_of(origin);
        match &outcome {
            Outcome::Done(Ok(v)) => {
                if !v[key].is_null() {
                    self.publish(&source, None, done, json!({ "label": v[key] }));
                }
            }
            Outcome::Done(Err(e)) => {
                self.publish(&source, None, failed, json!({ "label": label, "error": e }));
            }
            Outcome::Vetoed { by, reason } => self.publish(
                &source,
                None,
                failed,
                json!({ "label": label, "vetoed_by": by, "reason": reason }),
            ),
        }
        outcome
    }

    /// Every overlay on screen, bottom first, with where it was drawn
    /// last: dialogs, modals, popovers and menus of the overlay stack,
    /// the toast stack, then egui's popups. What an application keeps
    /// its own hit-testing and pictures right with, without egui-pop's
    /// internal ids.
    pub fn overlay_rects(&self) -> Vec<OverlayRect> {
        let ctx = self.egui();
        let dialog = self
            .service(&crate::dialogs::DIALOGS)
            .and_then(|d| d.current())
            .map(|(id, _)| id);
        let mut known: Vec<Id> = Vec::new();
        let mut out: Vec<OverlayRect> = overlay::rects(&ctx)
            .into_iter()
            .map(|(spec, rect)| {
                known.push(spec.id);
                let (kind, id) = match dialog {
                    Some(n) if spec.id == crate::dialogs::overlay_id(n) => {
                        (OverlayKind::Dialog, format!("dialog:{n}"))
                    }
                    _ => (
                        match spec.kind {
                            overlay::Kind::Modal => OverlayKind::Modal,
                            overlay::Kind::Popover => OverlayKind::Popover,
                            overlay::Kind::Menu => OverlayKind::Menu,
                        },
                        spec.label.clone(),
                    ),
                };
                OverlayRect { kind, id, rect }
            })
            .collect();
        if let Some(hint) = self.drop_hint() {
            out.push(OverlayRect {
                kind: OverlayKind::DropHint,
                id: hint.name(),
                rect: hint.rect,
            });
        }
        let toasts = crate::toasts::area_id();
        known.push(toasts);
        if self
            .service(&crate::toasts::TOASTS)
            .is_some_and(|t| !t.shown().is_empty())
            && let Some(rect) = ctx.memory(|m| m.area_rect(toasts))
        {
            out.push(OverlayRect {
                kind: OverlayKind::Toasts,
                id: "toasts".into(),
                rect,
            });
        }
        let mut popups: Vec<(Id, egui::Rect)> = ctx.memory(|m| {
            m.areas()
                .visible_layer_ids()
                .into_iter()
                .filter(|l| l.order == egui::Order::Foreground && !known.contains(&l.id))
                .filter_map(|l| m.area_rect(l.id).map(|r| (l.id, r)))
                .collect()
        });
        popups.sort_by_key(|a| a.0.value());
        let menu = self.popup().map(|p| p.menu);
        out.extend(popups.into_iter().map(|(id, rect)| match &menu {
            Some(m) if id == crate::menus::popup_id() => OverlayRect {
                kind: OverlayKind::Menu,
                id: m.to_string(),
                rect,
            },
            _ => OverlayRect {
                kind: OverlayKind::Popup,
                id: format!("popup:{}", id.short_debug_format()),
                rect,
            },
        }));
        out
    }

    /// The journal's last sequence number, as the host says.
    pub fn last_event(&self) -> Seq {
        self.0.host.borrow().as_ref().map_or(0, |h| {
            h.last_seq()
                .unwrap_or_else(|| self.0.journal.borrow().last())
        })
    }

    pub(crate) fn add_collector(&self, owner: &OwnerId, c: FnCollector) -> Result<(), CtxError> {
        let name = egui_pop_core::debug::Collector::name(&c).to_string();
        self.0
            .bridge
            .borrow_mut()
            .register(c)
            .map_err(CtxError::Collector)?;
        self.0.collectors.borrow_mut().push((owner.clone(), name));
        Ok(())
    }

    /// Runs a step (ADR 0011): its before-hooks (veto, redirect), its
    /// run-hook or `default`, its after-hooks; publishes the step on the
    /// journal and keeps a trace. Hooks run without any registry
    /// borrowed: they may run other steps or ask for commands.
    pub fn step(
        &self,
        step: &StepId,
        args: Json,
        default: impl FnOnce(&AppHandle, &Json) -> Result<Json, String>,
    ) -> Outcome {
        let hooks: Vec<(OwnerId, HookFn)> = self
            .0
            .step_hooks
            .borrow()
            .iter()
            .filter(|s| s.item.step == *step)
            .map(|s| (s.owner.clone(), s.item.f.clone()))
            .collect();
        let declared = self.0.steps.borrow().contains(step);
        let (outcome, trace) = crate::steps::run(self, step, args, hooks, declared, default);
        let mut traces = self.0.step_traces.borrow_mut();
        traces.push_back(trace);
        while traces.len() > crate::steps::TRACES {
            traces.pop_front();
        }
        outcome
    }

    /// The declared steps, with their hooks (owner, kind), in
    /// declaration order.
    pub fn steps(&self) -> Vec<StepInfo> {
        let hooks = self.0.step_hooks.borrow();
        self.0
            .steps
            .borrow()
            .in_order()
            .into_iter()
            .map(|(id, slot)| StepInfo {
                spec: slot.item.clone(),
                owner: slot.owner.clone(),
                hooks: hooks
                    .iter()
                    .filter(|h| h.item.step == *id)
                    .map(|h| HookInfo {
                        owner: h.owner.clone(),
                        kind: h.item.f.kind(),
                    })
                    .collect(),
            })
            .collect()
    }

    /// The last runs of steps, oldest first.
    pub fn step_traces(&self) -> Vec<StepTrace> {
        self.0.step_traces.borrow().iter().cloned().collect()
    }

    pub(crate) fn declare_step(&self, owner: &OwnerId, spec: StepSpec) -> Result<(), CtxError> {
        let id = spec.id.clone();
        self.0
            .steps
            .borrow_mut()
            .insert(owner, id, spec)
            .map_err(|t| CtxError::Step {
                id: t.id.to_string(),
                by: t.by,
            })
    }

    pub(crate) fn add_step_hook(&self, owner: &OwnerId, step: StepId, f: HookFn) {
        self.0
            .step_hooks
            .borrow_mut()
            .push(owner, StepHook { step, f });
    }

    pub(crate) fn add_list_provider(&self, owner: &OwnerId, provider: crate::menus::Provider) {
        self.0.lists.borrow_mut().push(owner, provider);
    }

    /// The entries of a list (`MenuItem::list`), asked of its provider (the
    /// last registered for the name) in a context; empty without one.
    /// Menus ask when they open; what it gave is kept for the `lists`
    /// collector.
    pub fn list_items(&self, name: &str, cx: &WhenContext) -> Vec<crate::menus::ListItem> {
        let provider = self
            .0
            .lists
            .borrow()
            .iter()
            .filter(|s| s.item.name == name)
            .last()
            .map(|s| (s.owner.clone(), s.item.f.clone()));
        let Some((owner, f)) = provider else {
            return Vec::new();
        };
        let items = f(self, cx);
        self.0.list_traces.borrow_mut().insert(
            name.to_string(),
            crate::menus::ListTrace {
                name: name.to_string(),
                owner,
                items: items.clone(),
            },
        );
        items
    }

    /// The list providers (name, owner) and what each gave last.
    pub fn lists(&self) -> Json {
        let traces = self.0.list_traces.borrow();
        json!(
            self.0
                .lists
                .borrow()
                .iter()
                .map(|s| json!({
                    "name": s.item.name,
                    "owner": s.owner,
                    "last": traces.get(&s.item.name).map(|t| &t.items),
                }))
                .collect::<Vec<_>>()
        )
    }

    pub(crate) fn popup(&self) -> Option<crate::menus::Popup> {
        self.0.popup.borrow().clone()
    }

    pub(crate) fn set_popup(&self, popup: Option<crate::menus::Popup>) {
        *self.0.popup.borrow_mut() = popup;
    }

    /// Whether a run of this command waits for its result.
    pub fn running(&self, id: &CommandId) -> bool {
        self.0.running.borrow().get(id).is_some_and(|n| *n > 0)
    }

    /// When the command's waiting runs started, by the motion clock
    /// (`egui_pop_core::motion::now`): what a busy indicator's delay
    /// counts from. `None` when no run waits.
    pub fn running_since(&self, id: &CommandId) -> Option<f64> {
        self.0.running_since.borrow().get(id).copied()
    }

    fn publish_now(&self, source: &str, cause: Option<Seq>, name: &str, data: Json) -> Seq {
        let mut host = self.0.host.borrow_mut();
        let h = host.as_mut().expect("the host is free outside a dispatch");
        for (s, c, n, d) in self.0.held.borrow_mut().drain(..) {
            self.publish_to(h.as_mut(), &s, c, &n, d);
        }
        self.publish_to(h.as_mut(), source, cause, name, data)
    }

    /// A service, typed.
    pub fn service<T: Send + Sync + 'static>(&self, token: &ServiceToken<T>) -> Option<Arc<T>> {
        self.0.services.borrow().get(token)
    }

    /// The theme service.
    pub fn theme(&self) -> Arc<ThemeService> {
        self.service(&THEME)
            .expect("the framework provides the theme")
    }

    /// Asks for a command, run by the user before the next frame.
    pub fn run(&self, command: CommandId, args: Json) {
        self.run_as(command, args, Origin::User);
    }

    /// Asks for a command with an origin.
    pub fn run_as(&self, command: CommandId, args: Json, origin: Origin) {
        self.push_request(Request::Run {
            command,
            args,
            origin,
        });
    }

    /// Asks for a plugin to be connected or disconnected before the next
    /// frame.
    pub fn set_plugin_enabled(&self, plugin: PluginId, enabled: bool) {
        self.push_request(Request::Enable(plugin, enabled));
    }

    pub(crate) fn push_request(&self, r: Request) {
        self.0.requests.borrow_mut().push(r);
        self.egui().request_repaint();
    }

    /// Sets an application-wide context key (depth 0), for `when`
    /// clauses. The context stack's own keys (`place`, `placeDepth`,
    /// `modePushed`, `mode.*`) are its projection's: a plugin setting one
    /// by hand is reported (`contexts::hand_writes`, a diagnostic) while
    /// the stack is connected.
    pub fn set_context(&self, key: &str, value: &str) {
        if !self.0.stack_writing.get() && crate::contexts::is_stack_key(key) {
            crate::contexts::written_by_hand(self, key, value);
        }
        self.0.context.borrow_mut().set(key, value);
    }

    /// Sets a context key as the context stack's projection.
    pub(crate) fn set_stack_context(&self, key: &str, value: &str) {
        self.0.stack_writing.set(true);
        self.0.context.borrow_mut().set(key, value);
        self.0.stack_writing.set(false);
    }

    /// The command running now (the innermost), with its owner.
    pub(crate) fn running_command(&self) -> Option<(CommandId, Option<OwnerId>)> {
        self.0.running_commands.borrow().last().cloned()
    }

    /// Records a warning among the diagnostics (logged).
    pub(crate) fn warn(
        &self,
        code: &'static str,
        owner: Option<&OwnerId>,
        subject: &str,
        message: String,
    ) {
        self.0
            .diagnostics
            .borrow_mut()
            .warn(code, owner, subject, message);
    }

    /// The application-wide context keys.
    pub fn context(&self) -> WhenContext {
        self.0.context.borrow().clone()
    }

    /// The context keys at this moment: application-wide, then the focus
    /// chain's, the overlays'.
    pub fn when_context(&self) -> WhenContext {
        let mut base = self.context();
        let ctx = &self.egui();
        let flag = |on: bool| if on { "true" } else { "false" };
        base.set("overlayOpen", flag(!overlay::stack(ctx).is_empty()));
        base.set("modalOpen", flag(overlay::modal_open(ctx)));
        base.set("canUndo", flag(self.can_undo()));
        base.set("canRedo", flag(self.can_redo()));
        focus::when_context(ctx, &base)
    }

    /// The diagnostics collected (registration, zones, hooks), then the
    /// store's (writes from plugins that are not a path's writers).
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        let mut all = self.0.diagnostics.borrow().all().to_vec();
        all.extend_from_slice(lock(&self.0.store).diagnostics().all());
        all
    }

    /// A menu's items after the hooks.
    pub fn menu(&self, menu: &MenuId) -> Vec<MenuItem> {
        self.0
            .derived
            .borrow()
            .menus
            .get(menu)
            .cloned()
            .unwrap_or_default()
    }

    /// The palette's entries after the hooks.
    pub fn palette(&self) -> Vec<PaletteEntry> {
        self.0.derived.borrow().palette.clone()
    }

    /// The settings form's entries after the hooks.
    pub fn settings(&self) -> Vec<SettingEntry> {
        self.0.derived.borrow().settings.clone()
    }

    /// A command's declaration.
    pub fn command(&self, id: &CommandId) -> Option<CommandSpec> {
        self.0
            .derived
            .borrow()
            .commands
            .iter()
            .find(|(s, _)| s.id == *id)
            .map(|(s, _)| s.clone())
    }

    /// Asks for a command's keys to be the user's own (`None`: no key),
    /// saved in the user's keymap; applied before the next frame.
    pub fn rebind(&self, command: CommandId, keys: Option<String>) {
        self.rebind_in(Layer::User, command, keys);
    }

    /// Asks for a command's keys in `layer`: the user's (saved in the
    /// user's keymap) or the workspace's (kept by the application, which
    /// reads it back with [`AppHandle::export_keymap`]); applied before
    /// the next frame, through the `keys.rebind` step. Another layer is
    /// refused there.
    pub fn rebind_in(&self, layer: Layer, command: CommandId, keys: Option<String>) {
        self.push_request(Request::Rebind {
            command,
            keys,
            layer,
        });
    }

    /// Asks for a command's default keys back.
    pub fn reset_keys(&self, command: CommandId) {
        self.reset_keys_in(Layer::User, command);
    }

    /// Takes back what `layer` (the user's or the workspace's) changed
    /// of a command's keys, before the next frame.
    pub fn reset_keys_in(&self, layer: Layer, command: CommandId) {
        self.push_request(Request::ResetKeys { command, layer });
    }

    /// Whether `layer` (the user's or the workspace's) changed a
    /// command's keys.
    pub fn rebound_in(&self, layer: Layer, id: &CommandId) -> bool {
        let d = self.0.derived.borrow();
        match layer {
            Layer::User => d.rebound.contains(id),
            Layer::Workspace => d.rebound_workspace.contains(id),
            _ => false,
        }
    }

    /// A command's live bindings, each with the layer it comes from.
    pub fn key_sources(&self, id: &CommandId) -> Vec<Binding> {
        self.0
            .derived
            .borrow()
            .sources
            .get(id)
            .cloned()
            .unwrap_or_default()
    }

    /// Whether the shortcuts editor offers to change keys for the
    /// workspace as well as for the user (off by default: an application
    /// with workspaces turns it on, and keeps the workspace's layer).
    pub fn offer_workspace_keys(&self, on: bool) {
        self.0.workspace_keys.set(on);
    }

    /// Whether the shortcuts editor offers the workspace's keys.
    pub fn workspace_keys_offered(&self) -> bool {
        self.0.workspace_keys.get()
    }

    /// How the shortcuts editor names where a binding comes from: `f`
    /// gives a name, in the interface's language, or `None` for the
    /// default one ([`AppHandle::key_source_name`]).
    pub fn name_key_sources(&self, f: impl Fn(&Binding) -> Option<String> + 'static) {
        *self.0.key_source_names.borrow_mut() = Some(Rc::new(f));
    }

    /// Where a binding comes from, as the shortcuts editor says it after
    /// its keys: the host's name for it, else `workspace` or `user`;
    /// nothing for the application's defaults.
    pub fn key_source_name(&self, ctx: &Context, binding: &Binding) -> Option<String> {
        let named = self.0.key_source_names.borrow().clone();
        if let Some(name) = named.and_then(|f| f(binding)) {
            return Some(name);
        }
        let key = match binding.layer {
            Layer::Suggestion | Layer::Default => return None,
            Layer::Workspace => &crate::strings::SHORTCUTS_FROM_WORKSPACE,
            Layer::User => &crate::strings::SHORTCUTS_FROM_USER,
        };
        Some(strings::tr(ctx, key))
    }

    /// While on, key presses reach the interface instead of the key
    /// dispatcher (a shortcuts editor capturing new keys).
    pub fn capture_keys(&self, on: bool) {
        self.0.capturing.set(on);
    }

    /// [`AppHandle::key_conflicts`] by scope (`global`, `surface:<name>`).
    pub fn key_conflicts_by_scope(&self) -> BTreeMap<String, Vec<(Binding, Binding)>> {
        let mut out: BTreeMap<String, Vec<(Binding, Binding)>> = BTreeMap::new();
        for (lose, win) in self.key_conflicts() {
            out.entry(lose.options.scope.to_string())
                .or_default()
                .push((lose, win));
        }
        out
    }

    /// Declares a command after build, owned by `owner` (a host's own
    /// registry changing at run time), with the rules and traces of
    /// `Ctx::declare`: its keys, menus and palette entry follow before the
    /// next frame, with no plugin reconnected. A refusal is logged and
    /// published (`command.declare_failed`).
    pub fn declare(&self, owner: &OwnerId, spec: CommandSpec) {
        self.push_request(Request::Declare {
            owner: owner.clone(),
            spec,
        });
    }

    /// Removes a command declared after build, with its keys and menu
    /// items, before the next frame.
    pub fn undeclare(&self, id: &CommandId) {
        self.push_request(Request::Undeclare(id.clone()));
    }

    /// A layer's bindings, whoever declared them, as a keymap document
    /// (scopes, `when` clauses, every key): a profile to save.
    pub fn export_keymap(&self, layer: Layer) -> egui_pop_model::keymap::KeymapDoc {
        self.0
            .derived
            .borrow()
            .layers
            .get(&layer)
            .cloned()
            .unwrap_or_default()
    }

    /// Loads a keymap document into the user's (or the workspace's)
    /// layer, replacing what was loaded there, before the next frame
    /// (the `keys.load` step). Returns what it will change against the
    /// layer as it stands; conflicts are published once applied
    /// (`keys.conflict`). `App::load_keymap` does it at once.
    pub fn load_keymap(
        &self,
        doc: egui_pop_model::keymap::KeymapDoc,
        layer: Layer,
    ) -> egui_pop_model::keymap::KeymapDiff {
        let diff = egui_pop_model::keymap::KeymapDiff::between(&self.export_keymap(layer), &doc);
        self.push_request(Request::LoadKeymap { doc, layer });
        diff
    }

    /// Bindings losing to another on the same keys (loser, winner): what
    /// a shortcuts editor shows as conflicts.
    pub fn key_conflicts(&self) -> Vec<(Binding, Binding)> {
        self.0.derived.borrow().conflicts.clone()
    }

    /// Whether the user changed a command's keys.
    pub fn rebound(&self, id: &CommandId) -> bool {
        self.0.derived.borrow().rebound.contains(id)
    }

    /// Whether a command applies in a context (its `when` clause holds).
    pub fn applies(&self, id: &CommandId, cx: &WhenContext) -> bool {
        self.0
            .derived
            .borrow()
            .commands
            .iter()
            .find(|(s, _)| s.id == *id)
            .is_some_and(|(s, _)| {
                s.when.is_empty()
                    || egui_pop_model::when::When::parse(&s.when).is_ok_and(|w| w.eval(cx))
            })
    }

    /// A command's live key bindings, as shown (`Ctrl+K Ctrl+S`).
    pub fn keys_for(&self, id: &CommandId) -> Vec<String> {
        self.0
            .derived
            .borrow()
            .keys
            .get(id)
            .cloned()
            .unwrap_or_default()
    }

    /// The live key bindings of the system scope (`BindOptions::system()`):
    /// what a plugin registers with the desktop as global shortcuts,
    /// current after each rebuild (`On::Rebuilt`).
    pub fn system_bindings(&self) -> Vec<Binding> {
        self.0.derived.borrow().system.clone()
    }

    /// Every command (spec and owner).
    pub fn commands(&self) -> Vec<(CommandSpec, OwnerId)> {
        self.0.derived.borrow().commands.clone()
    }

    /// The last command runs, oldest first.
    pub fn history(&self) -> Vec<Run> {
        self.0.history.borrow().clone()
    }

    /// The plugins, in connection order.
    pub fn plugins(&self) -> Vec<PluginInfo> {
        self.0.plugins.borrow().clone()
    }

    /// The zones with their resolved blocks.
    pub fn zones(&self) -> Vec<ZoneInfo> {
        self.0.zones.borrow().infos()
    }

    /// The services: id, owner, type.
    pub fn services(&self) -> Vec<(String, OwnerId, &'static str)> {
        self.0
            .services
            .borrow()
            .list()
            .into_iter()
            .map(|(i, o, t)| (i.to_string(), o, t))
            .collect()
    }

    pub(crate) fn zones_mut(&self) -> RefMut<'_, ZoneTable> {
        self.0.zones.borrow_mut()
    }

    pub(crate) fn services_mut(&self) -> RefMut<'_, Services> {
        self.0.services.borrow_mut()
    }

    pub(crate) fn add_panel_view(&self, owner: &OwnerId, id: PanelId, draw: DrawFn) {
        self.0.panels.borrow_mut().insert(
            id,
            PanelView {
                owner: owner.clone(),
                draw: Some(draw),
            },
        );
    }

    pub(crate) fn add_story(&self, owner: &OwnerId, story: Story) -> Result<(), CtxError> {
        let id = story.id.clone();
        self.0
            .stories
            .borrow_mut()
            .insert(owner, id, StoryEntry { story })
            .map_err(|t| CtxError::Story {
                id: t.id.to_string(),
                by: t.by,
            })
    }

    /// Whether a zone has blocks to draw.
    pub fn zone_has_blocks(&self, zone: &ZoneId) -> bool {
        self.0
            .zones
            .borrow()
            .resolved
            .get(zone)
            .is_some_and(|b| !b.is_empty())
    }

    /// Draws a zone's blocks, each in a fault boundary. A zone nobody
    /// declared draws nothing (and was reported when zones were
    /// resolved).
    pub fn show_zone(&self, ui: &mut Ui, zone: &ZoneId) {
        let plan: Vec<(usize, Vec<usize>, String)> = match self.0.zones.borrow().resolved.get(zone)
        {
            Some(blocks) => blocks
                .iter()
                .map(|r| (r.draw, r.wraps.clone(), r.id.to_string()))
                .collect(),
            None => return,
        };
        for (draw, wraps, id) in plan {
            let taken = {
                let mut z = self.0.zones.borrow_mut();
                let base = z.contributions.get_mut(draw).and_then(|c| c.block.take());
                let ws: Vec<(usize, Option<Block>)> = wraps
                    .iter()
                    .map(|&w| (w, z.contributions.get_mut(w).and_then(|c| c.block.take())))
                    .collect();
                (base, ws)
            };
            let (Some(mut base), ws) = taken else {
                continue;
            };
            let mut wrap_blocks: Vec<(usize, Block)> = ws
                .into_iter()
                .filter_map(|(i, b)| b.map(|b| (i, b)))
                .collect();
            ui.push_id(("egui_pop_block", &id), |ui| {
                fault::boundary(ui, "block", &id, |ui| {
                    let draw_fn: &mut DrawFn = match &mut base {
                        Block::Add { draw, .. }
                        | Block::Replace { draw, .. }
                        | Block::Extend { draw, .. } => draw,
                        Block::Wrap { .. } => return,
                    };
                    let mut wrap_fns: Vec<zones::WrapFn> = Vec::new();
                    // Move the wrappers out of their blocks for the call,
                    // then put them back.
                    for (_, b) in &mut wrap_blocks {
                        if let Block::Wrap { wrap, .. } = b {
                            wrap_fns.push(std::mem::replace(wrap, Box::new(|_, _, _| {})));
                        }
                    }
                    draw_chain(ui, self, draw_fn, &mut wrap_fns);
                    let mut back = wrap_fns.into_iter();
                    for (_, b) in &mut wrap_blocks {
                        if let Block::Wrap { wrap, .. } = b
                            && let Some(f) = back.next()
                        {
                            *wrap = f;
                        }
                    }
                });
            });
            let mut z = self.0.zones.borrow_mut();
            if let Some(c) = z.contributions.get_mut(draw) {
                c.block = Some(base);
            }
            for (i, b) in wrap_blocks {
                if let Some(c) = z.contributions.get_mut(i) {
                    c.block = Some(b);
                }
            }
        }
    }

    /// The stories, by group then weight.
    pub fn stories(&self) -> Vec<StoryInfo> {
        let mut v: Vec<StoryInfo> = self
            .0
            .stories
            .borrow()
            .iter()
            .map(|(_, s)| StoryInfo {
                id: s.item.story.id.clone(),
                title: s.item.story.title.clone(),
                description: s.item.story.description.clone(),
                group: s.item.story.group.clone(),
                weight: s.item.story.weight,
                has_playground: s.item.story.playground.is_some(),
                owner: s.owner.clone(),
            })
            .collect();
        v.sort_by(|a, b| (&a.group, a.weight, &a.title).cmp(&(&b.group, b.weight, &b.title)));
        v
    }

    fn with_story(&self, id: &StoryId, playground: bool, ui: &mut Ui) -> bool {
        let taken = {
            let mut s = self.0.stories.borrow_mut();
            s.get_mut(id).and_then(|e| {
                if playground {
                    e.story.playground.take()
                } else {
                    Some(std::mem::replace(&mut e.story.show, Box::new(|_, _| {})))
                }
            })
        };
        let Some(mut f) = taken else {
            return false;
        };
        let title = id.to_string();
        ui.push_id(("egui_pop_story", id.as_str(), playground), |ui| {
            fault::boundary(ui, "story", &title, |ui| f(ui, self));
        });
        if let Some(e) = self.0.stories.borrow_mut().get_mut(id) {
            if playground {
                e.story.playground = Some(f);
            } else {
                e.story.show = f;
            }
        }
        true
    }

    /// Draws a story; `false` when there is no such story.
    pub fn show_story(&self, ui: &mut Ui, id: &StoryId) -> bool {
        self.with_story(id, false, ui)
    }

    /// Draws a story's playground; `false` when it has none.
    pub fn show_playground(&self, ui: &mut Ui, id: &StoryId) -> bool {
        self.with_story(id, true, ui)
    }

    /// Draws a panel's content, in its focus scope and fault boundary.
    pub(crate) fn show_panel(&self, ui: &mut Ui, id: &PanelId) {
        let title = self.panel_title(ui.ctx(), id);
        let title = title.as_str();
        let taken = self
            .0
            .panels
            .borrow_mut()
            .get_mut(id)
            .and_then(|p| p.draw.take());
        let Some(mut draw) = taken else {
            return;
        };
        let scope = scope_name(id.as_str());
        ui.push_id(("egui_pop_panel", id.as_str()), |ui| {
            focus::scope(ui, &scope, &[], |ui| {
                fault::boundary(ui, "panel", title, |ui| draw(ui, self));
            });
        });
        if let Some(p) = self.0.panels.borrow_mut().get_mut(id) {
            p.draw = Some(draw);
        }
    }

    pub(crate) fn workbench_mut(&self) -> RefMut<'_, crate::workbench::Workbench> {
        self.0.workbench.borrow_mut()
    }

    /// Whether the centre is drawn edge to edge.
    pub fn edge_to_edge(&self) -> bool {
        self.0.edge_to_edge.get()
    }

    /// Whether the main menu is drawn as a bar under the title
    /// (`AppBuilder::menu_bar`).
    pub fn menu_bar(&self) -> bool {
        self.0.menu_bar.get()
    }

    pub(crate) fn set_drawing_areas(&self, on: bool) {
        self.0.drawing_areas.set(on);
    }

    /// Whether the workbench's areas are being drawn (their layout must
    /// not change under them).
    pub(crate) fn drawing_areas(&self) -> bool {
        self.0.drawing_areas.get()
    }

    /// Whether a panel is shown (docked or detached, not closed).
    pub fn panel_visible(&self, id: &PanelId) -> bool {
        self.workbench_mut().shows(id)
    }

    /// Shows or closes a panel (the `workbench.show` and `workbench.hide`
    /// steps; a palette opens through `palette.open`).
    pub fn set_panel_visible(&self, id: &PanelId, visible: bool) {
        let palette = self.workbench_mut().spec(id).is_some_and(|s| s.palette);
        let change = if visible && palette {
            // A palette opens through its own step, floating.
            crate::workbench::Change::OpenPalette(id.clone())
        } else if visible {
            crate::workbench::Change::Show(id.clone())
        } else {
            crate::workbench::Change::Hide(id.clone())
        };
        self.change_workbench(change);
    }

    /// The workbench's layout as it stands: areas, tabs and splits,
    /// detached panels, closed panels.
    pub fn layout(&self) -> egui_pop_model::layout::Layout {
        self.workbench_mut().layout()
    }

    /// Puts in a layout (loaded, or an arrangement), repaired against
    /// today's panels and screens; says what the repair changed.
    pub fn set_layout(&self, layout: egui_pop_model::layout::Layout) -> Vec<String> {
        let notes = self.workbench_mut().set_layout(layout, &SCREENS);
        self.egui().request_repaint();
        notes
    }

    /// The declared panels.
    pub fn panels(&self) -> Vec<egui_pop_model::panels::PanelSpec> {
        self.workbench_mut().specs.clone()
    }

    /// Every declared panel with the plugin that declared it (its
    /// [`OwnerId`]), in declaration order: the application's panels and
    /// those of the plugins it connected, egui-pop's among them (the
    /// jobs centre, `egui_pop.jobs`; the devtools). A host that checks
    /// layouts or templates against its own panels accepts these too, so
    /// a template may place a plugin's panel. Unlike [`Self::panels`], a
    /// zone shown as a panel is listed even while it has no blocks.
    pub fn declared_panels(&self) -> Vec<(egui_pop_model::panels::PanelSpec, OwnerId)> {
        self.0.derived.borrow().panels.clone()
    }

    /// The drop hint on screen, while a tab is dragged over another area.
    pub fn drop_hint(&self) -> Option<crate::workbench::DropHint> {
        self.workbench_mut().hint.clone()
    }

    /// The panel that last had the pointer pressed in it, or was focused.
    pub fn focused_panel(&self) -> Option<PanelId> {
        self.workbench_mut().focused.clone()
    }

    /// Changes the workbench through its step (ADR 0011): `None` when
    /// asked while the areas are drawn (it is done at the frame's end).
    pub fn change_workbench(&self, change: crate::workbench::Change) -> Option<Outcome> {
        if self.0.drawing_areas.get() {
            self.push_request(Request::Workbench(change));
            return None;
        }
        let (step, args) = change.step();
        let id = step.clone();
        let out = self.step(&step, args, move |app, args| {
            let c = crate::workbench::Change::from_args(&id, args)?;
            app.workbench_mut().apply(&c)
        });
        // A palette's change, once done, is news.
        if let (Some(event), Outcome::Done(Ok(result))) = (change.event(), &out) {
            self.publish("egui_pop", None, event, result.clone());
        }
        if matches!(change, crate::workbench::Change::Zen(_)) {
            self.follow_zen();
        }
        self.egui().request_repaint();
        Some(out)
    }

    /// Starts zen mode (only the centre is shown) or ends it
    /// (`workbench.zen`).
    pub fn set_zen(&self, on: bool) -> Option<Outcome> {
        self.change_workbench(crate::workbench::Change::Zen(on))
    }

    /// Whether zen mode is on.
    pub fn zen(&self) -> bool {
        self.workbench_mut().zen
    }

    /// Keeps the `workbenchZen` context key in line with zen mode.
    pub(crate) fn follow_zen(&self) {
        let on = self.zen();
        self.set_context("workbenchZen", if on { "true" } else { "false" });
    }

    /// Where an area was drawn in the last pass (none while it is not
    /// shown, folded included).
    pub fn area_rect(&self, area: Area) -> Option<egui::Rect> {
        self.workbench_mut().rects.get(&area).copied()
    }

    /// Folds an area to its strip, or unfolds it (`workbench.fold`).
    pub fn fold_area(&self, area: Area, folded: bool) -> Option<Outcome> {
        self.change_workbench(crate::workbench::Change::Fold(area, folded))
    }

    /// Lays a side area beside the centre or over it (`workbench.place`;
    /// refused for the centre and the bottom area).
    pub fn place_area(
        &self,
        area: Area,
        placement: egui_pop_model::layout::Placement,
    ) -> Option<Outcome> {
        self.change_workbench(crate::workbench::Change::Place(area, placement))
    }

    /// Where a side area lies now (beside the centre, or over it).
    pub fn area_placement(&self, area: Area) -> egui_pop_model::layout::Placement {
        self.workbench_mut().layout.placement(area)
    }

    /// The indicators and their states, lightest first.
    pub fn indicators(&self) -> Vec<(crate::strips::Indicator, crate::strips::IndicatorState)> {
        let mut v: Vec<_> = self
            .0
            .indicators
            .borrow()
            .iter()
            .map(|s| s.item.clone())
            .collect();
        v.sort_by_key(|(i, _)| i.weight);
        v
    }

    pub(crate) fn add_indicator(&self, owner: &OwnerId, ind: crate::strips::Indicator) {
        let state = ind.initial.clone();
        self.0.indicators.borrow_mut().push(owner, (ind, state));
    }

    /// Feeds the journal's new events to the indicators.
    pub(crate) fn feed_indicators(&self) {
        let seen = self.0.indicators_seen.get();
        let last = self.last_event();
        if last <= seen {
            return;
        }
        let events = self.events_since(seen);
        let mut list = self.0.indicators.borrow_mut();
        let mut items: Vec<_> = list.iter_mut().map(|s| &mut s.item).collect();
        for e in &events {
            for (ind, state) in items.iter_mut().map(|x| (&x.0, &mut x.1)) {
                if e.name.starts_with(&ind.prefix) {
                    (ind.reduce)(state, e);
                    state.seq = e.seq;
                }
            }
        }
        self.0.indicators_seen.set(last);
    }

    /// Moves a panel to an area (`workbench.move`); refused when its
    /// rules keep it out.
    pub fn move_panel(&self, panel: &PanelId, area: Area) -> Option<Outcome> {
        self.change_workbench(crate::workbench::Change::Move(panel.clone(), area))
    }

    /// Moves a panel to a window of its own (`workbench.detach`).
    pub fn detach_panel(&self, panel: &PanelId) -> Option<Outcome> {
        self.change_workbench(crate::workbench::Change::Detach(panel.clone()))
    }

    /// Docks a detached panel back in its area (`workbench.attach`).
    pub fn attach_panel(&self, panel: &PanelId) -> Option<Outcome> {
        self.change_workbench(crate::workbench::Change::Attach(panel.clone()))
    }

    /// Shows a panel and gives it the keyboard (`workbench.focus`).
    pub fn focus_panel(&self, panel: &PanelId) -> Option<Outcome> {
        self.change_workbench(crate::workbench::Change::Focus(panel.clone()))
    }

    /// A change the workbench refused: on the journal, and in a toast when
    /// toasts are there.
    pub(crate) fn workbench_refused(&self, what: &str, why: &str) {
        self.publish(
            "egui_pop",
            None,
            "workbench.refused",
            json!({ "change": what, "reason": why }),
        );
        if let Some(t) = self.service(&crate::toasts::TOASTS) {
            t.notify(crate::toasts::Toast::warning(why));
        }
    }
}

/// A step's outcome as a command's result.
fn outcome_result(outcome: Outcome) -> Result<Json, String> {
    match outcome {
        Outcome::Done(r) => r,
        Outcome::Vetoed { by, reason } => Err(format!("stopped by `{by}`: {reason}")),
    }
}

/// A zen mode command's result: the step's, or the change asked for when
/// it runs after the areas are drawn.
fn zen_result(outcome: Option<Outcome>, on: bool) -> Result<Json, String> {
    match outcome {
        Some(o) => outcome_result(o),
        None => Ok(json!({ "zen": on })),
    }
}

/// A focus scope name from an id: letters, digits, `_` and `.` only.
fn scope_name(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

struct Slot {
    plugin: Box<dyn Plugin>,
    id: PluginId,
    owner: OwnerId,
    connected: bool,
    reason: Option<String>,
}

/// Builds an application.
pub struct AppBuilder {
    plugins: Vec<Box<dyn Plugin>>,
    theme: ThemeSpec,
    themes: Themes,
    persist: Option<Persist>,
    store: Option<SharedStore>,
    host: Option<Box<dyn Host>>,
    system_focus: Option<SystemFocus>,
    edge_to_edge: bool,
    menu_bar: bool,
    shell_sizes: Vec<(PanelId, f32)>,
    folded: Vec<Area>,
    over: Vec<Area>,
    wrap: Option<WrapUi>,
    on_exit: Vec<ExitFn>,
    name: Option<String>,
    panel_alignment: crate::workbench::PanelAlignment,
    kits: Vec<KitSource>,
    user_kits: bool,
    cache: Option<egui_pop_core::tokens::cache::Cache>,
    writer_check: Option<egui_pop_model::values::WriterCheck>,
}

/// Draws around the whole application (`AppBuilder::wrap`): gets the
/// `Ui`, the handle, and the application's drawing to call where it
/// likes.
pub type WrapUi = Box<dyn FnMut(&mut Ui, &AppHandle, &mut dyn FnMut(&mut Ui))>;
/// Runs when the application closes (`AppBuilder::on_exit`), before the
/// `app.exit` step.
pub type ExitFn = Box<dyn FnMut(&AppHandle)>;

/// The application closes: its default writes what is still waiting
/// (the settings, the keymap, the layout and the documents plugins
/// offered, when a directory was given). A plugin replaces it to save
/// elsewhere, or follows it; a veto writes nothing more. Saving does not
/// wait for the exit: what changes is written about a second later
/// (`AppHandle::save_later`).
pub const STEP_EXIT: StepId = StepId::from_static("app.exit");
/// The workbench's layout is loaded (`{ "document" }`, from
/// `Host::layout_document`): at the start, and on `App::load_layout`. A
/// before-hook may redirect the document or veto; a run-hook loads it
/// from elsewhere (and calls `AppHandle::set_layout`).
pub const STEP_LAYOUT_LOAD: StepId = StepId::from_static("layout.load");
/// The workbench's layout is saved (`{ "document" }`): by `App::save`
/// (so at exit) and `App::save_layout`. A before-hook may redirect or
/// veto; a run-hook saves it elsewhere (`AppHandle::layout`).
pub const STEP_LAYOUT_SAVE: StepId = StepId::from_static("layout.save");

impl fmt::Debug for AppBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppBuilder")
            .field(
                "plugins",
                &self.plugins.iter().map(|p| p.id()).collect::<Vec<_>>(),
            )
            .field("theme", &self.theme)
            .finish()
    }
}

impl Default for AppBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl AppBuilder {
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
            theme: ThemeSpec::default(),
            themes: Themes::default(),
            persist: None,
            store: None,
            writer_check: None,
            host: None,
            system_focus: None,
            edge_to_edge: false,
            menu_bar: true,
            shell_sizes: Vec::new(),
            folded: Vec::new(),
            over: Vec::new(),
            wrap: None,
            on_exit: Vec::new(),
            name: None,
            panel_alignment: crate::workbench::PanelAlignment::Justify,
            kits: Vec::new(),
            user_kits: false,
            cache: None,
        }
    }

    /// The bottom panel's alignment by default (the
    /// `workbench.panel_alignment` setting's default; `Justify`, the
    /// whole width, unless given).
    pub fn panel_alignment(mut self, alignment: crate::workbench::PanelAlignment) -> Self {
        self.panel_alignment = alignment;
        self
    }

    /// The application's name ("Helios"), apart from its window's title
    /// ("photo.raw — Helios"): a detached panel's window says it
    /// (`{ $application }`), and the frame shows it when `FramePlugin`
    /// has no name of its own. `run_native` takes its `app_name` when
    /// none is given.
    pub fn name(mut self, name: &str) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Draws the main area edge to edge: no margin around the centre's
    /// panels (an image viewer, a canvas).
    pub fn main_edge_to_edge(mut self, on: bool) -> Self {
        self.edge_to_edge = on;
        self
    }

    /// Draws the main menu as a bar under the title (the default), or
    /// not at all: its commands stay in the palette, under their keys and
    /// in any menu the application opens itself (`menus::popup_at`). The
    /// top bar's zone is then drawn only when it has blocks.
    pub fn menu_bar(mut self, on: bool) -> Self {
        self.menu_bar = on;
        self
    }

    /// The size a shell panel starts at (`workbench::PANEL_SIDE_BAR`,
    /// `PANEL_INSPECTOR`: their width), instead of egui-pop's; its
    /// smallest size follows when it is larger. The user's resizing and a
    /// saved layout still win.
    pub fn shell_panel_size(mut self, panel: PanelId, size: f32) -> Self {
        self.shell_sizes.retain(|(p, _)| *p != panel);
        self.shell_sizes.push((panel, size));
        self
    }

    /// Areas folded to their strip when the application starts with no
    /// saved layout (a saved one, the user's choice, wins).
    pub fn folded(mut self, areas: &[Area]) -> Self {
        self.folded = areas.to_vec();
        self
    }

    /// Side areas lying over the centre when the application starts with
    /// no saved layout (`workbench.place`; a saved one wins): their
    /// folded strip keeps the room, the centre its width. The centre and
    /// the bottom area always lie beside.
    pub fn over(mut self, areas: &[Area]) -> Self {
        self.over = areas
            .iter()
            .copied()
            .filter(|a| matches!(a, Area::StartSide | Area::EndSide))
            .collect();
        self
    }

    /// Draws around the whole application every frame (a custom frame,
    /// a backdrop): `f` gets the `Ui`, the handle and the application's
    /// drawing, to call where it likes.
    pub fn wrap(
        mut self,
        f: impl FnMut(&mut Ui, &AppHandle, &mut dyn FnMut(&mut Ui)) + 'static,
    ) -> Self {
        self.wrap = Some(Box::new(f));
        self
    }

    /// Runs `f` when the application closes, before the `app.exit` step
    /// (which saves).
    pub fn on_exit(mut self, f: impl FnMut(&AppHandle) + 'static) -> Self {
        self.on_exit.push(Box::new(f));
        self
    }

    /// Adds a plugin.
    pub fn plugin(mut self, plugin: impl Plugin) -> Self {
        self.plugins.push(Box::new(plugin));
        self
    }

    /// Adds a group of plugins (`DefaultPlugins`, `MinimalPlugins`).
    pub fn plugins(mut self, group: impl crate::PluginGroup) -> Self {
        self.plugins.extend(group.plugins());
        self
    }

    /// Adds a boxed plugin.
    pub fn boxed(mut self, plugin: Box<dyn Plugin>) -> Self {
        self.plugins.push(plugin);
        self
    }

    /// The theme at start-up.
    pub fn theme(mut self, spec: ThemeSpec) -> Self {
        self.theme = spec;
        self
    }

    /// The themes to pick from (the built-in ones by default).
    pub fn themes(mut self, themes: Themes) -> Self {
        self.themes = themes;
        self
    }

    /// Adds a kit (`docs/kit-packages.md`): its theme joins the themes to
    /// pick from, under the kit's name, and its faces and handlers are
    /// registered when the application is built. A folder read at run
    /// time, a `kit!` or a kit crate's `kit()`:
    ///
    /// ```
    /// use egui_pop_api::App;
    /// use egui_pop_core::kit::Kit;
    /// use egui_pop_core::theme::ThemeSpec;
    /// use egui_pop_core::tokens::kit::KitData;
    ///
    /// let kit = KitData::from_text(
    ///     "underline",
    ///     r#"
    ///     tab:
    ///       shape: {$type: "string", $value: "underline"}
    ///     "#,
    /// );
    /// let ctx = egui::Context::default();
    /// let app = App::builder()
    ///     .kit(kit)
    ///     .theme(ThemeSpec::new("underline"))
    ///     .build(&ctx)
    ///     .unwrap();
    /// assert_eq!(Kit::of(&ctx).tab.shape, "underline");
    /// assert_eq!(app.handle().theme().kits()[0].name, "underline");
    /// ```
    ///
    /// What is wrong with a kit is logged and in the diagnostics
    /// (`kit-problem`); the kit is added anyway, with fallbacks.
    pub fn kit(mut self, kit: impl Into<KitSource>) -> Self {
        self.kits.push(kit.into());
        self
    }

    /// Adds the user's kits too: the data-only kit folders under
    /// `<persist directory>/kits` (`egui_pop_tokens::kit::discover`), read
    /// when the application is built, after the application's own (a
    /// user kit of the same name replaces it), their YAML through the
    /// cache (`AppBuilder::cache`). Without a persistence directory there
    /// are none.
    pub fn user_kits(mut self, on: bool) -> Self {
        self.user_kits = on;
        self
    }

    /// Where the files read at run time (the user's kits) are kept
    /// compiled (`egui_pop_tokens::cache`): `Cache::for_app(id)` for the
    /// platform's cache directory under the application's id,
    /// `Cache::at(dir)` elsewhere, `Cache::off()` for none. Not given,
    /// it is the process's default (`cache::set_default`), off until the
    /// application sets one.
    pub fn cache(mut self, cache: egui_pop_core::tokens::cache::Cache) -> Self {
        self.cache = Some(cache);
        self
    }

    /// Where settings and the user's keymap are saved.
    pub fn persist(mut self, dir: impl Into<std::path::PathBuf>) -> Self {
        self.persist = Some(Persist::new(dir));
        self
    }

    /// The application's kernel: runs the commands, keeps the journal
    /// and the store (`LocalHost` by default, see `host`).
    pub fn host(mut self, host: impl Host + 'static) -> Self {
        self.host = Some(Box::new(host));
        self
    }

    /// Shares an existing store instead of a new one (with the default
    /// host; a host given to `host` brings its own).
    pub fn store(mut self, store: SharedStore) -> Self {
        self.store = Some(store);
        self
    }

    /// What a write from a plugin that is not one of a path's writers
    /// gets: refused (the default) or applied with a warning, to migrate
    /// plugins written before the check
    /// (`egui_pop_model::values::WriterCheck`). Either way it is in the
    /// diagnostics.
    pub fn writer_check(mut self, check: egui_pop_model::values::WriterCheck) -> Self {
        self.writer_check = Some(check);
        self
    }

    /// The window manager's focus mode (`focus_mode::read_system()`
    /// reads it). Not given, it is read by `run_native` and taken as
    /// click elsewhere (tests stay the same on every desktop).
    pub fn system_focus(mut self, system: SystemFocus) -> Self {
        self.system_focus = Some(system);
        self
    }

    /// Sorts the plugins, connects them, installs the theme and resolves
    /// the declarations, on a context.
    pub fn build(self, ctx: &Context) -> Result<App, BuildError> {
        let nodes: Vec<Node> = self
            .plugins
            .iter()
            .map(|p| Node {
                id: p.id(),
                requires: p.requires(),
                priority: p.priority(),
            })
            .collect();
        let sorted = order(&nodes)?;
        let mut plugins: Vec<Option<Box<dyn Plugin>>> =
            self.plugins.into_iter().map(Some).collect();
        let slots: Vec<Slot> = sorted
            .into_iter()
            .map(|i| {
                let plugin = plugins[i].take().expect("each plugin once");
                let id = plugin.id();
                Slot {
                    owner: OwnerId::from(&id),
                    id,
                    plugin,
                    connected: false,
                    reason: None,
                }
            })
            .collect();
        let host = self.host.unwrap_or_else(|| {
            Box::new(LocalHost::new(
                self.store
                    .unwrap_or_else(|| Arc::new(Mutex::new(Store::new()))),
            ))
        });
        let handle = AppHandle::new(ctx, host);
        if let Some(check) = self.writer_check {
            lock(&handle.store()).set_writer_check(check);
        }
        let modes = FocusModes::new(self.system_focus.unwrap_or_default());
        let mut app = App {
            handle,
            plugins: slots,
            reg: Registries::new(),
            dispatcher: KeyDispatcher::new(),
            dirty: true,
            started: false,
            installed: None,
            preference: None,
            persist: self.persist,
            kits: Vec::new(),
            in_flight: Vec::new(),
            waking: None,
            wrap: self.wrap,
            on_exit: self.on_exit,
            autosave: AutoSave::default(),
            rebuilds: 0,
        };
        *app.handle.0.writer.borrow_mut() = app
            .persist
            .clone()
            .map(|p| Writer::new(p, egui_pop_core::persist::DEBOUNCE));
        app.handle.0.edge_to_edge.set(self.edge_to_edge);
        app.handle.0.menu_bar.set(self.menu_bar);
        *app.handle.0.shell_sizes.borrow_mut() = self.shell_sizes;
        app.handle.workbench_mut().layout.folded = self.folded;
        app.handle.workbench_mut().layout.over = self.over;
        *app.handle.0.name.borrow_mut() = self.name.unwrap_or_default();
        let mut themes = self.themes;
        let mut kits = self.kits;
        if self.user_kits
            && let Some(p) = &app.persist
        {
            let dir = p.dir().join(egui_pop_core::tokens::kit::USER_KITS_DIR);
            let cache = self
                .cache
                .unwrap_or_else(egui_pop_core::tokens::cache::default);
            kits.extend(
                egui_pop_core::tokens::kit::discover_with(&dir, &cache)
                    .into_iter()
                    .map(KitSource::new),
            );
        }
        // A kit of the same name as an earlier one replaces it.
        let mut seen = std::collections::BTreeSet::new();
        kits.reverse();
        kits.retain(|k| seen.insert(k.name().to_string()));
        kits.reverse();
        {
            let mut d = app.handle.0.diagnostics.borrow_mut();
            for kit in &kits {
                themes.add_kit(&kit.data);
                for p in kit.problems() {
                    d.warn("kit-problem", None, kit.name(), p);
                }
            }
        }
        let infos = kits.iter().map(|k| k.data.info()).collect();
        app.kits = kits;
        app.framework(self.theme, themes, infos, modes, self.panel_alignment)
            .expect("the framework's registrations are valid");
        app.install_collectors();
        for i in 0..app.plugins.len() {
            if let Err(error) = app.connect_slot(i) {
                let plugin = app.plugins[i].id.clone();
                return Err(BuildError::Connect { plugin, error });
            }
        }
        app.load_documents();
        app.rebuild();
        app.autosave_baseline();
        app.install_theme();
        app.emit(&Lifecycle::Started);
        app.started = true;
        Ok(app)
    }

    /// Builds and runs the application in a native window.
    #[cfg(all(feature = "eframe", not(target_arch = "wasm32")))]
    pub fn run_native(mut self, title: &str, mut options: eframe::NativeOptions) -> eframe::Result {
        self.system_focus
            .get_or_insert_with(egui_pop_core::focus_mode::read_system);
        self.name.get_or_insert_with(|| title.to_string());
        // The window's place from the last run, repaired (the screens are
        // not known before the window exists: only bounds are checked).
        if let Some(p) = &self.persist {
            crate::window::restore(p.dir(), &mut options);
        }
        eframe::run_native(
            title,
            options,
            Box::new(move |cc| {
                crate::frame::record_native_window(cc);
                let app = self
                    .build(&cc.egui_ctx)
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
                Ok(Box::new(app))
            }),
        )
    }

    /// As [`AppBuilder::run_native`], the built application handed to
    /// `make`, which gives eframe the `eframe::App` to run: one of the
    /// application's own around egui-pop's (it calls `App::logic`,
    /// `App::show_wrapped` and `App::exit` from its own). `make` may fail
    /// (its kernel does not start, a file is missing): the error is
    /// logged, published on the journal as `app.make_failed` and returned
    /// as eframe's `Error::AppCreation`.
    #[cfg(all(feature = "eframe", not(target_arch = "wasm32")))]
    pub fn run_native_with(
        mut self,
        title: &str,
        mut options: eframe::NativeOptions,
        make: impl FnOnce(App, &eframe::CreationContext<'_>) -> Result<Box<dyn eframe::App>, MakeError>
        + 'static,
    ) -> eframe::Result {
        self.system_focus
            .get_or_insert_with(egui_pop_core::focus_mode::read_system);
        self.name.get_or_insert_with(|| title.to_string());
        if let Some(p) = &self.persist {
            crate::window::restore(p.dir(), &mut options);
        }
        eframe::run_native(
            title,
            options,
            Box::new(move |cc| {
                crate::frame::record_native_window(cc);
                let app = self
                    .build(&cc.egui_ctx)
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
                let handle = app.handle().clone();
                make(app, cc).inspect_err(|e| report_make_error(&handle, e.as_ref()))
            }),
        )
    }
}

/// Why `make` (of `AppBuilder::run_native_with`) gave no application.
pub type MakeError = Box<dyn std::error::Error + Send + Sync>;

/// Says why `make` failed: in the log and on the journal
/// (`app.make_failed`, with the error).
#[cfg(any(test, all(feature = "eframe", not(target_arch = "wasm32"))))]
pub(crate) fn report_make_error(app: &AppHandle, e: &(dyn std::error::Error + Send + Sync)) {
    log::error!(target: "egui_pop", "the application could not be made: {e}");
    app.publish(
        "egui_pop",
        None,
        "app.make_failed",
        json!({ "error": e.to_string() }),
    );
}

/// A built application.
pub struct App {
    handle: AppHandle,
    plugins: Vec<Slot>,
    reg: Registries,
    dispatcher: KeyDispatcher,
    dirty: bool,
    started: bool,
    installed: Option<ThemeSpec>,
    /// The light or dark preference applied since the last install.
    preference: Option<egui::ThemePreference>,
    persist: Option<Persist>,
    /// The kits added: their faces and handlers are registered in each
    /// context the theme is installed in.
    kits: Vec<KitSource>,
    /// Results to come.
    in_flight: Vec<InFlight>,
    /// The context the host's waker wakes.
    waking: Option<Context>,
    wrap: Option<WrapUi>,
    on_exit: Vec<ExitFn>,
    /// What the debounced saving last saw and wrote.
    autosave: AutoSave,
    /// Rebuilds so far (the keymap may have changed).
    rebuilds: u64,
}

/// What the debounced saving of the settings, the keymap and the layout
/// last saw.
#[derive(Debug, Default)]
struct AutoSave {
    /// The store's sequence number last offered.
    store_seq: std::cell::Cell<Option<u64>>,
    /// The rebuild count when the keymap was last offered.
    rebuilds: std::cell::Cell<Option<u64>>,
    /// The layout's document and fingerprint, as last saved (or loaded).
    layout_saved: RefCell<Option<(String, u64)>>,
    /// The layout's fingerprint unsaved: when it first changed, when it
    /// last did.
    layout_seen: std::cell::Cell<Option<(u64, f64, f64)>>,
}

/// A layout's fingerprint.
fn layout_print(layout: &egui_pop_model::layout::Layout) -> u64 {
    use std::hash::{Hash as _, Hasher as _};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_string(layout)
        .unwrap_or_default()
        .hash(&mut h);
    h.finish()
}

/// A run waiting for its result.
struct InFlight {
    command: CommandId,
    args: Json,
    origin: Origin,
    seq: Seq,
    pending: Pending,
}

impl fmt::Debug for App {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("App")
            .field(
                "plugins",
                &self
                    .plugins
                    .iter()
                    .map(|p| (&p.id, p.connected))
                    .collect::<Vec<_>>(),
            )
            .finish_non_exhaustive()
    }
}

impl App {
    /// Starts building an application.
    pub fn builder() -> AppBuilder {
        AppBuilder::new()
    }

    /// The application handle.
    pub fn handle(&self) -> &AppHandle {
        &self.handle
    }

    /// How many things each registry holds: what removal tests compare.
    pub fn census(&self) -> BTreeMap<&'static str, usize> {
        let h = &self.handle.0;
        let store = lock(&h.store);
        BTreeMap::from([
            ("commands", self.reg.commands.len()),
            ("bindings", self.reg.keymap.all().count()),
            ("menu items", self.reg.menus.len()),
            ("panels", self.reg.panels.all().len()),
            ("panel views", h.panels.borrow().len()),
            ("hooks", self.reg.hooks.len()),
            ("lifecycle handlers", self.reg.lifecycle.len()),
            ("settings", self.reg.settings.len()),
            ("zones", h.zones.borrow().zones.len()),
            ("blocks", h.zones.borrow().contributions.len()),
            ("services", h.services.borrow().list().len()),
            ("stories", h.stories.borrow().len()),
            ("steps", h.steps.borrow().len()),
            ("step hooks", h.step_hooks.borrow().len()),
            ("list providers", h.lists.borrow().len()),
            ("indicators", h.indicators.borrow().len()),
            ("store paths", store.list("").len()),
            ("store listeners", store.listeners().len()),
        ])
    }

    /// The key dispatcher (its traces, a shortcut in progress).
    pub fn dispatcher(&self) -> &KeyDispatcher {
        &self.dispatcher
    }

    /// The keymap.
    pub fn keymap(&self) -> &Keymap {
        &self.reg.keymap
    }

    /// The framework's own registrations.
    fn framework(
        &mut self,
        theme: ThemeSpec,
        themes: Themes,
        kits: Vec<egui_pop_core::tokens::kit::KitInfo>,
        modes: FocusModes,
        panel_alignment: crate::workbench::PanelAlignment,
    ) -> Result<(), CtxError> {
        let owner = OwnerId::FRAMEWORK;
        let handle = self.handle.clone();
        let connected: Vec<PluginId> = Vec::new();
        let mut ctx = Ctx {
            owner: owner.clone(),
            plugin: PluginId::from_static("framework"),
            rank: 0,
            reg: &mut self.reg,
            app: &handle,
            connected: &connected,
        };
        ctx.step(
            StepSpec::new(
                crate::menus::STEP_OPEN,
                "A menu opens as a popup or a context menu; a veto keeps it closed, a redirect changes its arguments.",
            )
            .args(json!({ "menu": "app.menu.edit", "args": { "path": "node/exposure" }, "how": "context" })),
        )?;
        for (zone, title) in zones::SHELL.iter().zip([
            "Title",
            "Top bar",
            "Activity bar",
            "Side bar",
            "Main",
            "Inspector",
            "Status bar",
            "Overlays",
        ]) {
            ctx.zone(zone.clone(), title)?;
        }
        for (spec, zone, scope) in [
            (
                PanelSpec::new(
                    crate::workbench::PANEL_SIDE_BAR,
                    "Side bar",
                    Area::StartSide,
                )
                .min_size(120.0)
                .default_size(170.0),
                zones::SIDE_BAR,
                "sideBar",
            ),
            (
                PanelSpec::new(crate::workbench::PANEL_MAIN, "Main", Area::Center),
                zones::MAIN,
                "main",
            ),
            (
                PanelSpec::new(
                    crate::workbench::PANEL_INSPECTOR,
                    "Inspector",
                    Area::EndSide,
                )
                .min_size(160.0)
                .default_size(240.0),
                zones::INSPECTOR,
                "inspector",
            ),
        ] {
            let size = handle
                .0
                .shell_sizes
                .borrow()
                .iter()
                .find(|(p, _)| *p == spec.id)
                .map(|(_, s)| *s);
            let spec = match size {
                Some(s) => {
                    let min = spec.min_size.min(s);
                    spec.min_size(min).default_size(s)
                }
                None => spec,
            };
            ctx.zone_panel(spec, zone, scope)?;
        }
        for spec in crate::workbench::steps() {
            ctx.step(spec)?;
        }
        // Zen mode: no default key for it (applications bind one);
        // Escape leaves it, unless an overlay or a pushed mode wants it.
        ctx.command(
            CommandSpec::new(crate::workbench::CMD_ZEN, "Zen mode", "View")
                .description("Shows the centre alone: no title, menu or status bar, no side or bottom area, no palette. Escape leaves it.")
                .checked("workbenchZen")
                .menu(MENU_VIEW, "9_window", 2),
            |call| {
                let on = !call.app.zen();
                zen_result(call.app.set_zen(on), on)
            },
        )?
        .command(
            CommandSpec::new(crate::workbench::CMD_LEAVE_ZEN, "Leave zen mode", "View")
                .description("Shows the whole workbench again.")
                .when("workbenchZen"),
            |call| zen_result(call.app.set_zen(false), false),
        )?;
        ctx.keybinding(
            "Escape",
            crate::workbench::CMD_LEAVE_ZEN,
            "workbenchZen && !modePushed && !overlayOpen && !textInputFocus",
            Layer::Default,
        )?;
        crate::palettes::declare(&mut ctx)?;
        for (i, (menu, label)) in [
            (MENU_FILE, "File"),
            (MENU_EDIT, "Edit"),
            (MENU_VIEW, "View"),
            (MENU_HELP, "Help"),
        ]
        .into_iter()
        .enumerate()
        {
            ctx.submenu(MENU_MAIN, &format!("{i}"), 0, menu, label)?;
        }
        ctx.service(
            &THEME,
            ThemeService {
                spec: Mutex::new(theme),
                themes,
                kits,
                store: handle.store(),
                dark: Mutex::new(BTreeMap::new()),
            },
        )?;
        ctx.setting(
            SETTING_THEME_PREFERENCE,
            egui_pop_model::values::Descriptor::choice(
                "Light or dark",
                &["system", "light", "dark"],
                "system",
            )
            .description(
                "Which variant of a theme with a light and a dark one is shown: the system's, or always the light or the dark one. A theme without a counterpart keeps its own.",
            ),
        )?;
        ctx.command(
            CommandSpec::new(CMD_THEME_TOGGLE, "Toggle light and dark theme", "View")
                .description("Shows the other variant of the theme, light or dark, when it has one.")
                .when("themePaired")
                .menu(MENU_VIEW, "6_theme", 9),
            |call| {
                let mode = call.app.theme().toggle(&call.app.egui())?;
                Ok(json!({ "theme": call.app.theme().shown(&call.app.egui()), "dark": mode == egui::Theme::Dark }))
            },
        )?;
        let mut focus = egui_pop_model::values::Descriptor::choice(
            "Keyboard focus",
            &Preference::CHOICES,
            Preference::System.as_str(),
        );
        focus.description = format!(
            "Whether moving the mouse over a panel or a field gives it the keyboard \
             (hover), or only a click does (click). {}.",
            modes.system_said()
        );
        ctx.setting(SETTING_FOCUS, focus)?;
        crate::zoom::register(&mut ctx)?;
        crate::sliders::register(&mut ctx)?;
        ctx.setting(
            crate::workbench::SETTING_PANEL_ALIGNMENT,
            egui_pop_model::values::Descriptor::choice(
                "Panel alignment",
                &crate::workbench::PanelAlignment::CHOICES,
                panel_alignment.as_str(),
            )
            .description(
                "How the bottom panel lines up with the side bars: the whole width (justify), between them (center), under the start one (start) or under the end one (end).",
            ),
        )?;
        ctx.service(&FOCUS_MODE, modes)?;
        ctx.step(
            crate::steps::StepSpec::new(
                STEP_EXIT,
                "The application closes; the default saves the settings, keymap and layout.",
            )
            .args(json!({})),
        )?;
        ctx.step(
            crate::steps::StepSpec::new(
                STEP_LAYOUT_LOAD,
                "The workbench's layout is loaded from a document (`document`, as the host names it).",
            )
            .args(json!({ "document": crate::host::LAYOUT_DOCUMENT })),
        )?
        .step(
            crate::steps::StepSpec::new(
                STEP_LAYOUT_SAVE,
                "The workbench's layout is saved to a document (`document`, as the host names it).",
            )
            .args(json!({ "document": crate::host::LAYOUT_DOCUMENT })),
        )?;
        ctx.step(
            crate::steps::StepSpec::new(
                STEP_REBIND,
                "The user gives a command new keys (`command`, `keys`; null: no key).",
            )
            .args(json!({ "command": "app.quit", "keys": "Ctrl+Q" })),
        )?
        .step(
            crate::steps::StepSpec::new(
                STEP_LOAD_KEYMAP,
                "A keymap document is loaded into the user's or the workspace's layer.",
            )
            .args(json!({ "layer": "user", "bindings": 3 })),
        )?;
        ctx.step(
            StepSpec::new(
                STEP_UNDO,
                "Undoes the last change: the host's undo by default.",
            )
            .args(json!({ "label": "exposure change", "origin": "user" })),
        )?;
        ctx.step(
            StepSpec::new(
                STEP_REDO,
                "Redoes the last undone change: the host's redo by default.",
            )
            .args(json!({ "label": "exposure change", "origin": "user" })),
        )?;
        ctx.command(
            CommandSpec::new(CMD_UNDO, "Undo", "Edit")
                .description("Undo the last change.")
                .key("Ctrl+Z")
                .when("!textInputFocus && canUndo")
                .menu(MENU_EDIT, "0_undo", 0),
            |call| outcome_result(call.app.undo(call.origin)),
        )?;
        ctx.command(
            CommandSpec::new(CMD_REDO, "Redo", "Edit")
                .description("Redo the last undone change.")
                .key("Ctrl+Shift+Z")
                .key("Ctrl+Y")
                .when("!textInputFocus && canRedo")
                .menu(MENU_EDIT, "0_undo", 1),
            |call| outcome_result(call.app.redo(call.origin)),
        )?;
        ctx.command(
            CommandSpec::new(CMD_QUIT, "Quit", "File")
                .description("Close the application.")
                .key("Ctrl+Q")
                .menu(MENU_FILE, "9_quit", 0)
                .no_agent(),
            |call| {
                call.app
                    .egui()
                    .send_viewport_cmd(egui::ViewportCommand::Close);
                Ok(Json::Null)
            },
        )?;
        let store = self.handle.store();
        egui_pop_model::values::register_with(&mut self.reg.commands, &owner, store, |h| {
            let f: CommandFn = Rc::new(move |call: &Call<'_>| h(call.args, call.origin));
            Some(f)
        })
        .ok();
        Ok(())
    }

    /// The api's collectors: commands, keys, zones, plugins, services,
    /// diagnostics, the store.
    fn install_collectors(&mut self) {
        let h = &self.handle;
        let up = |w: &Weak<Shared>| {
            w.upgrade()
                .map(AppHandle)
                .ok_or_else(|| "the application is gone".to_string())
        };
        let mut b = h.0.bridge.borrow_mut();
        let (w1, w2) = (h.weak(), h.weak());
        b.register(
            FnCollector::new("commands", "Every command: id, title, keys, where it applies, owner; the last runs.")
                .snapshot(move |_| {
                    let Ok(app) = up(&w1) else { return Json::Null };
                    let cx = app.when_context();
                    let palette = app.palette();
                    json!({
                        "commands": app.commands().iter().map(|(s, owner)| json!({
                            "id": s.id,
                            "title": s.title,
                            "category": s.category,
                            "description": s.description,
                            "when": s.when,
                            "keys": palette.iter().find(|e| e.command == s.id).map(|e| e.keys.clone()).unwrap_or_default(),
                            "agent": s.agent,
                            "args": s.args,
                            "running": app.running(&s.id),
                            "owner": owner,
                        })).collect::<Vec<_>>(),
                        "context": cx.iter().map(|(k, v, d)| json!({"key": k, "value": v, "depth": d})).collect::<Vec<_>>(),
                        "history": app.history(),
                    })
                })
                .action("run", "Runs a command offered to agents (`id`, `args`).", true, move |_, args| {
                    let app = up(&w2)?;
                    let id = args.get("id").and_then(Json::as_str).ok_or("`id` is missing")?;
                    let id = CommandId::new(id).map_err(|e| e.to_string())?;
                    let offered = app.commands().iter().any(|(s, _)| s.id == id && s.agent);
                    if !offered {
                        return Err(format!("`{id}` is not offered to agents"));
                    }
                    app.run_as(id, args.get("args").cloned().unwrap_or(Json::Null), Origin::Agent("debug-bridge".into()));
                    Ok(json!({ "queued": true }))
                })
                .args(json!({ "id": "app.palette", "args": null })),
        )
        .expect("distinct names");
        let (w1, w2) = (h.weak(), h.weak());
        b.register(
            FnCollector::new(
                "journal",
                "The host's event journal (sequence, source, cause, name, data): the last 64, or since a sequence number.",
            )
            .snapshot(move |_| {
                let Ok(app) = up(&w1) else { return Json::Null };
                let last = app.last_event();
                json!({ "last": last, "events": app.events_since(last.saturating_sub(64)) })
            })
            .action("since", "The events after `seq`.", false, move |_, args| {
                let app = up(&w2)?;
                let seq = args.get("seq").and_then(Json::as_u64).unwrap_or(0);
                Ok(json!(app.events_since(seq)))
            })
            .args(json!({ "seq": 0 })),
        )
        .expect("distinct names");
        let w = h.weak();
        b.register(
            FnCollector::new(
                "keys",
                "The keymap's last resolutions, each binding considered and why it won or lost.",
            )
            .snapshot(move |_| {
                let Ok(app) = up(&w) else { return Json::Null };
                json!({ "pending": app.pending_keys(), "traces": app.key_traces() })
            }),
        )
        .expect("distinct names");
        let w = h.weak();
        b.register(
            FnCollector::new(
                "zones",
                "Every zone and its resolved blocks: owner, weight, what replaced or wrapped them.",
            )
            .snapshot(move |_| {
                let Ok(app) = up(&w) else { return Json::Null };
                json!(app.zones())
            }),
        )
        .expect("distinct names");
        let (w1, w2) = (h.weak(), h.weak());
        b.register(
            FnCollector::new("plugins", "The plugins in connection order, connected or why not; the services.")
                .snapshot(move |_| {
                    let Ok(app) = up(&w1) else { return Json::Null };
                    json!({
                        "plugins": app.plugins(),
                        "services": app.services().iter().map(|(i, o, t)| json!({"id": i, "owner": o, "type": t})).collect::<Vec<_>>(),
                    })
                })
                .action("set_enabled", "Connects or disconnects a plugin (`id`, `enabled`), before the next frame.", true, move |_, args| {
                    let app = up(&w2)?;
                    let id = args.get("id").and_then(Json::as_str).ok_or("`id` is missing")?;
                    let id = PluginId::new(id).map_err(|e| e.to_string())?;
                    if !app.plugins().iter().any(|p| p.id == id) {
                        return Err(format!("no plugin `{id}`"));
                    }
                    let enabled = args.get("enabled").and_then(Json::as_bool).ok_or("`enabled` is missing")?;
                    app.set_plugin_enabled(id, enabled);
                    Ok(json!({ "queued": true }))
                })
                .args(json!({ "id": "plugin.id", "enabled": false })),
        )
        .expect("distinct names");
        let w = h.weak();
        b.register(
            FnCollector::new(
                "diagnostics",
                "What registration ignored or changed, and what happens instead.",
            )
            .snapshot(move |_| {
                let Ok(app) = up(&w) else { return Json::Null };
                json!(app.diagnostics())
            }),
        )
        .expect("distinct names");
        let w = h.weak();
        b.register(
            FnCollector::new(
                "steps",
                "Every declared step with its hooks (before, replace, after), and the last runs: who redirected, vetoed, ran and followed.",
            )
            .snapshot(move |_| {
                let Ok(app) = up(&w) else { return Json::Null };
                json!({ "steps": app.steps(), "runs": app.step_traces() })
            }),
        )
        .expect("distinct names");
        let w = h.weak();
        b.register(
            FnCollector::new(
                "lists",
                "The menus' list providers (name, owner) and the entries each gave when its menu last opened.",
            )
            .snapshot(move |_| {
                let Ok(app) = up(&w) else { return Json::Null };
                app.lists()
            }),
        )
        .expect("distinct names");
        let w = h.weak();
        b.register(
            FnCollector::new(
                "menu_bar",
                "Each menu drawn as a bar (the shell's `app.main`): the menus in the bar, those moved into its \"…\" menu, whether it collapsed into one entry, and each menu's access key.",
            )
            .snapshot(move |_| {
                let Ok(app) = up(&w) else { return Json::Null };
                json!(crate::menus::bar_layouts(&app))
            }),
        )
        .expect("distinct names");
        let w = h.weak();
        b.register(
            FnCollector::new(
                "history",
                "What Edit > Undo and Redo would do, as the host says (the runs are in the `steps` collector).",
            )
            .snapshot(move |_| {
                let Ok(app) = up(&w) else { return Json::Null };
                json!({
                    "can_undo": app.can_undo(),
                    "undo": app.undo_label(),
                    "can_redo": app.can_redo(),
                    "redo": app.redo_label(),
                })
            }),
        )
        .expect("distinct names");
        let w = h.weak();
        b.register(
            FnCollector::new(
                "overlay_rects",
                "Every overlay on screen with where it was drawn last (points): dialogs, modals, popovers, menus, the toasts, egui's popups.",
            )
            .snapshot(move |_| {
                let Ok(app) = up(&w) else { return Json::Null };
                json!(
                    app.overlay_rects()
                        .iter()
                        .map(|o| json!({
                            "kind": o.kind,
                            "id": o.id,
                            "rect": [o.rect.min.x, o.rect.min.y, o.rect.max.x, o.rect.max.y],
                        }))
                        .collect::<Vec<_>>()
                )
            }),
        )
        .expect("distinct names");
        let w = h.weak();
        b.register(
            FnCollector::new(
                "workbench",
                "The workbench: the layout (areas, folded, closed and detached panels), where each area was drawn (points), the focused panel, the tab being dragged and the drop hint (its rect, name and refusal).",
            )
            .snapshot(move |_| {
                let Ok(app) = up(&w) else { return Json::Null };
                let layout = app.layout();
                let wb = app.workbench_mut();
                json!({
                    "layout": layout,
                    "rects": wb.rects.iter().map(|(a, r)| (
                        crate::workbench::area_name(*a),
                        [r.min.x, r.min.y, r.max.x, r.max.y],
                    )).collect::<BTreeMap<_, _>>(),
                    "focused": wb.focused,
                    "zen": wb.zen,
                    "dragging": wb.dragging.as_ref().map(|(p, a, _)| json!({
                        "panel": p,
                        "from": crate::workbench::area_name(*a),
                    })),
                    "drop_hint": wb.hint.as_ref().map(|h| json!({
                        "name": h.name(),
                        "panel": h.panel,
                        "area": h.area,
                        "rect": [h.rect.min.x, h.rect.min.y, h.rect.max.x, h.rect.max.y],
                        "refusal": h.refusal,
                        "preview": h.preview.as_ref().map(|p| json!({
                            "area": p.area,
                            "index": p.index,
                            "rect": [p.rect.min.x, p.rect.min.y, p.rect.max.x, p.rect.max.y],
                        })),
                    })),
                })
            }),
        )
        .expect("distinct names");
        b.register(egui_pop_core::debug::store_collector(h.store()))
            .expect("distinct names");
        b.register(crate::mounts::collector(h.downgrade()))
            .expect("distinct names");
    }

    /// Calls the debug bridge; `commands.run` runs at once and its result
    /// is in the feedback.
    pub fn debug_call(&mut self, collector: &str, action: &str, args: &Json) -> Feedback {
        let feedback = self.handle.debug_call(collector, action, args);
        if collector == "commands" && action == "run" && feedback.ok {
            self.handle.0.requests.borrow_mut().pop();
            let id = args
                .get("id")
                .and_then(Json::as_str)
                .and_then(|i| CommandId::new(i).ok());
            if let Some(id) = id {
                let reply = self.run(
                    &id,
                    args.get("args").cloned().unwrap_or(Json::Null),
                    Origin::Agent("debug-bridge".into()),
                );
                let Some(result) = reply.now() else {
                    return Feedback {
                        message: format!("`{id}` is running; its result goes to the journal"),
                        data: json!({ "pending": true }),
                        ..feedback
                    };
                };
                return Feedback {
                    ok: result.is_ok(),
                    message: match &result {
                        Ok(_) => format!("`{id}` ran; the application may have changed"),
                        Err(e) => format!("`{id}` failed: {e}"),
                    },
                    data: result.unwrap_or(Json::Null),
                    ..feedback
                };
            }
        }
        if feedback.ok && feedback.mutates {
            self.process_requests();
        }
        feedback
    }

    fn connected_ids(&self) -> Vec<PluginId> {
        self.plugins
            .iter()
            .filter(|p| p.connected)
            .map(|p| p.id.clone())
            .collect()
    }

    /// Connects one plugin (its requirements must be connected). A
    /// failure removes what it registered.
    fn connect_slot(&mut self, i: usize) -> Result<(), PluginError> {
        let connected = self.connected_ids();
        let slot = &self.plugins[i];
        if let Some(missing) = slot
            .plugin
            .requires()
            .into_iter()
            .find(|r| !connected.contains(r))
        {
            let reason = format!("its requirement `{missing}` is not connected");
            self.handle.0.diagnostics.borrow_mut().warn(
                "plugin-skipped",
                Some(&slot.owner),
                slot.id.as_str(),
                format!("{reason}: it is not connected"),
            );
            self.plugins[i].reason = Some(reason);
            self.update_plugin_infos();
            return Ok(());
        }
        let handle = self.handle.clone();
        let owner = slot.owner.clone();
        let id = slot.id.clone();
        let mut ctx = Ctx {
            owner: owner.clone(),
            plugin: id.clone(),
            rank: i + 1,
            reg: &mut self.reg,
            app: &handle,
            connected: &connected,
        };
        let slot = &mut self.plugins[i];
        // What it writes while connecting is written as the plugin.
        let _scope = egui_pop_model::values::enter_scope(&owner);
        if !slot.plugin.enabled(&ctx) {
            slot.reason = Some("it is disabled".into());
            self.update_plugin_infos();
            return Ok(());
        }
        match slot.plugin.connect(&mut ctx) {
            Ok(()) => {
                slot.connected = true;
                slot.reason = None;
                self.handle.zones_mut().set_rank(&owner, i + 1);
                self.dirty = true;
                self.update_plugin_infos();
                if self.started {
                    self.emit(&Lifecycle::Connected(id));
                }
                Ok(())
            }
            Err(e) => {
                self.remove_owner(&owner);
                let slot = &mut self.plugins[i];
                slot.reason = Some(format!("it failed to connect: {e}"));
                self.handle.0.diagnostics.borrow_mut().error(
                    "connect-failed",
                    Some(&owner),
                    id.as_str(),
                    format!("{e}: what it registered is removed"),
                );
                self.update_plugin_infos();
                Err(e)
            }
        }
    }

    fn remove_owner(&mut self, owner: &OwnerId) {
        self.reg.remove_owner(owner);
        let h = &self.handle.0;
        h.zones.borrow_mut().remove_owner(owner);
        h.panels.borrow_mut().retain(|_, p| p.owner != *owner);
        h.stories.borrow_mut().remove_owner(owner);
        h.steps.borrow_mut().remove_owner(owner);
        h.step_hooks.borrow_mut().remove_owner(owner);
        h.lists.borrow_mut().remove_owner(owner);
        h.list_traces.borrow_mut().retain(|_, t| t.owner != *owner);
        h.indicators.borrow_mut().remove_owner(owner);
        h.workbench.borrow_mut().templates.retain(|t| t.0 != *owner);
        h.services.borrow_mut().remove_owner(owner);
        lock(&h.store).remove_owner(owner);
        h.diagnostics.borrow_mut().remove_owner(owner);
        h.collectors.borrow_mut().retain(|(o, name)| {
            if o == owner {
                h.bridge.borrow_mut().unregister(name);
            }
            o != owner
        });
        self.dirty = true;
    }

    fn update_plugin_infos(&self) {
        *self.handle.0.plugins.borrow_mut() = self
            .plugins
            .iter()
            .map(|s| PluginInfo {
                id: s.id.clone(),
                title: s.plugin.title(),
                requires: s.plugin.requires(),
                priority: s.plugin.priority(),
                connected: s.connected,
                reason: s.reason.clone(),
            })
            .collect();
    }

    /// Disconnects a plugin and those requiring it (dependents first);
    /// everything they registered is removed. Returns the plugins
    /// disconnected.
    pub fn disconnect(&mut self, plugin: &PluginId) -> Vec<PluginId> {
        let mut out = Vec::new();
        let Some(i) = self.plugins.iter().position(|p| p.id == *plugin) else {
            return out;
        };
        if !self.plugins[i].connected {
            return out;
        }
        // Dependents first, latest first.
        for j in (i + 1..self.plugins.len()).rev() {
            if self.plugins[j].connected && self.plugins[j].plugin.requires().contains(plugin) {
                let dep = self.plugins[j].id.clone();
                out.extend(self.disconnect(&dep));
            }
        }
        let handle = self.handle.clone();
        let connected = self.connected_ids();
        let slot = &mut self.plugins[i];
        let owner = slot.owner.clone();
        let mut ctx = Ctx {
            owner: owner.clone(),
            plugin: slot.id.clone(),
            rank: i + 1,
            reg: &mut self.reg,
            app: &handle,
            connected: &connected,
        };
        slot.plugin.disconnect(&mut ctx);
        slot.connected = false;
        slot.reason = Some("it was disconnected".into());
        self.remove_owner(&owner);
        self.update_plugin_infos();
        self.emit(&Lifecycle::Disconnected(plugin.clone()));
        out.push(plugin.clone());
        out
    }

    /// Connects a plugin that is not (its requirements first).
    pub fn connect(&mut self, plugin: &PluginId) -> Result<(), PluginError> {
        let Some(i) = self.plugins.iter().position(|p| p.id == *plugin) else {
            return Err(PluginError::Failed(format!("no plugin `{plugin}`")));
        };
        if self.plugins[i].connected {
            return Ok(());
        }
        for r in self.plugins[i].plugin.requires() {
            self.connect(&r)?;
        }
        self.connect_slot(i)
    }

    fn emit(&mut self, event: &Lifecycle) {
        let handle = self.handle.clone();
        // Handlers get the handle, not `Ctx`: the list cannot change
        // while they run.
        for slot in self.reg.lifecycle.iter_mut() {
            let (on, f) = &mut slot.item;
            if on.matches(event) {
                let _scope = egui_pop_model::values::enter_scope(&slot.owner);
                f(&handle, event);
            }
        }
    }

    /// Rebuilds the derived declarations: menus, palette and settings
    /// through their hooks, and the zones. Runs after plugins come or go,
    /// never per frame.
    pub fn rebuild(&mut self) {
        let mut d = Diagnostics::new();
        let reg = &self.reg;
        // Hooks in order: `after` constraints, then priority.
        let hooks: Vec<&Hook> = reg.hooks.iter().map(|s| &s.item).collect();
        let nodes: Vec<Node> = hooks
            .iter()
            .map(|h| {
                let id = egui_pop_model::ids::PluginId::new(h.id.as_str()).expect("same syntax");
                let after = h
                    .after
                    .iter()
                    .filter(|a| {
                        let known = hooks.iter().any(|o| o.id == **a);
                        if !known {
                            d.warn(
                                "dangling-after",
                                None,
                                h.id.as_str(),
                                format!("hook `{}` runs after `{a}`, which is not registered: it runs by priority alone", h.id),
                            );
                        }
                        known
                    })
                    .map(|a| PluginId::new(a.as_str()).expect("same syntax"))
                    .collect();
                Node {
                    id,
                    requires: after,
                    priority: h.priority,
                }
            })
            .collect();
        let hook_order: Vec<usize> = match order(&nodes) {
            Ok(o) => o,
            Err(e) => {
                d.error(
                    "hook-order",
                    None,
                    "hooks",
                    format!("{e}: hooks run in registration order"),
                );
                (0..hooks.len()).collect()
            }
        };
        let mut derived = Derived::default();
        for id in reg.menus.ids() {
            let mut items = reg.menus.items(&id);
            sort_items(&mut items);
            derived.menus.insert(id, items);
        }
        derived.palette = reg
            .commands
            .specs()
            .filter(|s| s.palette)
            .map(|s| PaletteEntry {
                command: s.id.clone(),
                title: s.title.clone(),
                category: s.category.clone(),
                keys: reg
                    .keymap
                    .keys_for(&s.id)
                    .iter()
                    .map(|k| k.to_string())
                    .collect(),
            })
            .collect();
        derived
            .palette
            .sort_by(|a, b| (&a.category, &a.title).cmp(&(&b.category, &b.title)));
        let titles: BTreeMap<OwnerId, String> = self
            .plugins
            .iter()
            .map(|s| (s.owner.clone(), s.plugin.title()))
            .chain([(OwnerId::FRAMEWORK, FRAMEWORK_SECTION.to_string())])
            .collect();
        let store = lock(&self.handle.0.store);
        derived.sections.clone_from(&titles);
        derived.settings = reg
            .settings
            .in_order()
            .into_iter()
            .filter_map(|(id, slot)| {
                store.get(&slot.item).map(|e| SettingEntry {
                    id: id.clone(),
                    path: slot.item.clone(),
                    owner: slot.owner.clone(),
                    section: titles
                        .get(&slot.owner)
                        .cloned()
                        .unwrap_or_else(|| slot.owner.to_string()),
                    descriptor: e.descriptor.clone(),
                })
            })
            .collect();
        drop(store);
        for &i in &hook_order {
            match &hooks[i].alter {
                Alter::Menu(menu, f) => {
                    if let Some(items) = derived.menus.get_mut(menu) {
                        f(items);
                    } else {
                        let mut items = Vec::new();
                        f(&mut items);
                        derived.menus.insert(menu.clone(), items);
                    }
                }
                Alter::Palette(f) => f(&mut derived.palette),
                Alter::Settings(f) => f(&mut derived.settings),
            }
        }
        derived.commands = reg
            .commands
            .specs()
            .map(|s| {
                (
                    s.clone(),
                    reg.commands
                        .owner(&s.id)
                        .cloned()
                        .unwrap_or(OwnerId::FRAMEWORK),
                )
            })
            .collect();
        derived.keys = reg
            .commands
            .specs()
            .map(|s| {
                let keys = reg.keymap.keys_for(&s.id);
                (s.id.clone(), keys.iter().map(|k| k.to_string()).collect())
            })
            .collect();
        derived.sources = reg
            .commands
            .specs()
            .map(|s| (s.id.clone(), reg.keymap.sources_for(&s.id)))
            .filter(|(_, b)| !b.is_empty())
            .collect();
        derived.panels = reg
            .panels
            .owned()
            .into_iter()
            .map(|(spec, owner)| (spec.clone(), owner.clone()))
            .collect();
        derived.conflicts = reg.keymap.conflicts();
        derived.system = reg.keymap.system_bindings();
        derived.layers = [
            Layer::Suggestion,
            Layer::Default,
            Layer::Workspace,
            Layer::User,
        ]
        .into_iter()
        .map(|l| (l, reg.keymap.export_layer(l)))
        .collect();
        // New conflicts are signalled once.
        let before: Vec<String> = self
            .handle
            .0
            .derived
            .borrow()
            .conflicts
            .iter()
            .map(|(l, w)| format!("{} {} {}", l.keys, l.action, w.action))
            .collect();
        for (lose, win) in &derived.conflicts {
            let key = format!("{} {} {}", lose.keys, lose.action, win.action);
            if self.started && !before.contains(&key) {
                self.handle.publish(
                    "egui_pop",
                    None,
                    "keys.conflict",
                    json!({
                        "keys": lose.keys.to_string(),
                        "scope": lose.options.scope.to_string(),
                        "loses": lose.action.to_string(),
                        "wins": win.action.to_string(),
                    }),
                );
            }
        }
        derived.rebound = reg
            .commands
            .specs()
            .filter(|s| reg.keymap.rebound(&OwnerId::USER, Layer::User, &s.id))
            .map(|s| s.id.clone())
            .collect();
        derived.rebound_workspace = reg
            .commands
            .specs()
            .filter(|s| reg.keymap.rebound(&WORKSPACE_KEYS, Layer::Workspace, &s.id))
            .map(|s| s.id.clone())
            .collect();
        // Menu items naming unknown commands are reported.
        for (menu, items) in &derived.menus {
            for it in items {
                if let Item::Command { id, .. } = &it.item
                    && reg.commands.spec(id).is_none()
                {
                    d.warn(
                        "unknown-command",
                        None,
                        id.as_str(),
                        format!("menu `{menu}` names command `{id}`, which is not declared: the item is shown greyed"),
                    );
                }
            }
        }
        *self.handle.0.derived.borrow_mut() = derived;
        self.check_steps(&mut d);
        self.handle.zones_mut().resolve(&mut d);
        // The panels, but a zone shown as a panel only when it has blocks.
        let zone_of = self.handle.workbench_mut().zone_of.clone();
        let specs: Vec<egui_pop_model::panels::PanelSpec> = self
            .reg
            .panels
            .all()
            .into_iter()
            .filter(|p| {
                zone_of
                    .get(&p.id)
                    .is_none_or(|z| self.handle.zone_has_blocks(z))
            })
            .cloned()
            .collect();
        for note in self.handle.workbench_mut().refresh(specs, &SCREENS) {
            log::info!(target: "egui_pop", "layout: {note}");
        }
        let mut all = self.handle.0.diagnostics.borrow_mut();
        // Resolution diagnostics are recomputed each rebuild.
        let kept: Vec<Diagnostic> = all
            .take()
            .into_iter()
            .filter(|x| !RESOLUTION_CODES.contains(&x.code))
            .collect();
        for x in kept {
            all.push_quiet(x);
        }
        all.extend(d.take());
        drop(all);
        self.dirty = false;
        self.rebuilds += 1;
        if self.started {
            self.emit(&Lifecycle::Rebuilt);
        }
    }

    /// Hooks on steps nobody declared never run; two run-hooks on one
    /// step: the last wins.
    fn check_steps(&self, d: &mut Diagnostics) {
        let steps = self.handle.0.steps.borrow();
        let hooks = self.handle.0.step_hooks.borrow();
        let mut replaced: BTreeMap<StepId, Vec<OwnerId>> = BTreeMap::new();
        for h in hooks.iter() {
            if !steps.contains(&h.item.step) {
                d.warn(
                    "unknown-step",
                    Some(&h.owner),
                    h.item.step.as_str(),
                    format!(
                        "a {} hook on step `{}`, which nobody declares: it never runs",
                        h.item.f.kind(),
                        h.item.step
                    ),
                );
            }
            if matches!(h.item.f, HookFn::Run(_)) {
                replaced
                    .entry(h.item.step.clone())
                    .or_default()
                    .push(h.owner.clone());
            }
        }
        for (step, owners) in replaced {
            if owners.len() > 1 {
                let last = owners.last().expect("two or more");
                d.warn(
                    "step-replaced-twice",
                    Some(last),
                    step.as_str(),
                    format!(
                        "step `{step}` is replaced by {}: `{last}` (the last) runs it",
                        owners
                            .iter()
                            .map(|o| format!("`{o}`"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                );
            }
        }
    }

    fn load_documents(&mut self) {
        if self.persist.is_none() {
            return;
        }
        if let Some(Outcome::Done(Err(e))) = self.load_layout() {
            self.handle.0.diagnostics.borrow_mut().error(
                "layout-unreadable",
                None,
                "layout",
                format!("{e}: the default layout is used"),
            );
        }
        let Some(p) = &self.persist else { return };
        match p.load::<egui_pop_model::values::SettingsDoc>("settings", &settings_schema()) {
            Ok(Some(read)) => {
                let d = lock(&self.handle.0.store).import(&read.value, &Origin::User);
                self.handle
                    .0
                    .diagnostics
                    .borrow_mut()
                    .extend(d.all().iter().cloned());
            }
            Ok(None) => {}
            Err(e) => self.handle.0.diagnostics.borrow_mut().error(
                "settings-unreadable",
                None,
                "settings",
                format!("{e}: the defaults are used"),
            ),
        }
        match p
            .load::<egui_pop_model::keymap::KeymapDoc>("keymap", &egui_pop_model::keymap_schema())
        {
            Ok(Some(read)) => {
                let d = self
                    .reg
                    .keymap
                    .load(&OwnerId::USER, Layer::User, &read.value);
                self.handle
                    .0
                    .diagnostics
                    .borrow_mut()
                    .extend(d.all().iter().cloned());
            }
            Ok(None) => {}
            Err(e) => self.handle.0.diagnostics.borrow_mut().error(
                "keymap-unreadable",
                None,
                "keymap",
                format!("{e}: the default keys are used"),
            ),
        }
    }

    /// Saves now the settings, the user's keymap (when a directory was
    /// given), the layout ([`App::save_layout`]) and every document
    /// waiting in the debounced writer.
    pub fn save(&self) -> Result<(), egui_pop_core::persist::PersistError> {
        let Some(p) = &self.persist else {
            return Ok(());
        };
        let (settings, keymap) = self.document_texts()?;
        p.save_text("settings", &settings)?;
        self.handle.saved_already("settings", &settings);
        p.save_text("keymap", &keymap)?;
        self.handle.saved_already("keymap", &keymap);
        let layout = match self.save_layout() {
            Some(Outcome::Done(Err(e))) => Err(egui_pop_core::persist::PersistError::Io {
                path: p.dir().to_path_buf(),
                error: std::io::Error::other(e),
            }),
            _ => Ok(()),
        };
        let mut flushed = Ok(());
        for (_, r) in self.handle.flush_documents() {
            if let Err(e) = r {
                flushed = Err(e);
            }
        }
        layout.and(flushed)
    }

    /// The settings' and the user keymap's documents as they stand.
    fn document_texts(&self) -> Result<(String, String), PersistError> {
        let doc_error = |name: &str, error| PersistError::Doc {
            path: std::path::PathBuf::from(name),
            error,
            backup: None,
        };
        let doc = lock(&self.handle.0.store).export("settings/");
        let settings = settings_schema()
            .save(&doc)
            .map_err(|e| doc_error("settings", e))?;
        let keymap = self.reg.keymap.export(&OwnerId::USER, Layer::User);
        let keymap = egui_pop_model::keymap_schema()
            .save(&keymap)
            .map_err(|e| doc_error("keymap", e))?;
        Ok((settings, keymap))
    }

    /// What was loaded is what is on disk: the debounced saving writes
    /// only what changes from now on.
    fn autosave_baseline(&mut self) {
        if self.persist.is_none() {
            return;
        }
        if let Ok((settings, keymap)) = self.document_texts() {
            self.handle.saved_already("settings", &settings);
            self.handle.saved_already("keymap", &keymap);
        }
        self.autosave
            .store_seq
            .set(Some(lock(&self.handle.0.store).seq()));
        self.autosave.rebuilds.set(Some(self.rebuilds));
        self.layout_baseline();
    }

    /// The layout as it stands is the one on disk (just loaded or saved).
    fn layout_baseline(&self) {
        let document = self.layout_document();
        let print = layout_print(&self.handle.layout());
        *self.autosave.layout_saved.borrow_mut() = document.map(|d| (d, print));
        self.autosave.layout_seen.set(None);
    }

    /// Saves what changed a short while after it changed: the settings,
    /// the user's keymap, the layout (through `layout.save`, to the
    /// document the host names now) and the documents plugins offered.
    /// Saving never waits for the exit, so a crash loses at most the
    /// last second.
    fn autosave(&mut self, ctx: &Context) {
        if self.persist.is_none() {
            return;
        }
        let now = egui_pop_core::motion::now(ctx);
        let seq = lock(&self.handle.0.store).seq();
        let keymap_may_change = self.autosave.rebuilds.get() != Some(self.rebuilds);
        if self.autosave.store_seq.get() != Some(seq) || keymap_may_change {
            self.autosave.store_seq.set(Some(seq));
            self.autosave.rebuilds.set(Some(self.rebuilds));
            if let Ok((settings, keymap)) = self.document_texts() {
                self.handle.save_later("settings", settings);
                self.handle.save_later("keymap", keymap);
            }
        }
        let mut wake: Option<f64> = None;
        if let Some(document) = self.layout_document() {
            let print = layout_print(&self.handle.layout());
            let saved = self.autosave.layout_saved.borrow().clone();
            match saved {
                // Another document (the host switched workspaces): what
                // is shown is not a change of this one.
                Some((d, _)) if d != document => self.layout_baseline(),
                Some((_, p)) if p == print => self.autosave.layout_seen.set(None),
                None => self.layout_baseline(),
                Some(_) => {
                    let delay = egui_pop_core::persist::DEBOUNCE;
                    let (since, last) = match self.autosave.layout_seen.get() {
                        Some((p, since, last)) if p == print => (since, last),
                        Some((_, since, _)) => (since, now),
                        None => (now, now),
                    };
                    self.autosave.layout_seen.set(Some((print, since, last)));
                    let due = (last + delay).min(since + 5.0 * delay);
                    if now >= due {
                        if let Some(Outcome::Done(Err(e))) = self.save_layout() {
                            log::warn!(target: "egui_pop", "saving the layout: {e}");
                        }
                        self.layout_baseline();
                    } else {
                        wake = Some(due);
                    }
                }
            }
        }
        let due = {
            let mut w = self.handle.0.writer.borrow_mut();
            match w.as_mut() {
                Some(w) => {
                    for (name, r) in w.tick(now) {
                        if let Err(e) = r {
                            log::warn!(target: "egui_pop", "saving `{name}`: {e}");
                        }
                    }
                    w.next_due()
                }
                None => None,
            }
        };
        for d in [due, wake].into_iter().flatten() {
            if d > now {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64((d - now).max(0.05)));
            }
        }
    }

    /// The document the layout is kept in now, as the host says
    /// (`Host::layout_document`); `None`: the host keeps it.
    pub fn layout_document(&self) -> Option<String> {
        self.handle.with_host(|h| h.layout_document()).flatten()
    }

    /// Loads the layout from the host's document, through the
    /// `layout.load` step (an application switching workspaces calls it
    /// once its host names the new one). `None` when the host keeps the
    /// layout; a document not written yet keeps the layout as it is.
    pub fn load_layout(&mut self) -> Option<Outcome> {
        let document = self.layout_document()?;
        let persist = self.persist.clone();
        let started = self.started;
        Some(self.handle.step(
            &STEP_LAYOUT_LOAD,
            json!({ "document": document }),
            move |app, args| {
                let document = args["document"].as_str().ok_or("`document` is missing")?;
                let Some(p) = persist else {
                    return Ok(json!({ "document": document, "loaded": false }));
                };
                let read = p
                    .load::<egui_pop_model::layout::Layout>(
                        document,
                        &egui_pop_model::layout::schema(),
                    )
                    .map_err(|e| e.to_string())?;
                let Some(read) = read else {
                    return Ok(json!({ "document": document, "loaded": false }));
                };
                let notes = if started {
                    app.set_layout(read.value)
                } else {
                    // Repaired against the panels when the application is
                    // first resolved.
                    app.workbench_mut().layout = read.value;
                    Vec::new()
                };
                Ok(json!({ "document": document, "loaded": true, "repaired": notes }))
            },
        ))
        .inspect(|o| {
            if o.ok() && self.started {
                self.layout_baseline();
            }
        })
    }

    /// Saves the layout to the host's document, through the
    /// `layout.save` step. `None` when the host keeps the layout.
    pub fn save_layout(&self) -> Option<Outcome> {
        let document = self.layout_document()?;
        let persist = self.persist.clone();
        Some(self.handle.step(
            &STEP_LAYOUT_SAVE,
            json!({ "document": document }),
            move |app, args| {
                let document = args["document"].as_str().ok_or("`document` is missing")?;
                let Some(p) = persist else {
                    return Ok(json!({ "document": document, "saved": false }));
                };
                p.save(document, &egui_pop_model::layout::schema(), &app.layout())
                    .map_err(|e| e.to_string())?;
                Ok(json!({ "document": document, "saved": true }))
            },
        ))
        .inspect(|o| {
            if o.ok() {
                self.layout_baseline();
            }
        })
    }

    /// Declares a command now (see `AppHandle::declare`).
    pub fn declare(&mut self, owner: &OwnerId, spec: CommandSpec) -> Result<(), CtxError> {
        let id = spec.id.clone();
        match crate::ctx::add_command(&mut self.reg, owner, spec, None) {
            Ok(()) => {
                self.rebuild();
                Ok(())
            }
            Err(e) => {
                self.handle.publish(
                    "egui_pop",
                    None,
                    "command.declare_failed",
                    json!({ "command": id, "owner": owner, "error": e.to_string() }),
                );
                Err(e)
            }
        }
    }

    /// Removes a command, its owner's keys and menu items for it; whether
    /// it was declared.
    pub fn undeclare(&mut self, id: &CommandId) -> bool {
        let Some(owner) = self.reg.commands.remove(id) else {
            return false;
        };
        self.reg.keymap.remove_command(&owner, id);
        self.reg.menus.remove_command(&owner, id);
        self.rebuild();
        true
    }

    /// Loads a keymap document into the user's or the workspace's layer
    /// now, through the `keys.load` step: what changed, what was skipped,
    /// the conflicts after it.
    pub fn load_keymap(
        &mut self,
        doc: &egui_pop_model::keymap::KeymapDoc,
        layer: Layer,
    ) -> Result<egui_pop_model::keymap::KeymapDiff, String> {
        let owner = layer_owner(layer).ok_or("only the user's and the workspace's layers load")?;
        let handle = self.handle.clone();
        let keymap = &mut self.reg.keymap;
        let mut diff = None;
        let out = handle.step(
            &STEP_LOAD_KEYMAP,
            json!({ "layer": layer, "bindings": doc.bindings.len() }),
            |_, _| {
                let before = keymap.export(&owner, layer);
                let d = keymap.load(&owner, layer, doc);
                let mut diff_now = egui_pop_model::keymap::KeymapDiff::between(
                    &before,
                    &keymap.export(&owner, layer),
                );
                diff_now.skipped = d.all().iter().map(|x| x.message.clone()).collect();
                diff_now.conflicts = keymap
                    .conflicts()
                    .iter()
                    .map(|(l, w)| {
                        format!(
                            "{} ({}): {} loses to {}",
                            l.keys, l.options.scope, l.action, w.action
                        )
                    })
                    .collect();
                let summary = json!({
                    "added": diff_now.added.len(),
                    "removed": diff_now.removed.len(),
                    "changed": diff_now.changed.len(),
                    "skipped": diff_now.skipped,
                    "conflicts": diff_now.conflicts,
                });
                diff = Some(diff_now);
                Ok(summary)
            },
        );
        self.rebuild();
        match out {
            Outcome::Done(Ok(_)) => Ok(diff.unwrap_or_default()),
            Outcome::Done(Err(e)) => Err(e),
            Outcome::Vetoed { by, reason } => Err(format!("vetoed by {by}: {reason}")),
        }
    }

    /// The application closes: the `on_exit` functions, then the
    /// `app.exit` step, whose default writes what is still waiting
    /// (`App::save`). A vetoed exit writes nothing more: what changed was
    /// written already, a second after it changed.
    pub fn exit(&mut self) -> Outcome {
        let handle = self.handle.clone();
        for f in &mut self.on_exit {
            f(&handle);
        }
        let this: &App = self;
        handle.step(&STEP_EXIT, json!({}), |_, _| {
            this.save()
                .map(|()| json!({ "saved": true }))
                .map_err(|e| e.to_string())
        })
    }

    /// Draws the application, through the `wrap` given to the builder
    /// when there is one.
    pub fn show_wrapped(&mut self, ui: &mut Ui) {
        match self.wrap.take() {
            Some(mut wrap) => {
                let handle = self.handle.clone();
                wrap(ui, &handle, &mut |ui| self.show(ui));
                self.wrap = Some(wrap);
            }
            None => self.show(ui),
        }
    }

    fn install_theme(&mut self) {
        let service = self.handle.theme();
        let spec = service.spec();
        let ctx = self.handle.egui();
        let here = THEME_INSTALLED.get(&ctx).unwrap_or(false);
        if !here || self.installed.as_ref() != Some(&spec) {
            if !here {
                for kit in &self.kits {
                    kit.register(&ctx);
                }
            }
            theme::install_with(&ctx, service.themes(), &spec);
            THEME_INSTALLED.set(&ctx, true);
            self.installed = Some(spec);
            // The install forces the theme's own mode.
            self.preference = None;
            let paired = theme::is_paired(&ctx);
            self.handle
                .set_context("themePaired", if paired { "true" } else { "false" });
        }
        let preference = service.preference();
        if self.preference != Some(preference) {
            theme::set_preference(&ctx, preference);
            self.preference = Some(preference);
        }
    }

    /// Draws in this context from now on (an application built on one
    /// context and shown in another, as test harnesses do).
    fn attach(&mut self, ctx: &Context) {
        *self.handle.0.ctx.borrow_mut() = ctx.clone();
        let commands = self.handle.0.panel_commands.borrow().clone();
        if !strings::panel_commands(ctx).is_some_and(|c| Arc::ptr_eq(&c, &commands)) {
            strings::set_panel_commands(ctx, commands);
        }
        focus::install(ctx);
        // egui-pop's interface zoom replaces egui's keys (`zoom`).
        egui_pop_core::scale::disable_keyboard_zoom(ctx);
        let zoom = crate::zoom::current(&self.handle);
        if (ctx.zoom_factor() - zoom).abs() > 1e-3 {
            ctx.set_zoom_factor(zoom);
        }
        if self.waking.as_ref() != Some(ctx) {
            self.waking = Some(ctx.clone());
            let c = ctx.clone();
            if let Some(h) = self.handle.0.host.borrow_mut().as_mut() {
                h.set_waker(Waker::new(move || c.request_repaint()));
            }
        }
        if let Some(modes) = self.handle.service(&FOCUS_MODE) {
            let preference = lock(&self.handle.store())
                .get(&format!("settings/{SETTING_FOCUS}"))
                .and_then(|e| e.value.as_choice().and_then(Preference::parse))
                .unwrap_or_default();
            modes.set_preference(preference);
            focus_mode::set(ctx, &modes);
        }
    }

    /// Runs a command now, in the current context: its result, or a
    /// result to come (`Reply::Later`, kept until it arrives).
    pub fn run(&mut self, command: &CommandId, args: Json, origin: Origin) -> Reply {
        let cx = self.handle.when_context();
        self.run_in(command, args, origin, &cx)
    }

    /// Runs a command now, in a given context, through the host.
    pub fn run_in(
        &mut self,
        command: &CommandId,
        args: Json,
        origin: Origin,
        cx: &WhenContext,
    ) -> Reply {
        let handler = match self.reg.commands.prepare(command, cx) {
            Ok(h) => h.clone(),
            Err(e) => {
                let msg = e.to_string();
                if !matches!(e, egui_pop_model::commands::RunRefused::Unknown { .. }) {
                    self.record(command, &args, &origin, Err(msg.clone()));
                }
                return Reply::Done(Err(msg));
            }
        };
        let source = source_of(&origin);
        let _running = RunningCommand::new(&self.handle, command, self.reg.commands.owner(command));
        let seq = self.handle.publish_now(
            &source,
            None,
            "command.dispatched",
            json!({ "command": command, "args": args }),
        );
        let owner = self.reg.commands.owner(command).cloned();
        if IN_INTERFACE.contains(command) {
            let result = match &handler {
                Some(h) => {
                    let _scope = owner.as_ref().map(egui_pop_model::values::enter_scope);
                    h(&Call {
                        args: &args,
                        origin: &origin,
                        app: &self.handle,
                    })
                }
                None => Err(format!("no handler for `{command}`")),
            };
            self.finish(command, &args, &origin, seq, result.clone());
            return Reply::Done(result);
        }
        let mut host = self
            .handle
            .0
            .host
            .borrow_mut()
            .take()
            .expect("a command dispatched from inside a dispatch is queued (AppHandle::run)");
        let reply = host.dispatch(
            &Dispatch::new(
                command,
                &args,
                &origin,
                cx,
                seq,
                &self.handle,
                handler.as_ref(),
            )
            .owned_by(owner.as_ref()),
        );
        *self.handle.0.host.borrow_mut() = Some(host);
        match reply {
            Reply::Done(result) => {
                self.finish(command, &args, &origin, seq, result.clone());
                Reply::Done(result)
            }
            Reply::Later(pending) => {
                let waiting = {
                    let mut running = self.handle.0.running.borrow_mut();
                    let n = running.entry(command.clone()).or_default();
                    *n += 1;
                    *n
                };
                if waiting == 1 {
                    let now = egui_pop_core::motion::now(&self.handle.egui());
                    self.handle
                        .0
                        .running_since
                        .borrow_mut()
                        .insert(command.clone(), now);
                }
                self.in_flight.push(InFlight {
                    command: command.clone(),
                    args,
                    origin,
                    seq,
                    pending: pending.clone(),
                });
                Reply::Later(pending)
            }
        }
    }

    /// A result arrived: the history and the journal keep it.
    fn finish(
        &mut self,
        command: &CommandId,
        args: &Json,
        origin: &Origin,
        seq: Seq,
        result: Result<Json, String>,
    ) {
        let (name, data) = match &result {
            Ok(v) => ("command.done", json!({ "command": command, "result": v })),
            Err(e) => ("command.failed", json!({ "command": command, "error": e })),
        };
        self.handle
            .publish_now(&source_of(origin), Some(seq), name, data);
        self.record(command, args, origin, result);
    }

    /// Takes the results that arrived (each frame does it before
    /// drawing); returns how many.
    pub fn settle(&mut self) -> usize {
        profiling::scope!("pop.app.settle");
        let mut arrived = Vec::new();
        self.in_flight.retain(|f| match f.pending.take() {
            Some(r) => {
                arrived.push((
                    f.command.clone(),
                    f.args.clone(),
                    f.origin.clone(),
                    f.seq,
                    r,
                ));
                false
            }
            None => true,
        });
        let n = arrived.len();
        for (command, args, origin, seq, result) in arrived {
            if let Some(c) = self.handle.0.running.borrow_mut().get_mut(&command) {
                *c = c.saturating_sub(1);
                if *c == 0 {
                    self.handle.0.running_since.borrow_mut().remove(&command);
                }
            }
            if let Err(e) = &result {
                log::warn!(target: "egui_pop", "command `{command}`: {e}");
            }
            self.finish(&command, &args, &origin, seq, result);
        }
        n
    }

    fn record(
        &mut self,
        command: &CommandId,
        args: &Json,
        origin: &Origin,
        result: Result<Json, String>,
    ) {
        let run = Run {
            id: command.clone(),
            args: args.clone(),
            origin: origin.clone(),
            result,
        };
        self.reg.commands.record(run.clone());
        let mut h = self.handle.0.history.borrow_mut();
        h.push(run);
        let len = h.len();
        if len > 64 {
            h.drain(..len - 64);
        }
    }

    /// Does what blocks and handlers asked for.
    pub fn process_requests(&mut self) {
        profiling::scope!("pop.app.requests");
        loop {
            let requests: Vec<Request> = std::mem::take(&mut *self.handle.0.requests.borrow_mut());
            if requests.is_empty() {
                break;
            }
            for r in requests {
                match r {
                    Request::Run {
                        command,
                        args,
                        origin,
                    } => {
                        if let Reply::Done(Err(e)) = self.run(&command, args, origin) {
                            log::warn!(target: "egui_pop", "command `{command}`: {e}");
                        }
                    }
                    Request::RunIn {
                        command,
                        args,
                        origin,
                        context,
                    } => {
                        if let Reply::Done(Err(e)) = self.run_in(&command, args, origin, &context) {
                            log::warn!(target: "egui_pop", "command `{command}`: {e}");
                        }
                    }
                    Request::Enable(p, true) => {
                        if let Err(e) = self.connect(&p) {
                            log::warn!(target: "egui_pop", "plugin `{p}`: {e}");
                        }
                    }
                    Request::Enable(p, false) => {
                        self.disconnect(&p);
                    }
                    Request::Rebind {
                        command,
                        keys,
                        layer,
                    } => {
                        let handle = self.handle.clone();
                        let keymap = &mut self.reg.keymap;
                        let mut changed = false;
                        let out = handle.step(
                            &STEP_REBIND,
                            json!({ "command": command, "keys": keys, "layer": layer }),
                            |_, args| {
                                let command = args
                                    .get("command")
                                    .and_then(Json::as_str)
                                    .and_then(|c| CommandId::new(c).ok())
                                    .ok_or("`command` is missing")?;
                                let keys = args.get("keys").and_then(Json::as_str);
                                let layer: Layer = args
                                    .get("layer")
                                    .cloned()
                                    .map_or(Ok(Layer::User), serde_json::from_value)
                                    .map_err(|e| format!("`layer`: {e}"))?;
                                let owner = layer_owner(layer).ok_or(
                                    "keys change in the user's or the workspace's layer only",
                                )?;
                                keymap.rebind(&owner, layer, &command, keys)?;
                                changed = true;
                                Ok(json!({ "command": command, "keys": keys, "layer": layer }))
                            },
                        );
                        if changed {
                            self.dirty = true;
                        }
                        if let Outcome::Done(Err(e)) = out {
                            log::warn!(target: "egui_pop", "keys for `{command}`: {e}");
                        }
                    }
                    Request::Declare { owner, spec } => {
                        let id = spec.id.clone();
                        if let Err(e) = self.declare(&owner, spec) {
                            log::warn!(target: "egui_pop", "declaring `{id}`: {e}");
                        }
                    }
                    Request::Undeclare(id) => {
                        self.undeclare(&id);
                    }
                    Request::LoadKeymap { doc, layer } => {
                        if let Err(e) = self.load_keymap(&doc, layer) {
                            log::warn!(target: "egui_pop", "loading a keymap: {e}");
                        }
                    }
                    Request::ResetKeys { command, layer } => {
                        if let Some(owner) = layer_owner(layer) {
                            self.reg.keymap.reset(&owner, layer, &command);
                            self.dirty = true;
                        }
                    }
                    Request::Workbench(change) => {
                        let what = format!("{change:?}");
                        if let Some(Outcome::Done(Err(e))) = self.handle.change_workbench(change) {
                            self.handle.workbench_refused(&what, &e);
                        }
                    }
                }
            }
        }
        if self.dirty {
            self.rebuild();
        }
    }

    /// Between frames: the theme asked for, requests, declarations.
    pub fn logic(&mut self, ctx: &Context) {
        profiling::scope!("pop.app.logic");
        self.attach(ctx);
        self.install_theme();
        self.settle();
        self.handle.feed_indicators();
        self.emit(&Lifecycle::Pass);
        self.process_requests();
        self.autosave(ctx);
    }

    /// Draws the whole application in a `Ui` covering the window, after
    /// dispatching this frame's keys.
    pub fn show(&mut self, ui: &mut Ui) {
        profiling::scope!("pop.app.show");
        let ctx = ui.ctx().clone();
        // The style `ui` was made with: a theme installed or a light or
        // dark switch below comes after it.
        let before = ctx.global_style();
        self.logic(&ctx);
        if let Some((from, to)) = egui_pop_core::scale::changed(&ctx) {
            self.handle.publish(
                "egui_pop",
                None,
                "scale.changed",
                json!({ "viewport": "root", "from": from, "to": to }),
            );
            // Strokes are snapped for the scale: install the theme again.
            self.installed = None;
            self.install_theme();
        }
        // Drawn with the theme of this pass, as `Kit::of` and
        // `Tokens::of` are: else this pass mixes the variants, and what a
        // cache keeps from it stays drawn in the other one. A style the
        // host set on `ui` itself is kept.
        let now = ctx.global_style();
        if !Arc::ptr_eq(&before, &now) && Arc::ptr_eq(ui.style(), &before) {
            ui.set_style(now);
        }
        overlay::begin_pass(&ctx);
        let cx = self.handle.when_context();
        let dispatched = if self.handle.0.capturing.get() {
            Vec::new()
        } else {
            self.dispatcher.process(&ctx, &self.reg.keymap, &cx)
        };
        *self.handle.0.traces.borrow_mut() = self.dispatcher.traces().cloned().collect();
        *self.handle.0.pending_keys.borrow_mut() = self.dispatcher.pending().map(|k| k.to_string());
        for d in dispatched {
            if let Reply::Done(Err(e)) = self.run(&d.command, Json::Null, Origin::User) {
                log::info!(target: "egui_pop", "{}: {e}", d.keys);
            }
        }
        self.process_requests();
        self.outlined(ui);
        let name = self.handle.name();
        let name = if name.is_empty() {
            ctx.input(|i| i.viewport().title.clone())
                .unwrap_or_default()
        } else {
            name
        };
        crate::workbench::windows(&self.handle.clone(), &ctx, &name);
        self.handle.clone().show_zone(ui, &zones::OVERLAYS);
        crate::menus::draw_popup(&ctx, &self.handle);
        a11y::live_regions(ui);
        // Interactive indicators bound to a command, activated this frame.
        for command in egui_pop_core::indicator::take_commands(&ctx) {
            self.handle.run(command, Json::Null);
        }
        self.process_requests();
    }

    /// The shell, inside the kit's outline when the application draws
    /// the window (`window.rest.border-width`).
    fn outlined(&mut self, ui: &mut Ui) {
        let outline = if crate::frame::own_frame(&self.handle) {
            egui_pop_core::frame::outline(ui.ctx())
        } else {
            egui::Stroke::NONE
        };
        if outline.width <= 0.0 {
            return self.shell(ui);
        }
        let rect = ui.max_rect();
        ui.ctx()
            .layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                Id::new("egui_pop.window_outline"),
            ))
            .rect_stroke(rect, 0.0, outline, egui::StrokeKind::Inside);
        ui.scope_builder(
            egui::UiBuilder::new().max_rect(rect.shrink(outline.width)),
            |ui| self.shell(ui),
        );
    }

    fn shell(&mut self, ui: &mut Ui) {
        let h = self.handle.clone();
        h.follow_zen();
        if h.zen() {
            // Zen mode: the centre alone, and no caption published.
            egui_pop_core::frame::hide_title_bar(ui.ctx());
            crate::workbench::show(&h, ui);
            return;
        }
        if h.zone_has_blocks(&zones::TITLE) {
            // No least height: a title zone whose blocks draw nothing (the
            // system draws the frame) takes no room.
            egui::Panel::top(Id::new("egui_pop.title"))
                .frame(egui::Frame::NONE)
                // The kit's `title-bar` draws the line under the bar.
                .show_separator_line(false)
                .min_size(0.0)
                .show(ui, |ui| {
                    focus::scope(ui, "titleBar", &[], |ui| h.show_zone(ui, &zones::TITLE));
                });
        }
        if h.menu_bar() {
            egui::Panel::top(Id::new("egui_pop.top")).show(ui, |ui| {
                focus::scope(ui, "topBar", &[], |ui| {
                    egui::MenuBar::new().ui(ui, |ui| {
                        // The top bar's blocks keep the width they took
                        // last pass; the menus overflow before them.
                        let reserve = TOP_BAR_RESERVE.get(ui.ctx()).unwrap_or(0.0);
                        crate::menus::bar_within(ui, &h, &MENU_MAIN, reserve);
                        let mut used = 0.0;
                        if h.zone_has_blocks(&zones::TOP_BAR) {
                            // The bar takes its whole width: the zone is
                            // measured in its own scope.
                            let start = ui.cursor().min.x;
                            ui.separator();
                            let zone = ui.scope(|ui| h.show_zone(ui, &zones::TOP_BAR));
                            used = zone.response.rect.max.x - start + ui.spacing().item_spacing.x;
                        }
                        if used != reserve {
                            TOP_BAR_RESERVE.set(ui.ctx(), used);
                            // Laid out again at once when a menu moves.
                            if crate::menus::refit_needed(&h, &MENU_MAIN, used) {
                                ui.ctx().request_repaint();
                            }
                        }
                    });
                });
            });
        } else if h.zone_has_blocks(&zones::TOP_BAR) {
            egui::Panel::top(Id::new("egui_pop.top")).show(ui, |ui| {
                focus::scope(ui, "topBar", &[], |ui| {
                    ui.horizontal(|ui| h.show_zone(ui, &zones::TOP_BAR));
                });
            });
        }
        if h.zone_has_blocks(&zones::STATUS_BAR) {
            egui::Panel::bottom(Id::new("egui_pop.status")).show(ui, |ui| {
                focus::scope(ui, "statusBar", &[], |ui| {
                    ui.horizontal(|ui| h.show_zone(ui, &zones::STATUS_BAR));
                });
            });
        }
        crate::workbench::show(&h, ui);
    }
}

/// Diagnostic codes recomputed by each rebuild.
const RESOLUTION_CODES: &[&str] = &[
    "unknown-zone",
    "duplicate-block",
    "dangling-replace",
    "dangling-extend",
    "dangling-wrap",
    "dangling-after",
    "hook-order",
    "unknown-command",
    "unknown-step",
    "step-replaced-twice",
];

#[cfg(feature = "eframe")]
impl eframe::App for App {
    fn logic(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        App::logic(self, ctx);
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.show_wrapped(ui);
    }

    fn on_exit(&mut self) {
        match self.exit() {
            Outcome::Done(Err(e)) => log::error!(target: "egui_pop", "saving: {e}"),
            Outcome::Vetoed { by, reason } => {
                log::warn!(target: "egui_pop", "exit step vetoed by {by}: {reason}");
            }
            Outcome::Done(Ok(_)) => {}
        }
    }
}

#[cfg(test)]
mod make_tests {
    use super::*;

    #[test]
    fn a_failed_make_is_logged_and_published() {
        let app = App::builder().build(&Context::default()).unwrap();
        let h = app.handle().clone();
        let mark = h.last_event();
        let e: MakeError = "the kernel did not start".into();
        report_make_error(&h, e.as_ref());
        let events = h.events_since(mark);
        let ev = events.last().expect("published");
        assert_eq!(ev.name, "app.make_failed");
        assert_eq!(ev.data["error"], "the kernel did not start");
    }

    /// `make` may fail; it compiles with an error of any kind.
    #[cfg(all(feature = "eframe", not(target_arch = "wasm32")))]
    #[allow(dead_code)]
    fn fallible_make(b: AppBuilder) -> eframe::Result {
        b.run_native_with("demo", eframe::NativeOptions::default(), |app, _| {
            if app.handle().panels().is_empty() {
                return Err("nothing to show".into());
            }
            Ok(Box::new(app))
        })
    }
}
