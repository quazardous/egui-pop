//! The context stack (ADR 0012): places (drill-downs, shown as a
//! breadcrumb) and modes (global states, shown as chips), owned by the
//! application's kernel and projected by the interface.
//!
//! - **The owner** implements [`Contexts`]: the frames, the state, the
//!   transitions (through the pure `egui_pop_model::context::apply`), the
//!   holds. The host gives it ([`Host::contexts`](crate::host::Host::contexts));
//!   without one, [`ContextsPlugin`] keeps a [`LocalContexts`], which
//!   writes the stack to the `context` document inside each transition.
//!   The owner publishes `context.changed` (with the whole new state and
//!   its revision), then `context.left` and `context.entered` per frame,
//!   or `context.refused`; `context.restored` at start, `context.held`
//!   and `context.released`.
//! - **The interface asks, never writes.** Every transition is the step
//!   `context.request` (ADR 0011): a before-hook vetoes it (unsaved typed
//!   input) or redirects it, its default asks the owner. Nothing is shown
//!   until the owner's state has a new revision, so a refusal leaves the
//!   display as it was.
//! - **The projection** follows the owner's state: per projection key
//!   (the place path and the active modes), the workbench's layout (a
//!   place's arrangement the first time, a mode's panels shown or hidden;
//!   a change made in a mode belongs to it and goes when it is left), the
//!   focused panel and the values components keep ([`view_value`],
//!   [`scroll_area`]); the context keys `place`, `placeDepth`,
//!   `modePushed`, `mode.<group>` and the frames' own keys, read by
//!   `when` clauses (a frame's own key bindings use
//!   `FrameSpec::clause`: [`Ctx::frame_keybinding`]). It is written to
//!   the `view` document about a second after it changes.
//! - **Widgets**: [`breadcrumb`], [`mode_chips`], [`mode_switch`],
//!   [`mode_toggle`]. **Commands**: `nav.back` (Alt+Left; Escape while a
//!   mode is pushed), `nav.up` (Alt+Shift+Up), `nav.enter`, and for
//!   agents `context.read`, `context.request` (with `if_rev` and
//!   `dry_run`), `context.go`, `context.set_mode`, `context.hold` and
//!   `context.release`. The `contexts` collector shows it all.
//! - **Undo out of sight**: an undo or a redo of a change made in
//!   another place says so in a toast, with "Go there".
//!
//! ```
//! use egui_pop_api::contexts::{self, ContextsPlugin, FrameSpec};
//! use egui_pop_api::{App, Ctx, Plugin, PluginError};
//! use egui_pop_model::ids::{FrameId, GroupId, PluginId};
//!
//! const ALBUM: FrameId = FrameId::from_static("demo.album");
//! const CULLING: FrameId = FrameId::from_static("demo.culling");
//!
//! struct Demo;
//! impl Plugin for Demo {
//!     fn id(&self) -> PluginId { PluginId::from_static("demo") }
//!     fn requires(&self) -> Vec<PluginId> { vec![contexts::PLUGIN] }
//!     fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
//!         ctx.frame(FrameSpec::place(ALBUM, "Album"))?
//!             .frame(FrameSpec::mode(CULLING, GroupId::from_static("orientation"), "Culling"))?;
//!         Ok(())
//!     }
//! }
//!
//! let app = App::builder().plugin(ContextsPlugin::default()).plugin(Demo)
//!     .build(&egui::Context::default()).unwrap();
//! let h = app.handle();
//! assert!(contexts::enter(h, ALBUM, serde_json::Value::Null).ok());
//! assert!(contexts::set_mode(h, CULLING).ok());
//! assert_eq!(h.context().get("place"), "demo.album");
//! assert_eq!(h.context().get("mode.orientation"), "demo.culling");
//! assert!(contexts::back(h).vetoed().is_none());
//! assert_eq!(h.context().get("place"), "root");
//! ```

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use egui::{Ui, WidgetType};
use egui_pop_core::a11y::{self, Politeness};
use egui_pop_core::debug::FnCollector;
use egui_pop_core::lock;
use egui_pop_core::persist::Persist;
use egui_pop_core::state::Slot;
use egui_pop_core::strings;
use egui_pop_core::widgets;

/// The view generation a scroll area last applied its saved offset for.
const SCROLL_APPLIED: Slot<u64> = Slot::new("api.contexts.scroll_applied");
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::context::{self as model, ContextDoc, ViewDoc, ViewState};
use egui_pop_model::ids::{CommandId, OwnerId, PanelId, PluginId, StepId};
use egui_pop_model::keymap::Layer;
use egui_pop_model::layout::Layout;
use serde_json::{Value as Json, json};

pub use egui_pop_model::context::{
    Dropped, Entry, Envelope, FrameRegistry, FrameSpec, Hold, Kind, LabelCall, Life, ModeEntry,
    PLACES, ROOT, Refusal, RefusalKind, Request, State, Transition, Verdict,
};
pub use egui_pop_model::ids::{FrameId, GroupId};

use crate::app::{AppHandle, STEP_REDO, STEP_UNDO};
use crate::hooks::{Lifecycle, On};
use crate::host::{Pending, source_of};
use crate::services::ServiceToken;
use crate::steps::{Outcome, StepSpec};
use crate::strings as txt;
use crate::{Ctx, CtxError, Plugin, PluginError};

/// The plugin's id: plugins declaring frames require it.
pub const PLUGIN: PluginId = PluginId::from_static("egui_pop.contexts");

/// The context stack's service.
pub const CONTEXTS: ServiceToken<ContextService> = ServiceToken::new("app.contexts");

/// A transition asked of the owner (`{ "request": envelope, "origin" }`):
/// a before-hook vetoes it (the interface's own guards) or redirects it;
/// the default asks the owner. Its result: the new revision, the frames
/// entered and left; a refusal is its error.
pub const STEP_REQUEST: StepId = StepId::from_static("context.request");

/// Back: pops the last pushed mode, else leaves the top place.
pub const CMD_BACK: CommandId = CommandId::from_static("nav.back");
/// Up: leaves the top place.
pub const CMD_UP: CommandId = CommandId::from_static("nav.up");
/// Enters a place (`frame`, `payload`).
pub const CMD_ENTER: CommandId = CommandId::from_static("nav.enter");
/// The stack, its holds and its frames.
pub const CMD_READ: CommandId = CommandId::from_static("context.read");
/// Any transition, as an envelope (`request`, `if_rev`, `dry_run`).
pub const CMD_REQUEST: CommandId = CommandId::from_static("context.request");
/// Goes to a place path (`places`: `[{ "frame", "payload" }]`).
pub const CMD_GO: CommandId = CommandId::from_static("context.go");
/// Sets a mode (`mode`), pushes it (`push: true`), or turns a group off
/// (`group`, no `mode`).
pub const CMD_SET_MODE: CommandId = CommandId::from_static("context.set_mode");
/// Holds groups of the stack (`groups`, `reason`): requests touching
/// them are refused until released.
pub const CMD_HOLD: CommandId = CommandId::from_static("context.hold");
/// Releases a hold (`token`).
pub const CMD_RELEASE: CommandId = CommandId::from_static("context.release");

/// The document `LocalContexts` writes the stack to, beside the
/// layout's (`workspaces/photos/context` when the host keeps the layout
/// in `workspaces/photos/layout`; see [`documents`]).
pub const CONTEXT_DOCUMENT: &str = "context";
/// The document the projection writes its layouts and view states to,
/// beside the layout's too.
pub const VIEW_DOCUMENT: &str = "view";

const NO_STACK: &str = "no context stack: the ContextsPlugin is not connected";

/// What the owner answers a request.
#[derive(Debug)]
pub enum Answer {
    /// Applied (or, for a dry run, would be), or refused.
    Done(Result<Transition, Refusal>),
    /// A remote owner answers later; its new state comes with a new
    /// revision.
    Later(Pending),
}

/// The owner of the context stack, as the interface reaches it.
pub trait Contexts {
    /// The frames that exist.
    fn registry(&self) -> FrameRegistry;

    /// The stack now.
    fn state(&self) -> State;

    /// Its revision (the projection follows it).
    fn rev(&self) -> u64 {
        self.state().rev
    }

    /// The holds in force.
    fn holds(&self) -> Vec<Hold> {
        Vec::new()
    }

    /// Applies a transition, or refuses it. The owner writes the new
    /// stack before it answers, then publishes (see [`Contexts::take_events`]).
    fn request(&mut self, envelope: &Envelope, origin: &Origin) -> Answer;

