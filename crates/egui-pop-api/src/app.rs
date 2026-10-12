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
use egui_pop_core::overlay;
use egui_pop_core::persist::Persist;
use egui_pop_core::theme::{self, ThemeSpec, Themes};
use egui_pop_core::{a11y, fault, focus, lock};
use egui_pop_model::commands::{CommandSpec, Commands, Origin, Run};
use egui_pop_model::diagnostics::{Diagnostic, Diagnostics};
use egui_pop_model::ids::{
    CommandId, MenuId, OwnerId, PanelId, PluginId, SettingId, StepId, StoryId, ZoneId,
};
use egui_pop_model::keymap::{Binding, Keymap, Layer, Trace};
use egui_pop_model::menus::{Item, MenuItem, Menus, sort_items};
use egui_pop_model::order::{Node, order};
use egui_pop_model::panels::{Area, Panels};
use egui_pop_model::registry::{OwnedList, Registry};
use egui_pop_model::values::{Store, settings_schema};
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

/// The main menu bar.
pub const MENU_MAIN: MenuId = MenuId::from_static("app.main");
pub const MENU_FILE: MenuId = MenuId::from_static("app.menu.file");
pub const MENU_EDIT: MenuId = MenuId::from_static("app.menu.edit");
pub const MENU_VIEW: MenuId = MenuId::from_static("app.menu.view");
pub const MENU_HELP: MenuId = MenuId::from_static("app.menu.help");

/// The framework's commands.
pub use crate::palette::CMD_PALETTE;
pub const CMD_QUIT: CommandId = CommandId::from_static("app.quit");
pub const CMD_UNDO: CommandId = CommandId::from_static("value.undo");
pub const CMD_REDO: CommandId = CommandId::from_static("value.redo");
pub const CMD_SAVE: CommandId = CommandId::from_static("app.save");

/// The theme, as a service: read it, ask for another (installed between
/// frames).
#[derive(Debug)]
pub struct ThemeService {
    spec: Mutex<ThemeSpec>,
    themes: Themes,
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
}

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

/// Declarations derived from the registries, after the alter hooks.
#[derive(Clone, Debug, Default)]
pub(crate) struct Derived {
    pub menus: BTreeMap<MenuId, Vec<MenuItem>>,
    pub palette: Vec<PaletteEntry>,
    pub settings: Vec<SettingEntry>,
    pub commands: Vec<(CommandSpec, OwnerId)>,
    /// Each command's live key bindings, as shown.
    pub keys: BTreeMap<CommandId, Vec<String>>,
    /// Bindings losing to another on the same keys: loser, winner.
    pub conflicts: Vec<(Binding, Binding)>,
    /// Commands whose keys the user changed.
    pub rebound: BTreeSet<CommandId>,
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
    /// Gives a command the user's own keys (none: no key).
    Rebind {
        command: CommandId,
        keys: Option<String>,
    },
    /// Gives a command back its default keys.
    ResetKeys(CommandId),
}

struct PanelView {
    owner: OwnerId,
    draw: Option<DrawFn>,
}

struct StoryEntry {
    story: Story,
}