    /// Holds groups (empty: everything; [`PLACES`] for places): its
    /// token.
    fn hold(&mut self, _groups: Vec<GroupId>, _reason: &str, _by: &str) -> Result<u64, String> {
        Err("this owner takes no holds".into())
    }

    /// Releases a hold; whether it was held.
    fn release(&mut self, _token: u64) -> bool {
        false
    }

    /// A plugin declares a frame (`Ctx::frame`).
    fn declare(&mut self, _owner: &OwnerId, _spec: FrameSpec) -> Result<(), String> {
        Err("this host declares its own frames".into())
    }

    /// A plugin is gone: its frames go.
    fn remove_owner(&mut self, _owner: &OwnerId) {}

    /// The events to publish since the last call, as (name, data): egui-pop
    /// publishes them on the host's journal. An owner publishing on its
    /// own bus gives none.
    fn take_events(&mut self) -> Vec<(String, Json)> {
        Vec::new()
    }
}

/// The default owner, in process: frames declared by plugins, the stack
/// written to a document inside each transition when given one.
#[derive(Debug, Default)]
pub struct LocalContexts {
    registry: FrameRegistry,
    owners: BTreeMap<FrameId, OwnerId>,
    state: State,
    holds: Vec<Hold>,
    next_hold: u64,
    events: Vec<(String, Json)>,
    persist: Option<(Persist, String)>,
}

/// Decides whether a saved frame may come back (its payload still
/// exists): `None` when it may, else why not.
pub type Validator = Arc<dyn Fn(&FrameId, &Json) -> Option<String> + Send + Sync>;

impl LocalContexts {
    pub fn new() -> Self {
        Self::default()
    }

    /// Writes the stack to `document` under `persist`, inside each
    /// transition (before answering and publishing).
    pub fn with_persist(mut self, persist: Persist, document: &str) -> Self {
        self.persist = Some((persist, document.into()));
        self
    }

    /// Where it writes (`None`: nowhere).
    pub fn set_persist(&mut self, persist: Option<Persist>, document: &str) {
        self.persist = persist.map(|p| (p, document.to_string()));
    }

    fn write(&self, state: &State) -> Result<(), String> {
        let Some((p, document)) = &self.persist else {
            return Ok(());
        };
        p.save(
            document,
            &model::schema(),
            &ContextDoc {
                state: state.clone(),
            },
        )
        .map_err(|e| e.to_string())
    }

    /// Loads the saved stack and brings it back against today's frames
    /// (`egui_pop_model::context::restore`); publishes
    /// `context.restored`. What could not come back, and why.
    pub fn restore(
        &mut self,
        valid: &dyn Fn(&FrameId, &Json) -> Option<String>,
    ) -> Result<Vec<Dropped>, String> {
        let Some((p, document)) = &self.persist else {
            return Ok(Vec::new());
        };
        let read = p
            .load::<ContextDoc>(document, &model::schema())
            .map_err(|e| e.to_string())?;
        let Some(read) = read else {
            return Ok(Vec::new());
        };
        let (state, dropped) = model::restore(&read.value.state, &self.registry, valid);
        if !dropped.is_empty() {
            self.write(&state)?;
        }
        self.state = state;
        self.events.push((
            "context.restored".into(),
            json!({ "rev": self.state.rev, "state": self.state, "dropped": dropped }),
        ));
        Ok(dropped)
    }
}

impl Contexts for LocalContexts {
    fn registry(&self) -> FrameRegistry {
        self.registry.clone()
    }

    fn state(&self) -> State {
        self.state.clone()
    }

    fn rev(&self) -> u64 {
        self.state.rev
    }

    fn holds(&self) -> Vec<Hold> {
        self.holds.clone()
    }

    fn request(&mut self, envelope: &Envelope, origin: &Origin) -> Answer {
        let source = source_of(origin);
        match model::apply(&self.state, &self.registry, &self.holds, envelope) {
            Ok(t) => {
                if envelope.dry_run {
                    return Answer::Done(Ok(t));
                }
                // Written before anyone hears of it.
                if let Err(e) = self.write(&t.state) {
                    log::warn!(target: "egui_pop", "writing the context stack: {e}");
                }
                self.state = t.state.clone();
                self.events.push((
                    "context.changed".into(),
                    json!({
                        "rev": t.state.rev,
                        "state": t.state,
                        "entered": t.entered,
                        "left": t.left,
                        "request": envelope.request,
                        "redirected": t.redirected,
                        "origin": source,
                    }),
                ));
                for f in &t.left {
                    self.events.push((
                        "context.left".into(),
                        json!({ "frame": f, "rev": t.state.rev }),
                    ));
                }
                for f in &t.entered {
                    self.events.push((
                        "context.entered".into(),
                        json!({ "frame": f, "rev": t.state.rev }),
                    ));
                }
                Answer::Done(Ok(t))
            }
            Err(r) => {
                if !envelope.dry_run {
                    self.events.push((
                        "context.refused".into(),
                        json!({
                            "request": envelope.request,
                            "kind": r.kind,
                            "reason": r.reason,
                            "by": r.by,
                            "rev": r.rev,
                            "origin": source,
                        }),
                    ));
                }
                Answer::Done(Err(r))
            }
        }
    }

    fn hold(&mut self, groups: Vec<GroupId>, reason: &str, by: &str) -> Result<u64, String> {
        self.next_hold += 1;
        let hold = Hold {
            token: self.next_hold,
            groups,
            reason: reason.into(),
            by: by.into(),
        };
        self.events.push(("context.held".into(), json!(hold)));
        self.holds.push(hold);
        Ok(self.next_hold)
    }

    fn release(&mut self, token: u64) -> bool {
        let before = self.holds.len();
        self.holds.retain(|h| h.token != token);
        let released = self.holds.len() != before;
        if released {
            self.events
                .push(("context.released".into(), json!({ "token": token })));
        }
        released
    }

    fn declare(&mut self, owner: &OwnerId, spec: FrameSpec) -> Result<(), String> {
        let id = spec.id.clone();
        self.registry.insert(spec)?;
        self.owners.insert(id, owner.clone());
        Ok(())
    }

    fn remove_owner(&mut self, owner: &OwnerId) {
        let gone: Vec<FrameId> = self
            .owners
            .iter()
            .filter(|(_, o)| *o == owner)
            .map(|(f, _)| f.clone())
            .collect();
        for f in gone {
            self.registry.remove(&f);
            self.owners.remove(&f);
        }
    }

    fn take_events(&mut self) -> Vec<(String, Json)> {
        std::mem::take(&mut self.events)
    }
}

/// What the projection keeps.
#[derive(Debug, Default)]
struct Projection {
    /// The state projected last.
    applied: Option<State>,
    /// The frames, as the owner had them then.
    registry: FrameRegistry,
    /// The layouts of the projection keys not shown now.
    layouts: BTreeMap<String, Layout>,
    /// The view states, by projection key.
    views: BTreeMap<String, ViewState>,
    /// Values kept per place (its frame id, `root`).
    places: BTreeMap<String, BTreeMap<String, Json>>,
    /// The context keys it set.
    keys: BTreeSet<String>,
    /// Counts the projection's moves (scroll areas restore once per
    /// move).
    generation: u64,
    /// The place path each undo step and redo step was made in, as the
    /// store's histories stand.
    stamps: Vec<Vec<Entry>>,
    redo_stamps: Vec<Vec<Entry>>,
    undo_len: Option<usize>,
}

impl Projection {
    fn key(&self) -> String {
        self.applied
            .as_ref()
            .map_or_else(|| State::default().projection_key(), State::projection_key)
    }

    fn doc(&self) -> ViewDoc {
        ViewDoc {
            layouts: self.layouts.clone(),
            views: self.views.clone(),
            places: self.places.clone(),
        }
    }

    fn title(&self, ctx: &egui::Context, frame: &FrameId) -> String {
        title_in(ctx, &self.registry, frame)
    }

    /// A place as its crumb says it: its frame's label for the payload
    /// (`FrameSpec::label`), else its title, and its payload when it is a
    /// text or a number (`Album Holidays`, `Photo 3`).
    fn crumb(&self, ctx: &egui::Context, e: &Entry) -> String {
        let title = self.title(ctx, &e.frame);
        if let Some(label) = self
            .registry
            .get(&e.frame)
            .and_then(|s| s.label_of(&title, &e.payload))
        {
            return label;
        }
        match &e.payload {
            Json::String(s) if !s.is_empty() => format!("{title} {s}"),
            Json::Number(n) => format!("{title} {n}"),
            _ => title,
        }
    }
}

/// A frame's title in the interface's language (`frame-<id>.title`,
/// else its English); its id when it is not declared.
fn title_in(ctx: &egui::Context, registry: &FrameRegistry, frame: &FrameId) -> String {
    registry.get(frame).map_or_else(
        || frame.to_string(),
        |s| strings::frame_title(ctx, s.id.as_str(), &s.title),
    )
}

/// The context stack in the application: the default owner, the
/// projection, the requests waiting for a remote owner.
pub struct ContextService {
    local: Mutex<LocalContexts>,
    projection: Mutex<Projection>,
    /// The host owns the stack.
    custom: AtomicBool,
    pending: Mutex<Vec<(Pending, Origin)>>,
    /// The stack's keys written by hand, by key.
    hand_writes: Mutex<Vec<HandWrite>>,
    /// The documents the stack and its view are kept in (see
    /// [`documents`]), as read at start or at the last [`reload`].
    documents: Mutex<(String, String)>,
    /// The root crumb's title (`None`: "Home", translated).
    root: Option<String>,
    valid: Option<Validator>,
}

impl fmt::Debug for ContextService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ContextService")
            .field("custom", &self.custom.load(Ordering::Relaxed))
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

impl ContextService {
    /// Whether requests wait for a remote owner's answer.
    pub fn switching(&self) -> bool {
        !lock(&self.pending).is_empty()
    }

    /// The state the interface shows (the owner's, as projected last).
    pub fn shown(&self) -> State {
        lock(&self.projection).applied.clone().unwrap_or_default()
    }

    /// The frames, as projected last.
    pub fn frames(&self) -> FrameRegistry {
        lock(&self.projection).registry.clone()
    }

    /// The projection key shown.
    pub fn key(&self) -> String {
        lock(&self.projection).key()
    }

    /// The stack's own context keys written by hand, by key (see
    /// [`is_stack_key`]).
    pub fn hand_writes(&self) -> Vec<HandWrite> {
        lock(&self.hand_writes).clone()
    }
}

/// Whether a context key is the stack's own: `place`, `placeDepth`,
/// `modePushed`, `mode.<group>`. Only the projection sets them; a plugin
/// declares frames and reads them.
pub fn is_stack_key(key: &str) -> bool {
    matches!(key, "place" | "placeDepth" | "modePushed") || key.starts_with("mode.")
}

/// One of the stack's own context keys, set by hand (`AppHandle::set_context`)
/// instead of by the projection: two truths, and the projection's
/// overwrites the plugin's at the next transition.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct HandWrite {
    pub key: String,
    /// The value written last.
    pub value: String,
    /// How many times.
    pub times: u64,
    /// The command running at the last write, when one was.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<CommandId>,
    /// Who declared that command.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<OwnerId>,
}

impl fmt::Display for HandWrite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "`{}` set by hand to `{}` ({} time{}",
            self.key,
            self.value,
            self.times,
            if self.times == 1 { "" } else { "s" }
        )?;
        if let Some(c) = &self.command {
            write!(f, ", last by the command `{c}`")?;
            if let Some(o) = &self.owner {
                write!(f, " of `{o}`")?;
            }
        }
        write!(
            f,
            "): the context stack owns this key; declare a frame (`Ctx::frame`) and read the key instead"
        )
    }
}

/// Two frames that can be on together (a place and a mode, two modes of
/// different groups) binding the same keys to different commands
/// (`Ctx::frame_keybinding`): while both are on, one wins (the later
/// declared, between equals) and the other never fires.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct FrameClash {
    pub keys: String,
    /// The frame whose binding loses, and its command.
    pub loser: (FrameId, String),
    /// The frame whose binding wins, and its command.
    pub winner: (FrameId, String),
    /// Why it wins: `declared later`, `higher layer`, `more specific
    /// clause`.
    pub because: &'static str,
}

impl fmt::Display for FrameClash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "`{}`: the frames `{}` and `{}` can be on together and both bind it; while they are, `{}` ({}) wins over `{}` ({}), {}",
            self.keys,
            self.loser.0,
            self.winner.0,
            self.winner.1,
            self.winner.0,
            self.loser.1,
            self.loser.0,
            self.because
        )
    }
}

/// The keymap's frame clashes (see [`FrameClash`]): the key conflicts
/// (`AppHandle::key_conflicts`) between two frames' own bindings.
pub fn frame_clashes(app: &AppHandle) -> Vec<FrameClash> {
    let Some(registry) = with_owner(app, |c| c.registry()) else {
        return Vec::new();
    };
    let frame_of = |clause: &str| {
        registry
            .iter()
            .find(|f| f.clause() == clause)
            .map(|f| f.id.clone())
    };
    app.key_conflicts()
        .into_iter()
        .filter_map(|(lose, win)| {
            let (lf, wf) = (frame_of(&lose.when)?, frame_of(&win.when)?);
            if lf == wf {
                return None;
            }
            let because = if lose.layer != win.layer {
                "higher layer"
            } else {
                "declared later"
            };
            Some(FrameClash {
                keys: lose.keys.to_string(),
                loser: (lf, lose.action.to_string()),
                winner: (wf, win.action.to_string()),
                because,
            })
        })
        .collect()
}

/// Says each frame clash once, as a diagnostic (`context-frame-key-clash`).
fn report_clashes(app: &AppHandle) {
    let said: BTreeSet<String> = app
        .diagnostics()
        .into_iter()
        .filter(|d| d.code == "context-frame-key-clash")
        .map(|d| d.message)
        .collect();
    for c in frame_clashes(app) {
        let text = c.to_string();
        if !said.contains(&text) {
            app.warn("context-frame-key-clash", None, &c.keys, text);
        }
    }
}

/// Records a write of a stack key by hand; the first one of a key is a
/// diagnostic (`context-key-by-hand`). Nothing without the plugin.
pub(crate) fn written_by_hand(app: &AppHandle, key: &str, value: &str) {
    let Some(svc) = app.service(&CONTEXTS) else {
        return;
    };
    let (command, owner) = app
        .running_command()
        .map_or((None, None), |(c, o)| (Some(c), o));
    let first = {
        let mut writes = lock(&svc.hand_writes);
        match writes.iter_mut().find(|w| w.key == key) {
            Some(w) => {
                w.value = value.into();
                w.times += 1;
                w.command = command;
                w.owner = owner;
                None
            }
            None => {
                let w = HandWrite {
                    key: key.into(),
                    value: value.into(),
                    times: 1,
                    command,
                    owner,
                };
                writes.push(w.clone());
                Some(w)
            }
        }
    };
    if let Some(w) = first {
        app.warn("context-key-by-hand", w.owner.as_ref(), key, w.to_string());
    }
}

/// Runs `f` on the owner: the host's, else the plugin's `LocalContexts`.
/// `None` without the plugin, or while the host (owning the stack) runs a
/// command.
pub fn with_owner<R>(app: &AppHandle, f: impl FnOnce(&mut dyn Contexts) -> R) -> Option<R> {
    let svc = app.service(&CONTEXTS)?;
    let custom = match app.with_host_mut(|h| h.contexts().is_some()) {
        Some(c) => {
            svc.custom.store(c, Ordering::Relaxed);
            c
        }
        None => svc.custom.load(Ordering::Relaxed),
    };
    if custom {
        app.with_host_mut(|h| h.contexts().map(f)).flatten()
    } else {
        let mut local = lock(&svc.local);
        Some(f(&mut *local))
    }
}

fn publish_events(app: &AppHandle, source: &str, events: Vec<(String, Json)>) {
    for (name, data) in events {
        app.publish(source, None, &name, data);
    }
}

/// The state the interface shows, as projected last (`None` without the
/// plugin).
pub fn state(app: &AppHandle) -> Option<State> {
    app.service(&CONTEXTS).map(|s| s.shown())
}