#[derive(Default)]
struct UiState {
    hidden_panels: BTreeSet<PanelId>,
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
    ui: RefCell<UiState>,
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
    /// Commands whose result is to come, by id: how many.
    running: RefCell<BTreeMap<CommandId, usize>>,
    /// Collectors added by plugins.
    collectors: RefCell<Vec<(OwnerId, String)>>,
    /// A shortcuts editor waits for keys: the dispatcher stands aside.
    capturing: std::cell::Cell<bool>,
    /// Declared steps (ADR 0011), their hooks, their last runs.
    steps: RefCell<Registry<StepId, StepSpec>>,
    step_hooks: RefCell<OwnedList<StepHook>>,
    step_traces: RefCell<std::collections::VecDeque<StepTrace>>,
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

impl AppHandle {
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
            ui: RefCell::new(UiState::default()),
            diagnostics: RefCell::new(Diagnostics::new()),
            derived: RefCell::new(Derived::default()),
            plugins: RefCell::new(Vec::new()),
            history: RefCell::new(Vec::new()),
            bridge: RefCell::new(DebugBridge::with_core_collectors()),
            traces: RefCell::new(Vec::new()),
            pending_keys: RefCell::new(None),
            host: RefCell::new(Some(host)),
            held: RefCell::new(Vec::new()),
            running: RefCell::new(BTreeMap::new()),
            collectors: RefCell::new(Vec::new()),
            capturing: std::cell::Cell::new(false),
            steps: RefCell::new(Registry::new()),
            step_hooks: RefCell::new(OwnedList::new()),
            step_traces: RefCell::new(std::collections::VecDeque::new()),
        }))
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

    /// The value store.
    pub fn store(&self) -> SharedStore {
        self.0.store.clone()
    }

    /// Appends an event to the host's journal (held until the host is
    /// free when published from a command it is running).
    pub fn publish(&self, source: &str, cause: Option<Seq>, name: &str, data: Json) {
        match self.0.host.borrow_mut().as_mut() {
            Some(h) => {
                h.publish(source, cause, name, data);
            }
            None => self
                .0
                .held
                .borrow_mut()
                .push((source.into(), cause, name.into(), data)),
        }
    }

    /// The journal's events after `seq` (empty while the host is
    /// dispatching).
    pub fn events_since(&self, seq: Seq) -> Vec<Event> {
        self.0
            .host
            .borrow()
            .as_ref()
            .map(|h| h.since(seq))
            .unwrap_or_default()
    }

    /// The journal's last sequence number.
    pub fn last_event(&self) -> Seq {
        self.0
            .host
            .borrow()
            .as_ref()
            .map_or(0, |h| h.journal().last())
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

    /// Whether a run of this command waits for its result.
    pub fn running(&self, id: &CommandId) -> bool {
        self.0.running.borrow().get(id).is_some_and(|n| *n > 0)
    }

    fn publish_now(&self, source: &str, cause: Option<Seq>, name: &str, data: Json) -> Seq {
        let mut host = self.0.host.borrow_mut();
        let h = host.as_mut().expect("the host is free outside a dispatch");
        for (s, c, n, d) in self.0.held.borrow_mut().drain(..) {
            h.publish(&s, c, &n, d);
        }
        h.publish(source, cause, name, data)
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
    /// clauses.
    pub fn set_context(&self, key: &str, value: &str) {
        self.0.context.borrow_mut().set(key, value);
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
        focus::when_context(ctx, &base)
    }

    /// The diagnostics collected (registration, zones, hooks).
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        self.0.diagnostics.borrow().all().to_vec()
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
        self.push_request(Request::Rebind { command, keys });
    }

    /// Asks for a command's default keys back.
    pub fn reset_keys(&self, command: CommandId) {
        self.push_request(Request::ResetKeys(command));
    }

    /// While on, key presses reach the interface instead of the key
    /// dispatcher (a shortcuts editor capturing new keys).
    pub fn capture_keys(&self, on: bool) {
        self.0.capturing.set(on);
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

    fn show_panel(&self, ui: &mut Ui, id: &PanelId, title: &str) {
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

    /// Whether the command palette is open.
    /// Whether a panel is shown.
    pub fn panel_visible(&self, id: &PanelId) -> bool {
        !self.0.ui.borrow().hidden_panels.contains(id)
    }

    /// Shows or hides a panel.
    pub fn set_panel_visible(&self, id: &PanelId, visible: bool) {
        let mut u = self.0.ui.borrow_mut();
        if visible {
            u.hidden_panels.remove(id);
        } else {
            u.hidden_panels.insert(id.clone());
        }
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
}

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
            host: None,
            system_focus: None,
        }
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
        let modes = FocusModes::new(self.system_focus.unwrap_or_default());
        let mut app = App {
            handle,
            plugins: slots,
            reg: Registries::new(),
            dispatcher: KeyDispatcher::new(),
            dirty: true,
            started: false,
            installed: None,
            persist: self.persist,
            in_flight: Vec::new(),
            waking: None,
        };
        app.framework(self.theme, self.themes, modes)
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
        // The window's place from the last run, repaired (the screens are
        // not known before the window exists: only bounds are checked).
        if let Some(p) = &self.persist
            && let Ok(Some(read)) =
                p.load::<egui_pop_model::values::SettingsDoc>("settings", &settings_schema())
            && let Some(state) = crate::window::from_settings(&read.value)
        {
            crate::window::apply(&mut options, state.repair(&[]));
        }
        eframe::run_native(
            title,
            options,
            Box::new(move |cc| {
                let app = self
                    .build(&cc.egui_ctx)
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
                Ok(Box::new(app))
            }),
        )
    }
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
    persist: Option<Persist>,
    /// Results to come.
    in_flight: Vec<InFlight>,
    /// The context the host's waker wakes.
    waking: Option<Context>,
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
        modes: FocusModes,
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
        for (zone, title) in zones::SHELL.iter().zip([
            "Top bar",
            "Side bar",
            "Main",
            "Inspector",
            "Status bar",
            "Overlays",
        ]) {
            ctx.zone(zone.clone(), title)?;
        }
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
        ctx.service(&FOCUS_MODE, modes)?;
        ctx.command(
            CommandSpec::new(CMD_UNDO, "Undo", "Edit")
                .description("Undo the last change of values.")
                .key("Ctrl+Z")
                .when("!textInputFocus")
                .menu(MENU_EDIT, "0_undo", 0),
            |call| {
                let undone = lock(&call.app.store())
                    .undo(call.origin)
                    .map_err(|r| r.to_string())?;
                Ok(json!({ "undone": undone }))
            },
        )?;
        ctx.command(
            CommandSpec::new(CMD_REDO, "Redo", "Edit")
                .description("Redo the last undone change of values.")
                .key("Ctrl+Shift+Z")
                .key("Ctrl+Y")
                .when("!textInputFocus")
                .menu(MENU_EDIT, "0_undo", 1),
            |call| {
                let redone = lock(&call.app.store())
                    .redo(call.origin)
                    .map_err(|r| r.to_string())?;
                Ok(json!({ "redone": redone }))
            },
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
        b.register(egui_pop_core::debug::store_collector(h.store()))
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
        derived.conflicts = reg.keymap.conflicts();
        derived.rebound = reg
            .commands
            .specs()
            .filter(|s| reg.keymap.rebound(&OwnerId::USER, Layer::User, &s.id))
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

    /// Saves the settings and the user's keymap (when a directory was
    /// given).
    pub fn save(&self) -> Result<(), egui_pop_core::persist::PersistError> {
        let Some(p) = &self.persist else {
            return Ok(());
        };
        let doc = lock(&self.handle.0.store).export("settings/");
        p.save("settings", &settings_schema(), &doc)?;
        let keymap = self.reg.keymap.export(&OwnerId::USER, Layer::User);
        p.save("keymap", &egui_pop_model::keymap_schema(), &keymap)
    }

    fn install_theme(&mut self) {
        let service = self.handle.theme();
        let spec = service.spec();
        let ctx = self.handle.egui();
        let marker = Id::new("egui_pop_api::theme_installed");
        let here = ctx.data(|d| d.get_temp::<bool>(marker)).unwrap_or(false);
        if !here || self.installed.as_ref() != Some(&spec) {
            theme::install_with(&ctx, service.themes(), &spec);
            ctx.data_mut(|d| d.insert_temp(marker, true));
            self.installed = Some(spec);
        }
    }

    /// Draws in this context from now on (an application built on one
    /// context and shown in another, as test harnesses do).
    fn attach(&mut self, ctx: &Context) {
        *self.handle.0.ctx.borrow_mut() = ctx.clone();
        focus::install(ctx);
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
        let seq = self.handle.publish_now(
            &source,
            None,
            "command.dispatched",
            json!({ "command": command, "args": args }),
        );
        let mut host = self
            .handle
            .0
            .host
            .borrow_mut()
            .take()
            .expect("a command dispatched from inside a dispatch is queued (AppHandle::run)");
        let reply = host.dispatch(&Dispatch::new(
            command,
            &args,
            &origin,
            cx,
            seq,
            &self.handle,
            handler.as_ref(),
        ));
        *self.handle.0.host.borrow_mut() = Some(host);
        match reply {
            Reply::Done(result) => {
                self.finish(command, &args, &origin, seq, result.clone());
                Reply::Done(result)
            }
            Reply::Later(pending) => {
                *self
                    .handle
                    .0
                    .running
                    .borrow_mut()
                    .entry(command.clone())
                    .or_default() += 1;
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
                    Request::Rebind { command, keys } => {
                        match self.reg.keymap.rebind(
                            &OwnerId::USER,
                            Layer::User,
                            &command,
                            keys.as_deref(),
                        ) {
                            Ok(()) => self.dirty = true,
                            Err(e) => {
                                log::warn!(target: "egui_pop", "keys for `{command}`: {e}");
                            }
                        }
                    }
                    Request::ResetKeys(command) => {
                        self.reg.keymap.reset(&OwnerId::USER, Layer::User, &command);
                        self.dirty = true;
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
        self.attach(ctx);
        self.install_theme();
        self.settle();
        self.process_requests();
    }

    /// Draws the whole application in a `Ui` covering the window, after
    /// dispatching this frame's keys.
    pub fn show(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        self.logic(&ctx);
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
        self.shell(ui);
        self.handle.clone().show_zone(ui, &zones::OVERLAYS);
        a11y::live_regions(ui);
        self.process_requests();
    }

    fn shell(&mut self, ui: &mut Ui) {
        let h = self.handle.clone();
        let panels: Vec<(PanelId, String, Area)> = self
            .reg
            .panels
            .all()
            .into_iter()
            .filter(|p| h.panel_visible(&p.id))
            .map(|p| (p.id.clone(), p.title.clone(), p.area))
            .collect();
        let in_area = |a: Area| {
            panels
                .iter()
                .filter(|p| p.2 == a)
                .cloned()
                .collect::<Vec<_>>()
        };
        egui::Panel::top(Id::new("egui_pop.top")).show(ui, |ui| {
            focus::scope(ui, "topBar", &[], |ui| {
                egui::MenuBar::new().ui(ui, |ui| {
                    crate::menus::bar(ui, &h, &MENU_MAIN);
                    if h.zone_has_blocks(&zones::TOP_BAR) {
                        ui.separator();
                        h.show_zone(ui, &zones::TOP_BAR);
                    }
                });
            });
        });
        if h.zone_has_blocks(&zones::STATUS_BAR) {
            egui::Panel::bottom(Id::new("egui_pop.status")).show(ui, |ui| {
                focus::scope(ui, "statusBar", &[], |ui| {
                    ui.horizontal(|ui| h.show_zone(ui, &zones::STATUS_BAR));
                });
            });
        }
        let bottom = in_area(Area::Bottom);
        if !bottom.is_empty() {
            egui::Panel::bottom(Id::new("egui_pop.bottom"))
                .resizable(true)
                .show(ui, |ui| self.panel_tabs(ui, "bottom", &bottom));
        }
        let start = in_area(Area::StartSide);
        if h.zone_has_blocks(&zones::SIDE_BAR) || !start.is_empty() {
            egui::Panel::left(Id::new("egui_pop.side"))
                .resizable(false)
                .exact_size(170.0)
                .show(ui, |ui| {
                    focus::scope(ui, "sideBar", &[], |ui| h.show_zone(ui, &zones::SIDE_BAR));
                    self.panel_tabs(ui, "start", &start);
                });
        }
        let end = in_area(Area::EndSide);
        if h.zone_has_blocks(&zones::INSPECTOR) || !end.is_empty() {
            egui::Panel::right(Id::new("egui_pop.inspector"))
                .resizable(false)
                .exact_size(240.0)
                .show(ui, |ui| {
                    focus::scope(ui, "inspector", &[], |ui| {
                        h.show_zone(ui, &zones::INSPECTOR)
                    });
                    self.panel_tabs(ui, "end", &end);
                });
        }
        let center = in_area(Area::Center);
        egui::CentralPanel::default().show(ui, |ui| {
            if !center.is_empty() {
                self.panel_tabs(ui, "center", &center);
            }
            focus::scope(ui, "main", &[], |ui| h.show_zone(ui, &zones::MAIN));
        });
    }

    fn panel_tabs(&self, ui: &mut Ui, area: &str, panels: &[(PanelId, String, Area)]) {
        if panels.is_empty() {
            return;
        }
        let key = Id::new(("egui_pop.tab", area));
        let mut active: usize = ui.ctx().data(|d| d.get_temp(key)).unwrap_or(0);
        if active >= panels.len() {
            active = 0;
        }
        if panels.len() > 1 {
            ui.horizontal(|ui| {
                for (i, (_, title, _)) in panels.iter().enumerate() {
                    if ui.selectable_label(i == active, title).clicked() {
                        active = i;
                    }
                }
            });
            ui.separator();
        } else {
            ui.strong(&panels[0].1);
        }
        ui.ctx().data_mut(|d| d.insert_temp(key, active));
        let (id, title, _) = &panels[active];
        self.handle.show_panel(ui, id, title);
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
        self.show(ui);
    }

    fn on_exit(&mut self) {
        if let Err(e) = App::save(self) {
            log::error!(target: "egui_pop", "saving: {e}");
        }
    }
}