/// Asks the owner for a transition, through the `context.request` step;
/// the projection follows (at once, or at the next pass while the areas
/// are drawn). A refusal or a veto asked by the user shows in a toast,
/// unless there was nothing to do.
pub fn request(app: &AppHandle, envelope: Envelope, origin: &Origin) -> Outcome {
    let args = json!({ "request": envelope, "origin": source_of(origin) });
    let refused: Rc<RefCell<Option<Refusal>>> = Rc::default();
    let said = refused.clone();
    let who = origin.clone();
    let out = app.step(&STEP_REQUEST, args, move |app, args| {
        let env: Envelope = serde_json::from_value(args["request"].clone())
            .map_err(|e| format!("not a context request: {e}"))?;
        let (answer, events) = with_owner(app, |c| {
            let a = c.request(&env, &who);
            (a, c.take_events())
        })
        .ok_or_else(|| NO_STACK.to_string())?;
        publish_events(app, &source_of(&who), events);
        match answer {
            Answer::Done(Ok(t)) => Ok(json!({
                "rev": t.state.rev,
                "entered": t.entered,
                "left": t.left,
                "redirected": t.redirected,
                "dry_run": env.dry_run,
            })),
            Answer::Done(Err(r)) => {
                let msg = r.to_string();
                *said.borrow_mut() = Some(r);
                Err(msg)
            }
            Answer::Later(p) => {
                if let Some(svc) = app.service(&CONTEXTS) {
                    lock(&svc.pending).push((p, who.clone()));
                }
                Ok(json!({ "pending": true }))
            }
        }
    });
    if app.drawing_areas() {
        app.egui().request_repaint();
    } else {
        sync(app);
    }
    if *origin == Origin::User {
        let reason = match &out {
            Outcome::Vetoed { reason, .. } => Some(reason.clone()),
            Outcome::Done(Err(e)) => match &*refused.borrow() {
                Some(r) if r.kind == RefusalKind::Nothing => None,
                Some(r) => Some(r.reason.clone()),
                None => Some(e.clone()),
            },
            Outcome::Done(Ok(_)) => None,
        };
        if let Some(reason) = reason {
            refusal_toast(app, &reason);
        }
    }
    out
}

fn refusal_toast(app: &AppHandle, reason: &str) {
    if let Some(t) = app.service(&crate::toasts::TOASTS) {
        let ctx = app.egui();
        t.notify(crate::toasts::Toast::warning(&strings::tr_args(
            &ctx,
            &txt::CONTEXT_REFUSED,
            &[("reason", reason)],
        )));
    }
}

/// Enters a place, as the user.
pub fn enter(app: &AppHandle, frame: FrameId, payload: Json) -> Outcome {
    request(
        app,
        Envelope::new(Request::Enter { frame, payload }),
        &Origin::User,
    )
}

/// Leaves the top place, as the user.
pub fn leave(app: &AppHandle) -> Outcome {
    request(app, Envelope::new(Request::Leave), &Origin::User)
}

/// Back, as the user: pops the last pushed mode, else leaves the top
/// place.
pub fn back(app: &AppHandle) -> Outcome {
    request(app, Envelope::new(Request::Back), &Origin::User)
}

/// Leaves places until `depth` are left (0: the root), as the user.
pub fn jump(app: &AppHandle, depth: usize) -> Outcome {
    request(app, Envelope::new(Request::Jump { depth }), &Origin::User)
}

/// Goes to a place path, as the user.
pub fn go(app: &AppHandle, places: Vec<Entry>) -> Outcome {
    request(app, Envelope::new(Request::Go { places }), &Origin::User)
}

/// Sets a mode, as the user.
pub fn set_mode(app: &AppHandle, mode: FrameId) -> Outcome {
    request(app, Envelope::new(Request::set(mode)), &Origin::User)
}

/// Pushes a mode, as the user.
pub fn push_mode(app: &AppHandle, mode: FrameId) -> Outcome {
    request(app, Envelope::new(Request::push(mode)), &Origin::User)
}

/// Pops the last pushed mode, as the user.
pub fn pop_mode(app: &AppHandle) -> Outcome {
    request(app, Envelope::new(Request::PopMode), &Origin::User)
}

/// Turns a group's mode off, as the user.
pub fn clear_mode(app: &AppHandle, group: GroupId) -> Outcome {
    request(
        app,
        Envelope::new(Request::ClearMode { group }),
        &Origin::User,
    )
}

/// Holds groups of the stack (empty: all of it) for `by`: requests
/// touching them are refused with `reason` until [`release`].
pub fn hold(app: &AppHandle, groups: Vec<GroupId>, reason: &str, by: &str) -> Result<u64, String> {
    let (r, events) = with_owner(app, |c| {
        let r = c.hold(groups, reason, by);
        (r, c.take_events())
    })
    .ok_or_else(|| NO_STACK.to_string())?;
    publish_events(app, by, events);
    r
}

/// Releases a hold; whether it was held.
pub fn release(app: &AppHandle, token: u64) -> bool {
    let Some((r, events)) = with_owner(app, |c| {
        let r = c.release(token);
        (r, c.take_events())
    }) else {
        return false;
    };
    publish_events(app, "egui_pop", events);
    r
}

/// Projects the owner's state when its revision moved (each pass does).
pub fn sync(app: &AppHandle) {
    let Some(svc) = app.service(&CONTEXTS) else {
        return;
    };
    let applied = lock(&svc.projection).applied.as_ref().map(|s| s.rev);
    let Some(rev) = with_owner(app, |c| c.rev()) else {
        return;
    };
    if applied == Some(rev) {
        return;
    }
    let Some((state, registry, events)) =
        with_owner(app, |c| (c.state(), c.registry(), c.take_events()))
    else {
        return;
    };
    publish_events(app, "egui_pop", events);
    project(app, &svc, state, registry);
}

/// Shows a panel in a layout, in its declared area when it was closed.
fn show_panel(app: &AppHandle, layout: &mut Layout, panel: &PanelId) {
    if layout.shows(panel) {
        return;
    }
    if let Some(spec) = app.panels().into_iter().find(|s| s.id == *panel) {
        layout.place(panel, spec.area);
    }
}

/// The layout of a projection key seen for the first time: the arranged
/// frame's arrangement when it changed (a mode declaring one, else the
/// place; see `arrangements::arranged_frame`), the place's layout as it
/// was left, or the layout shown; then the modes' panels shown and
/// hidden.
fn derive(
    app: &AppHandle,
    p: &Projection,
    old: Option<&State>,
    new: &State,
    registry: &FrameRegistry,
    current: &Layout,
) -> Layout {
    let same_place = old.is_some_and(|o| o.path_key() == new.path_key());
    let arranged = crate::arrangements::arranged_in(new, registry);
    let was_arranged = old.and_then(|o| crate::arrangements::arranged_in(o, registry));
    let arrangement = |frame: &FrameId, declared: Option<String>| {
        crate::arrangements::of_frame(app, frame, declared)
            .and_then(|name| crate::arrangements::layout_of(app, &name))
    };
    let mut layout =
        if same_place && arranged.as_ref().map(|a| &a.0) == was_arranged.as_ref().map(|a| &a.0) {
            let mut l = current.clone();
            // The modes left take back what they did.
            if let Some(o) = old {
                for m in o.modes.iter().filter(|m| !new.is_on(&m.frame)) {
                    if let Some(spec) = registry.get(&m.frame) {
                        for panel in &spec.show {
                            l.hide(panel);
                        }
                        for panel in &spec.hide {
                            show_panel(app, &mut l, panel);
                        }
                    }
                }
            }
            l
        } else {
            let base = State {
                modes: Vec::new(),
                ..new.clone()
            };
            let place = new.places.last().map(|e| &e.frame);
            match arranged {
                // A mode with an arrangement of its own.
                Some((frame, declared)) if Some(&frame) != place => arrangement(&frame, declared),
                // The place: as it was left, else its arrangement (the
                // one its payload chooses, `FrameSpec::arrangement_for`).
                Some((frame, declared)) => p
                    .layouts
                    .get(&base.projection_key())
                    .cloned()
                    .or_else(|| arrangement(&frame, declared)),
                // The root, as it was left.
                None => p.layouts.get(&base.projection_key()).cloned(),
            }
            .unwrap_or_else(|| current.clone())
        };
    for m in &new.modes {
        if let Some(spec) = registry.get(&m.frame) {
            for panel in &spec.show {
                show_panel(app, &mut layout, panel);
            }
            for panel in &spec.hide {
                layout.hide(panel);
            }
        }
    }
    layout
}

/// Projects a new state: keeps the view of the key left, puts the
/// layout of the key entered, sets the context keys, gives the focus
/// back, says where the user is now.
fn project(app: &AppHandle, svc: &ContextService, new: State, registry: FrameRegistry) {
    let current = app.layout();
    let focused = app.focused_panel();
    let mut p = lock(&svc.projection);
    let old = p.applied.clone();
    let old_key = old.as_ref().map(State::projection_key);
    let new_key = new.projection_key();
    let moved = old_key.as_deref() != Some(new_key.as_str());
    let mut target: Option<Layout> = None;
    let mut focus: Option<PanelId> = None;
    if moved {
        if let Some(k) = &old_key {
            p.layouts.insert(k.clone(), current.clone());
            p.views.entry(k.clone()).or_default().focus = focused;
            let kept = p.layouts.remove(&new_key);
            target = Some(match kept {
                Some(l) => l,
                None => derive(app, &p, old.as_ref(), &new, &registry, &current),
            });
        } else {
            // At start the layout loaded is the shown key's, unless the
            // restore dropped frames and the key shown is another one.
            target = p.layouts.remove(&new_key);
        }
        focus = p.views.get(&new_key).and_then(|v| v.focus.clone());
        p.generation += 1;
    }
    let keys = new.when_keys(&registry);
    let stale: Vec<String> = p
        .keys
        .iter()
        .filter(|k| !keys.iter().any(|(n, _)| n == *k))
        .cloned()
        .collect();
    p.keys = keys.iter().map(|(k, _)| k.clone()).collect();
    p.registry = registry;
    p.applied = Some(new.clone());
    let ctx = app.egui();
    let path: Vec<String> = std::iter::once(root_title(app, svc))
        .chain(new.places.iter().map(|e| p.crumb(&ctx, e)))
        .collect();
    let doc = p.doc();
    drop(p);
    for k in stale {
        app.set_stack_context(&k, "");
    }
    for (k, v) in &keys {
        app.set_stack_context(k, v);
    }
    if let Some(l) = target
        && l != current
    {
        app.set_layout(l);
    }
    if let Some(panel) = focus
        && app.panel_visible(&panel)
    {
        app.focus_panel(&panel);
    }
    save_view(app, &doc);
    if moved && old.is_some() {
        let text = strings::tr_args(&ctx, &txt::CONTEXT_NOW_IN, &[("place", &path.join(" › "))]);
        a11y::announce(&ctx, text, Politeness::Polite);
    }
    app.egui().request_repaint();
}

fn save_view(app: &AppHandle, doc: &ViewDoc) {
    if app.persist().is_none() {
        return;
    }
    match model::view_schema().save(doc) {
        Ok(text) => {
            let document = app
                .service(&CONTEXTS)
                .map_or_else(|| VIEW_DOCUMENT.into(), |s| lock(&s.documents).1.clone());
            app.save_later(&document, text);
        }
        Err(e) => log::warn!(target: "egui_pop", "the view document: {e}"),
    }
}

fn root_title(app: &AppHandle, svc: &ContextService) -> String {
    svc.root
        .clone()
        .unwrap_or_else(|| strings::tr(&app.egui(), &txt::CONTEXT_ROOT))
}

/// A value a component keeps for the projection key shown (a list's
/// scroll offset, a selection), saved with the view state.
pub fn view_value(app: &AppHandle, name: &str) -> Option<Json> {
    let svc = app.service(&CONTEXTS)?;
    let p = lock(&svc.projection);
    let key = p.key();
    p.views.get(&key).and_then(|v| v.values.get(name).cloned())
}

/// Keeps a value for the projection key shown; written to the `view`
/// document a second later.
pub fn set_view_value(app: &AppHandle, name: &str, value: Json) {
    let Some(svc) = app.service(&CONTEXTS) else {
        return;
    };
    let doc = {
        let mut p = lock(&svc.projection);
        let key = p.key();
        let view = p.views.entry(key).or_default();
        if view.values.get(name) == Some(&value) {
            return;
        }
        view.values.insert(name.to_string(), value);
        p.doc()
    };
    save_view(app, &doc);
}

/// A value kept for a place (its frame id, [`ROOT`] at the root) rather
/// than for a projection key: the same in every album, whatever the
/// modes. Saved with the view state.
pub fn place_value_of(app: &AppHandle, place: &str, name: &str) -> Option<Json> {
    let svc = app.service(&CONTEXTS)?;
    let p = lock(&svc.projection);
    p.places.get(place).and_then(|v| v.get(name).cloned())
}

/// A value kept for the place shown ([`place_value_of`]).
pub fn place_value(app: &AppHandle, name: &str) -> Option<Json> {
    let place = state(app)?.place().to_string();
    place_value_of(app, &place, name)
}

/// Keeps a value for the place shown (`Json::Null` forgets it); written
/// to the `view` document a second later.
pub fn set_place_value(app: &AppHandle, name: &str, value: Json) {
    let Some(svc) = app.service(&CONTEXTS) else {
        return;
    };
    let doc = {
        let mut p = lock(&svc.projection);
        let place = p.applied.clone().unwrap_or_default().place().to_string();
        let values = p.places.entry(place.clone()).or_default();
        if value.is_null() {
            if values.remove(name).is_none() {
                return;
            }
            if values.is_empty() {
                p.places.remove(&place);
            }
        } else {
            if values.get(name) == Some(&value) {
                return;
            }
            values.insert(name.to_string(), value);
        }
        p.doc()
    };
    save_view(app, &doc);
}

/// A scroll area whose offset is kept per projection key: entering a
/// place and coming back finds it where it was.
pub fn scroll_area<R>(
    ui: &mut Ui,
    app: &AppHandle,
    name: &str,
    area: egui::ScrollArea,
    add: impl FnOnce(&mut Ui) -> R,
) -> R {
    let generation = app
        .service(&CONTEXTS)
        .map_or(0, |s| lock(&s.projection).generation);
    let saved = view_value(app, name)
        .and_then(|v| v.as_f64())
        .map(|v| v as f32);
    let id = ui.make_persistent_id(("egui_pop.contexts.scroll", name));
    let slot = SCROLL_APPLIED.keyed(id);
    let applied = slot.get(ui.ctx());
    let mut area = area.id_salt(id);
    if applied != Some(generation) {
        area = area.vertical_scroll_offset(saved.unwrap_or(0.0));
        slot.set(ui.ctx(), generation);
    }
    let out = area.show(ui, add);
    let y = out.state.offset.y;
    let changed = match saved {
        Some(s) => (s - y).abs() > 0.5,
        None => y > 0.5,
    };
    if changed {
        set_view_value(app, name, json!(y));
    }
    out.inner
}

/// The breadcrumb's crumbs as the interface says them: the root's title,
/// then each place's (its title, and its payload when it is a text or a
/// number). Crumb `i` is reached with [`jump`]`(app, i)`. Empty without
/// the context plugin.
pub fn crumbs(app: &AppHandle) -> Vec<String> {
    let Some(svc) = app.service(&CONTEXTS) else {
        return Vec::new();
    };
    let ctx = app.egui();
    let p = lock(&svc.projection);
    let state = p.applied.clone().unwrap_or_default();
    std::iter::once(root_title(app, &svc))
        .chain(state.places.iter().map(|e| p.crumb(&ctx, e)))
        .collect()
}

/// An active mode, as a chip shows it.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Chip {
    pub frame: FrameId,
    pub group: GroupId,
    /// Its title, translated.
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Its tone's name, as declared (`FrameSpec::tone`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tone: Option<String>,
    /// The last pushed mode: leaving it goes back to what it replaced.
    pub pushed: bool,
    /// Its group is shown in the status bar (`FrameSpec::visible`).
    pub visible: bool,
}

impl Chip {
    /// The request leaving it: a pop for the last pushed mode (back to
    /// what it replaced), else its group turned off.
    pub fn leave(&self) -> Request {
        if self.pushed {
            Request::PopMode
        } else {
            Request::ClearMode {
                group: self.group.clone(),
            }
        }
    }
}

/// The active modes, in the order they came. Empty without the context
/// plugin.
pub fn chips(app: &AppHandle) -> Vec<Chip> {
    let Some(svc) = app.service(&CONTEXTS) else {
        return Vec::new();
    };
    let ctx = app.egui();
    let p = lock(&svc.projection);
    let state = p.applied.clone().unwrap_or_default();
    let visible = p.registry.visible_groups();
    let last_pushed = state.modes.iter().rposition(|m| m.life == Life::Pushed);
    state
        .modes
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let spec = p.registry.get(&m.frame);
            Chip {
                frame: m.frame.clone(),
                group: m.group.clone(),
                title: p.title(&ctx, &m.frame),
                icon: spec.and_then(|s| s.icon.clone()),
                tone: spec.and_then(|s| s.tone.clone()),
                pushed: Some(i) == last_pushed,
                visible: visible.contains(&m.group),
            }
        })
        .collect()
}

/// Why the owner would refuse a request now (its guards, a hold), asked
/// as a dry run of the stack's own owner; `None` when it would go, when
/// there is nothing to do, or when the host owns the stack (its answer
/// may come later).
pub fn why_not(app: &AppHandle, request: Request) -> Option<String> {
    let svc = app.service(&CONTEXTS)?;
    if svc.custom.load(Ordering::Relaxed) {
        return None;
    }
    let env = Envelope::new(request).dry_run();
    let answer = with_owner(app, |c| c.request(&env, &Origin::User))?;
    match answer {
        Answer::Done(Err(r)) if r.kind != RefusalKind::Nothing => Some(r.reason),
        _ => None,
    }
}

/// The breadcrumb: the root, then each place; a crumb jumps back to its
/// level (the last one is where the user is).
pub fn breadcrumb(ui: &mut Ui, app: &AppHandle) {
    let Some(svc) = app.service(&CONTEXTS) else {
        return;
    };
    let crumbs = crumbs(app);
    let mut jump_to = None;
    ui.horizontal_wrapped(|ui| {
        for (i, title) in crumbs.iter().enumerate() {
            if i > 0 {
                ui.weak("›");
            }
            let here = i + 1 == crumbs.len();
            if ui
                .add(widgets::SelectableRow::new(here, title.as_str()))
                .clicked()
                && !here
            {
                jump_to = Some(i);
            }
        }
        if svc.switching() {
            ui.spinner();
        }
    });
    if let Some(depth) = jump_to {
        jump(app, depth);
    }
}

/// The active modes as chips, each with a way out (a pushed mode pops,
/// a set one is turned off).
pub fn mode_chips(ui: &mut Ui, app: &AppHandle) {
    let chips = chips(app);
    let mut leave = None;
    ui.horizontal_wrapped(|ui| {
        for chip in &chips {
            let title = &chip.title;
            let r = ui.add(widgets::SelectableRow::new(true, format!("{title}  ×")));
            let name = strings::tr_args(ui.ctx(), &txt::CONTEXT_LEAVE_MODE, &[("mode", title)]);
            a11y::name(&r, WidgetType::Button, &name);
            let r = r.on_hover_text(name.as_str());
            if r.clicked() {
                leave = Some(chip.leave());
            }
        }
    });
    if let Some(r) = leave {
        request(app, Envelope::new(r), &Origin::User);
    }
}

/// The modes of a group as a row of choices, the one on selected;
/// `none` adds a choice turning the group off.
pub fn mode_switch(ui: &mut Ui, app: &AppHandle, group: &GroupId, none: bool) {
    let Some(svc) = app.service(&CONTEXTS) else {
        return;
    };
    let (modes, on): (Vec<(FrameId, String)>, Option<FrameId>) = {
        let p = lock(&svc.projection);
        let groups = p.registry.groups();
        let modes = groups
            .get(group)
            .map(|v| {
                v.iter()
                    .map(|s| {
                        (
                            s.id.clone(),
                            strings::frame_title(ui.ctx(), s.id.as_str(), &s.title),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let on = p
            .applied
            .as_ref()
            .and_then(|s| s.mode_of(group.as_str()).cloned());
        (modes, on)
    };
    let mut asked = None;
    ui.horizontal_wrapped(|ui| {
        if none {
            let label = strings::tr(ui.ctx(), &txt::CONTEXT_NO_MODE);
            if ui
                .add(widgets::SelectableRow::new(on.is_none(), label))
                .clicked()
                && on.is_some()
            {
                asked = Some(Request::ClearMode {
                    group: group.clone(),
                });
            }
        }
        for (id, title) in &modes {
            let selected = on.as_ref() == Some(id);
            if ui
                .add(widgets::SelectableRow::new(selected, title.as_str()))
                .clicked()
                && !selected
            {
                asked = Some(Request::set(id.clone()));
            }
        }
    });
    if let Some(r) = asked {
        request(app, Envelope::new(r), &Origin::User);
    }
}

/// A pushed mode as a toggle: pushes it, or pops it (turns it off when
/// it is not the last pushed).
pub fn mode_toggle(ui: &mut Ui, app: &AppHandle, mode: &FrameId) -> egui::Response {
    let (title, on, last) = match app.service(&CONTEXTS) {
        Some(svc) => {
            let p = lock(&svc.projection);
            let state = p.applied.clone().unwrap_or_default();
            let entry = state.modes.iter().find(|m| m.frame == *mode).cloned();
            let last = state
                .modes
                .iter()
                .rev()
                .find(|m| m.life == Life::Pushed)
                .is_some_and(|m| m.frame == *mode);
            (p.title(ui.ctx(), mode), entry, last)
        }
        None => (mode.to_string(), None, false),
    };
    let r = ui.add(widgets::SelectableRow::new(on.is_some(), title));
    if r.clicked() {
        let req = match &on {
            None => Request::push(mode.clone()),
            Some(_) if last => Request::PopMode,
            Some(m) => Request::ClearMode {
                group: m.group.clone(),
            },
        };
        request(app, Envelope::new(req), &Origin::User);
    }
    r
}

/// Follows the store's undo and redo histories: each new step is stamped
/// with the place path it was made in.
fn track_undo(app: &AppHandle, svc: &ContextService) {
    let (undo, redo) = {
        let store = app.store();
        let s = lock(&store);
        (s.undo_steps().len(), s.redo_steps().len())
    };
    let mut p = lock(&svc.projection);
    let here = p
        .applied
        .as_ref()
        .map(|s| s.places.clone())
        .unwrap_or_default();
    if p.undo_len.is_some_and(|n| undo > n) && redo == 0 {
        p.redo_stamps.clear();
    }
    p.stamps.truncate(undo);
    while p.stamps.len() < undo {
        p.stamps.push(here.clone());
    }
    p.redo_stamps.truncate(redo);
    while p.redo_stamps.len() < redo {
        p.redo_stamps.insert(0, here.clone());
    }
    p.undo_len = Some(undo);
}

/// After an undo (or a redo): a change made in another place says where,
/// with "Go there".
fn undone(app: &AppHandle, redo: bool, result: &Result<Json, String>) {
    let key = if redo { "redone" } else { "undone" };
    let Ok(v) = result else { return };
    if v[key].is_null() {
        return;
    }
    let Some(svc) = app.service(&CONTEXTS) else {
        return;
    };
    let (stamp, here, title) = {
        let mut p = lock(&svc.projection);
        let stamp = if redo {
            p.redo_stamps.pop()
        } else {
            p.stamps.pop()
        };
        if let Some(s) = &stamp {
            if redo {
                p.stamps.push(s.clone());
            } else {
                p.redo_stamps.push(s.clone());
            }
        }
        p.undo_len = p
            .undo_len
            .map(|n| if redo { n + 1 } else { n.saturating_sub(1) });
        let here = p
            .applied
            .as_ref()
            .map(|s| s.places.clone())
            .unwrap_or_default();
        let title = stamp
            .as_ref()
            .and_then(|s| s.last())
            .map(|e| p.crumb(&app.egui(), e));
        (stamp, here, title)
    };
    let Some(stamp) = stamp else { return };
    if stamp == here {
        return;
    }
    let Some(t) = app.service(&crate::toasts::TOASTS) else {
        return;
    };
    let ctx = app.egui();
    let place = title.unwrap_or_else(|| root_title(app, &svc));
    let text = strings::tr_args(
        &ctx,
        if redo {
            &txt::CONTEXT_REDONE_IN
        } else {
            &txt::CONTEXT_UNDONE_IN
        },
        &[("place", &place)],
    );
    t.notify(crate::toasts::Toast::info(&text).action(
        &strings::tr(&ctx, &txt::CONTEXT_GO_THERE),
        CMD_GO,
        json!({ "places": stamp }),
    ));
}

/// Each pass: the answers a remote owner gave, the projection, the undo
/// stamps.
fn pass(app: &AppHandle) {
    let Some(svc) = app.service(&CONTEXTS) else {
        return;
    };
    let arrived: Vec<(Result<Json, String>, Origin)> = {
        let mut pending = lock(&svc.pending);
        let mut out = Vec::new();
        pending.retain(|(p, o)| match p.take() {
            Some(r) => {
                out.push((r, o.clone()));
                false
            }
            None => true,
        });
        out
    };
    for (r, origin) in arrived {
        if let (Err(e), Origin::User) = (&r, &origin) {
            refusal_toast(app, e);
        }
    }
    sync(app);
    track_undo(app, &svc);
}

/// The documents the stack (`LocalContexts`) and its view are kept in:
/// [`CONTEXT_DOCUMENT`] and [`VIEW_DOCUMENT`] beside the layout's
/// document as the host names it (`Host::layout_document`), so a host
/// keeping a layout per workspace keeps a stack per workspace too; at the
/// top when it names none.
pub fn documents(app: &AppHandle) -> (String, String) {
    let layout = app.with_host_mut(|h| h.layout_document()).flatten();
    match layout.as_deref().and_then(|l| l.rsplit_once('/')) {
        Some((dir, _)) => (
            format!("{dir}/{CONTEXT_DOCUMENT}"),
            format!("{dir}/{VIEW_DOCUMENT}"),
        ),
        None => (CONTEXT_DOCUMENT.into(), VIEW_DOCUMENT.into()),
    }
}

/// Reads the stack and its view again from [`documents`], as at start:
/// after the host changed where they are (another workspace opened in
/// the same window). What was shown is forgotten (its view was written
/// already); what the documents hold, checked against today's frames,
/// is projected.
pub fn reload(app: &AppHandle) {
    let Some(svc) = app.service(&CONTEXTS) else {
        return;
    };
    {
        let mut p = lock(&svc.projection);
        p.applied = None;
        p.layouts.clear();
        p.views.clear();
        p.places.clear();
    }
    if !svc.custom.load(Ordering::Relaxed) {
        let mut l = lock(&svc.local);
        // Without a document the stack starts empty, its revision kept.
        let rev = l.state.rev;
        l.state = State {
            rev,
            ..State::default()
        };
    }
    start(app);
}

/// At start: the owner's stack restored (`LocalContexts` from its
/// document), the view document loaded, the first projection.
fn start(app: &AppHandle) {
    let Some(svc) = app.service(&CONTEXTS) else {
        return;
    };
    let (context_document, view_document) = documents(app);
    *lock(&svc.documents) = (context_document.clone(), view_document.clone());
    let custom = app
        .with_host_mut(|h| h.contexts().is_some())
        .unwrap_or(false);
    svc.custom.store(custom, Ordering::Relaxed);
    let Some(persist) = app.persist() else {
        sync(app);
        return;
    };
    if !custom {
        let valid = svc.valid.clone();
        let (restored, events) = {
            let mut l = lock(&svc.local);
            l.set_persist(Some(persist.clone()), &context_document);
            let r = match &valid {
                Some(v) => l.restore(&**v),
                None => l.restore(&|_, _| None),
            };
            (r, l.take_events())
        };
        publish_events(app, "egui_pop", events);
        match restored {
            Ok(dropped) if !dropped.is_empty() => {
                if let Some(t) = app.service(&crate::toasts::TOASTS) {
                    let registry = lock(&svc.local).registry();
                    let ctx = app.egui();
                    let frames = dropped
                        .iter()
                        .map(|d| format!("{} ({})", title_in(&ctx, &registry, &d.frame), d.why))
                        .collect::<Vec<_>>()
                        .join(", ");
                    t.notify(crate::toasts::Toast::warning(&strings::tr_args(
                        &app.egui(),
                        &txt::CONTEXT_DROPPED,
                        &[("frames", &frames)],
                    )));
                }
            }
            Ok(_) => {}
            Err(e) => log::warn!(target: "egui_pop", "the context stack could not be read: {e}"),
        }
    }
    match persist.load::<ViewDoc>(&view_document, &model::view_schema()) {
        Ok(Some(read)) => {
            if let Ok(text) = model::view_schema().save(&read.value) {
                app.saved_already(&view_document, &text);
            }
            let mut p = lock(&svc.projection);
            p.layouts = read.value.layouts;
            p.views = read.value.views;
            p.places = read.value.places;
        }
        Ok(None) => {}
        Err(e) => log::warn!(target: "egui_pop", "the view document could not be read: {e}"),
    }
    sync(app);
}

/// The crumbs as the interface says them, for `context.read`: the
/// root's title, then each place's.
fn crumb_labels(app: &AppHandle, svc: &ContextService, p: &Projection) -> Vec<String> {
    let ctx = app.egui();
    let places = p
        .applied
        .as_ref()
        .map(|s| s.places.as_slice())
        .unwrap_or_default();
    std::iter::once(root_title(app, svc))
        .chain(places.iter().map(|e| p.crumb(&ctx, e)))
        .collect()
}

/// What the `contexts` collector shows.
fn describe(app: &AppHandle) -> Json {
    let Some(svc) = app.service(&CONTEXTS) else {
        return Json::Null;
    };
    let owner = with_owner(app, |c| {
        json!({
            "state": c.state(),
            "holds": c.holds(),
            "frames": c.registry().iter().collect::<Vec<_>>(),
        })
    });
    let projection = {
        let p = lock(&svc.projection);
        json!({
            "rev": p.applied.as_ref().map(|s| s.rev),
            "key": p.key(),
            "keys": p.keys,
            "layouts": p.layouts.keys().collect::<Vec<_>>(),
            "views": p.views,
            "places": p.places,
            "undo_stamps": p.stamps.len(),
            "crumbs": crumb_labels(app, &svc, &p),
        })
    };
    let last = app.last_event();
    let events: Vec<_> = app
        .events_since(last.saturating_sub(64))
        .into_iter()
        .filter(|e| e.name.starts_with("context."))
        .collect();
    let hand_writes = svc.hand_writes();
    let clashes = frame_clashes(app);
    let warnings: Vec<String> = hand_writes
        .iter()
        .map(ToString::to_string)
        .chain(clashes.iter().map(ToString::to_string))
        .collect();
    json!({
        "owner": owner,
        "status": crate::strips::visible(app),
        "warnings": warnings,
        "hand_writes": hand_writes,
        "clashes": clashes,
        "host_owns": svc.custom.load(Ordering::Relaxed),
        "switching": svc.switching(),
        "projection": projection,
        "events": events,
    })
}

fn outcome_json(o: Outcome) -> Result<Json, String> {
    match o {
        Outcome::Done(r) => r,
        Outcome::Vetoed { by, reason } => Err(format!("vetoed by {by}: {reason}")),
    }
}

fn parse<T: serde::de::DeserializeOwned>(args: &Json, what: &str) -> Result<T, String> {
    serde_json::from_value(args.clone()).map_err(|e| format!("{what}: {e}"))
}

/// The context stack as a plugin: the default owner (a
/// [`LocalContexts`], unless the host owns the stack), the projection,
/// the `context.request` step, the commands and keys, the `contexts`
/// collector, the undo toasts.
#[derive(Default)]
pub struct ContextsPlugin {
    root: Option<String>,
    valid: Option<Validator>,
}

impl fmt::Debug for ContextsPlugin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ContextsPlugin")
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

impl ContextsPlugin {
    /// The root crumb's title ("Home", translated, by default).
    pub fn root(mut self, title: &str) -> Self {
        self.root = Some(title.into());
        self
    }

    /// Whether a saved frame may come back at start (its payload still
    /// exists): `None` when it may, else why not.
    pub fn valid(
        mut self,
        f: impl Fn(&FrameId, &Json) -> Option<String> + Send + Sync + 'static,
    ) -> Self {
        self.valid = Some(Arc::new(f));
        self
    }
}

impl Plugin for ContextsPlugin {
    fn id(&self) -> PluginId {
        PLUGIN
    }

    fn title(&self) -> String {
        "Context stack".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.service(
            &CONTEXTS,
            ContextService {
                local: Mutex::new(LocalContexts::new()),
                projection: Mutex::new(Projection::default()),
                custom: AtomicBool::new(false),
                pending: Mutex::new(Vec::new()),
                hand_writes: Mutex::new(Vec::new()),
                documents: Mutex::new((CONTEXT_DOCUMENT.into(), VIEW_DOCUMENT.into())),
                root: self.root.clone(),
                valid: self.valid.clone(),
            },
        )?;
        ctx.step(
            StepSpec::new(
                STEP_REQUEST,
                "A transition of the context stack is asked of its owner; a veto keeps it from being asked, a redirect asks another.",
            )
            .args(json!({ "request": { "request": { "op": "enter", "frame": "demo.album" } }, "origin": "user" })),
        )?;
        let category = "Navigation";
        ctx.command(
            CommandSpec::new(CMD_BACK, "Back", category)
                .description("Pops the last pushed mode, else leaves the place shown for its parent.")
                .key("Alt+Left"),
            |call| outcome_json(request(call.app, Envelope::new(Request::Back), call.origin)),
        )?
        .command(
            CommandSpec::new(CMD_UP, "Up to the parent place", category)
                .description("Leaves the place shown for its parent.")
                .key("Alt+Shift+Up")
                .when("placeDepth != '0'"),
            |call| outcome_json(request(call.app, Envelope::new(Request::Leave), call.origin)),
        )?
        .command(
            CommandSpec::new(CMD_ENTER, "Enter a place", category)
                .description("Enters a place under the one shown (`frame`, `payload`).")
                .args(json!({ "frame": "demo.album", "payload": null })),
            |call| {
                let frame: FrameId = parse(&call.args["frame"], "`frame`")?;
                let payload = call.args.get("payload").cloned().unwrap_or(Json::Null);
                outcome_json(request(
                    call.app,
                    Envelope::new(Request::Enter { frame, payload }),
                    call.origin,
                ))
            },
        )?
        .command(
            CommandSpec::new(CMD_READ, "Read the context stack", category)
                .description("The places and modes on, their revision, the holds and the frames declared, and what the status bar shows (`status`: each visible item's id, text, tone, count and progress)."),
            |call| Ok(describe(call.app)),
        )?
        .command(
            CommandSpec::new(CMD_REQUEST, "Ask for a context transition", category)
                .description("Asks the owner for a transition (`request`: `op` enter, leave, jump, go, set_mode, push_mode, pop_mode, clear_mode or back), checked against `if_rev`, applied unless `dry_run`.")
                .args(json!({ "request": { "op": "back" }, "if_rev": null, "dry_run": false })),
            |call| {
                let env: Envelope = parse(call.args, "a context request")?;
                outcome_json(request(call.app, env, call.origin))
            },
        )?
        .command(
            CommandSpec::new(CMD_GO, "Go to a place", category)
                .description("Goes to a place path (`places`: each `frame` with its `payload`).")
                .args(json!({ "places": [{ "frame": "demo.album" }] })),
            |call| {
                let places: Vec<Entry> = parse(&call.args["places"], "`places`")?;
                outcome_json(request(call.app, Envelope::new(Request::Go { places }), call.origin))
            },
        )?
        .command(
            CommandSpec::new(CMD_SET_MODE, "Set a mode", category)
                .description("Sets a mode (`mode`), or pushes it (`push`: true); with `group` and no mode, turns the group off.")
                .args(json!({ "mode": "demo.culling", "push": false })),
            |call| {
                let req = match call.args.get("mode").filter(|m| !m.is_null()) {
                    Some(m) => {
                        let mode: FrameId = parse(m, "`mode`")?;
                        if call.args["push"] == true {
                            Request::push(mode)
                        } else {
                            Request::set(mode)
                        }
                    }
                    None => Request::ClearMode {
                        group: parse(&call.args["group"], "`group`")?,
                    },
                };
                outcome_json(request(call.app, Envelope::new(req), call.origin))
            },
        )?
        .command(
            CommandSpec::new(CMD_HOLD, "Hold the context stack", category)
                .description("Holds groups of the stack (`groups`, empty: all of it; `place` for places) with a `reason`: transitions touching them are refused until released. Its token.")
                .args(json!({ "groups": ["orientation"], "reason": "an agent holds the image" })),
            |call| {
                let groups: Vec<GroupId> = match call.args.get("groups") {
                    Some(g) if !g.is_null() => parse(g, "`groups`")?,
                    _ => Vec::new(),
                };
                let reason = call.args["reason"].as_str().unwrap_or("held");
                let token = hold(call.app, groups, reason, &source_of(call.origin))?;
                Ok(json!({ "token": token }))
            },
        )?
        .command(
            CommandSpec::new(CMD_RELEASE, "Release a hold", category)
                .description("Releases a hold of the context stack (`token`).")
                .args(json!({ "token": 1 })),
            |call| {
                let token = call.args["token"].as_u64().ok_or("`token` is missing")?;
                Ok(json!({ "released": release(call.app, token) }))
            },
        )?;
        // Escape pops a pushed mode, unless an overlay or a field wants it.
        ctx.keybinding(
            "Escape",
            CMD_BACK,
            "modePushed && !overlayOpen",
            Layer::Default,
        )?;
        let (w1, w2, w3, w4) = (
            ctx.app().downgrade(),
            ctx.app().downgrade(),
            ctx.app().downgrade(),
            ctx.app().downgrade(),
        );
        ctx.collector(
            FnCollector::new(
                "contexts",
                "The context stack: the owner's state (places, modes, revision), its holds and frames; the projection (its key, the context keys it set, the layouts and view states kept); the last context events; warnings: the stack's own keys set by hand (`hand_writes`), frames that can be on together binding the same keys (`clashes`). `request` asks for a transition as an agent; `hold` and `release`.",
            )
            .snapshot(move |_| match w1.get() {
                Ok(app) => describe(&app),
                Err(_) => Json::Null,
            })
            .action(
                "request",
                "Asks the owner for a transition (`request`, `if_rev`, `dry_run`), as an agent.",
                true,
                move |_, args| {
                    let app = w2.get()?;
                    let env: Envelope = parse(args, "a context request")?;
                    outcome_json(request(&app, env, &Origin::Agent("debug-bridge".into())))
                },
            )
            .args(json!({ "request": { "op": "back" }, "dry_run": true }))
            .action(
                "hold",
                "Holds groups of the stack (`groups`, `reason`); its token.",
                true,
                move |_, args| {
                    let app = w3.get()?;
                    let groups: Vec<GroupId> = match args.get("groups") {
                        Some(g) if !g.is_null() => parse(g, "`groups`")?,
                        _ => Vec::new(),
                    };
                    let reason = args["reason"].as_str().unwrap_or("held by an agent");
                    Ok(json!({ "token": hold(&app, groups, reason, "agent:debug-bridge")? }))
                },
            )
            .args(json!({ "groups": [], "reason": "an agent holds the stack" }))
            .action("release", "Releases a hold (`token`).", true, move |_, args| {
                let app = w4.get()?;
                let token = args["token"].as_u64().ok_or("`token` is missing")?;
                Ok(json!({ "released": release(&app, token) }))
            })
            .args(json!({ "token": 1 })),
        )?;
        ctx.on(On::Started, |app, _| {
            start(app);
            report_clashes(app);
        })
        .on(On::Pass, |app, _| pass(app))
        .on(On::Rebuilt, |app, _| {
            if let (Some(svc), Some(reg)) =
                (app.service(&CONTEXTS), with_owner(app, |c| c.registry()))
            {
                lock(&svc.projection).registry = reg;
            }
            report_clashes(app);
        })
        .on(On::Disconnected, |app, ev| {
            if let Lifecycle::Disconnected(p) = ev {
                with_owner(app, |c| c.remove_owner(&OwnerId::from(p)));
            }
        });
        ctx.after(STEP_UNDO, |app, _, result| undone(app, false, result))
            .after(STEP_REDO, |app, _, result| undone(app, true, result));
        Ok(())
    }
}

impl Ctx<'_> {
    /// Declares a frame of the context stack, owned by this plugin (gone
    /// with it). The plugin requires [`PLUGIN`].
    pub fn frame(&mut self, spec: FrameSpec) -> Result<&mut Self, CtxError> {
        let owner = self.owner.clone();
        let app = self.app.clone();
        with_owner(&app, |c| c.declare(&owner, spec))
            .ok_or_else(|| CtxError::Frame(NO_STACK.into()))?
            .map_err(CtxError::Frame)?;
        Ok(self)
    }

    /// Binds keys to a command while a frame is on: the frame's own
    /// keymap layer, through its `when` clause (`mode.<group> == 'id'`,
    /// `place == 'id'`), winning over a binding without a clause.
    pub fn frame_keybinding(
        &mut self,
        frame: &FrameSpec,
        keys: &str,
        command: CommandId,
    ) -> Result<&mut Self, CtxError> {
        self.keybinding(keys, command, &frame.clause(), Layer::Default)
    }
}
