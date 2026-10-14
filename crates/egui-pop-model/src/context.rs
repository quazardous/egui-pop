//! The context stack (ADR 0012): where the user is and how the
//! application behaves right now, without egui.
//!
//! - **Places** are drill-downs (an album, a group, a sub-workspace):
//!   a path from the root, shown as a breadcrumb.
//! - **Modes** are global states (an orientation, compare, read-only):
//!   at most one per **group**, modes of different groups combine. A mode
//!   is **set** (sticky until changed) or **pushed** (temporary: popping
//!   it puts back what it replaced in its group).
//! - A frame may be **transient**: on the live stack, never restored.
//!
//! Frames are declared in a [`FrameRegistry`]; the [`State`] is the place
//! path, the active modes and a revision. [`apply`] is the pure
//! transition function (guards, holds, `if_rev`, redirections) and
//! [`restore`] brings a saved state back against today's frames. The
//! owner of the stack (the application's kernel, or egui-pop's
//! `LocalContexts`) calls them; nothing here does I/O.
//!
//! ```
//! use egui_pop_model::context::{self, Envelope, FrameRegistry, FrameSpec, Request, State};
//! use egui_pop_model::ids::{FrameId, GroupId};
//!
//! const ALBUM: FrameId = FrameId::from_static("demo.album");
//! const CULLING: FrameId = FrameId::from_static("demo.culling");
//! let mut frames = FrameRegistry::new();
//! frames.insert(FrameSpec::place(ALBUM, "Album")).unwrap();
//! frames
//!     .insert(FrameSpec::mode(CULLING, GroupId::from_static("orientation"), "Culling"))
//!     .unwrap();
//!
//! let s = State::default();
//! let t = context::apply(&s, &frames, &[], &Envelope::new(Request::enter(ALBUM))).unwrap();
//! assert_eq!(t.state.place(), "demo.album");
//! let t = context::apply(&t.state, &frames, &[], &Envelope::new(Request::set(CULLING))).unwrap();
//! assert_eq!(t.state.mode_of("orientation"), Some(&CULLING));
//! assert_eq!(t.state.rev, 2);
//! ```

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::doc::{Schema, same};
use crate::ids::{FrameId, GroupId, PanelId};
use crate::layout::Layout;

/// The place at the bottom of every path, as the `place` context key
/// says it.
pub const ROOT: &str = "root";

/// The group a hold names to stop place transitions (enter, leave,
/// jump, go).
pub const PLACES: GroupId = GroupId::from_static("place");

/// How many redirections a request may go through.
const HOPS: usize = 4;

/// What a frame is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A drill-down: entered, left.
    Place,
    /// A global state: set, pushed, cleared.
    Mode,
}

/// How a mode is on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Life {
    /// Sticky until changed.
    #[default]
    Set,
    /// Temporary: popped by `nav.back` (Escape), putting back what it
    /// replaced in its group.
    Pushed,
}

/// What a guard decides.
#[derive(Clone, Debug, PartialEq)]
pub enum Verdict {
    Go,
    /// The transition does not happen, for this reason.
    Refuse(String),
    /// Another transition happens instead.
    Redirect(Request),
}

/// What a guard is asked about.
#[derive(Debug)]
pub struct GuardCall<'a> {
    pub frame: &'a FrameId,
    pub payload: &'a Json,
    /// Entering (`can_enter`) or leaving (`can_leave`).
    pub entering: bool,
    /// The state before the transition.
    pub state: &'a State,
}

/// A guard on a frame: `can_enter` or `can_leave`, evaluated by the
/// owner of the stack.
pub type Guard = Arc<dyn Fn(&GuardCall<'_>) -> Verdict + Send + Sync>;

/// A frame as declared.
#[derive(Clone, Serialize)]
pub struct FrameSpec {
    pub id: FrameId,
    pub kind: Kind,
    /// A mode's group (one mode of a group at a time); none for a place.
    pub group: Option<GroupId>,
    /// For the breadcrumb and the mode chips, in English; the interface
    /// translates it through the key derived from the id,
    /// `frame-<id>.title` (`egui_pop_core::strings::frame_title`).
    pub title: String,
    /// An icon's name, for the chips and crumbs (`egui_pop_core::icon`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// On the live stack, never restored.
    pub transient: bool,
    /// Context keys the frame sets while it is on (the top place's, every
    /// active mode's).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<(String, String)>,
    /// A place: the arrangement (a template or a saved name) its
    /// projection starts from the first time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arrangement: Option<String>,
    /// A mode: panels it shows while on.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub show: Vec<PanelId>,
    /// A mode: panels it hides while on.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hide: Vec<PanelId>,
    #[serde(skip)]
    pub can_enter: Option<Guard>,
    #[serde(skip)]
    pub can_leave: Option<Guard>,
}

impl fmt::Debug for FrameSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FrameSpec")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .field("group", &self.group)
            .field("transient", &self.transient)
            .field(
                "guarded",
                &(self.can_enter.is_some() || self.can_leave.is_some()),
            )
            .finish_non_exhaustive()
    }
}

impl FrameSpec {
    fn new(id: FrameId, kind: Kind, group: Option<GroupId>, title: &str) -> Self {
        Self {
            id,
            kind,
            group,
            title: title.into(),
            icon: None,
            transient: false,
            keys: Vec::new(),
            arrangement: None,
            show: Vec::new(),
            hide: Vec::new(),
            can_enter: None,
            can_leave: None,
        }
    }

    /// A place.
    pub fn place(id: FrameId, title: &str) -> Self {
        Self::new(id, Kind::Place, None, title)
    }

    /// A mode of a group.
    pub fn mode(id: FrameId, group: GroupId, title: &str) -> Self {
        Self::new(id, Kind::Mode, Some(group), title)
    }

    pub fn icon(mut self, icon: &str) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// Never restored (an agent driving, a modal tool mode).
    pub fn transient(mut self) -> Self {
        self.transient = true;
        self
    }

    /// A context key set while the frame is on.
    pub fn key(mut self, key: &str, value: &str) -> Self {
        self.keys.push((key.into(), value.into()));
        self
    }

    /// The arrangement a place's projection starts from.
    pub fn arrangement(mut self, name: &str) -> Self {
        self.arrangement = Some(name.into());
        self
    }

    /// A panel a mode shows.
    pub fn show(mut self, panel: PanelId) -> Self {
        self.show.push(panel);
        self
    }

    /// A panel a mode hides.
    pub fn hide(mut self, panel: PanelId) -> Self {
        self.hide.push(panel);
        self
    }

    /// Whether the frame may be entered.
    pub fn can_enter(
        mut self,
        f: impl Fn(&GuardCall<'_>) -> Verdict + Send + Sync + 'static,
    ) -> Self {
        self.can_enter = Some(Arc::new(f));
        self
    }

    /// Whether the frame may be left.
    pub fn can_leave(
        mut self,
        f: impl Fn(&GuardCall<'_>) -> Verdict + Send + Sync + 'static,
    ) -> Self {
        self.can_leave = Some(Arc::new(f));
        self
    }

    /// The `when` clause that holds while the frame is on: `place ==
    /// 'id'` for a place (the top one), `mode.<group> == 'id'` for a
    /// mode. A frame's own key bindings use it (its keymap layer).
    pub fn clause(&self) -> String {
        match (&self.kind, &self.group) {
            (Kind::Mode, Some(g)) => format!("mode.{g} == '{}'", self.id),
            _ => format!("place == '{}'", self.id),
        }
    }
}

/// The frames that exist.
#[derive(Clone, Debug, Default)]
pub struct FrameRegistry {
    frames: BTreeMap<FrameId, FrameSpec>,
}

impl FrameRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Declares a frame; refused when its id is taken, or when a mode has
    /// no group.
    pub fn insert(&mut self, spec: FrameSpec) -> Result<(), String> {
        if self.frames.contains_key(&spec.id) {
            return Err(format!("frame `{}` is already declared", spec.id));
        }
        if spec.kind == Kind::Mode && spec.group.is_none() {
            return Err(format!("mode `{}` has no group", spec.id));
        }
        self.frames.insert(spec.id.clone(), spec);
        Ok(())
    }

    pub fn get(&self, id: &FrameId) -> Option<&FrameSpec> {
        self.frames.get(id)
    }

    pub fn remove(&mut self, id: &FrameId) -> Option<FrameSpec> {
        self.frames.remove(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &FrameSpec> {
        self.frames.values()
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Every mode group, with its modes in id order.
    pub fn groups(&self) -> BTreeMap<GroupId, Vec<&FrameSpec>> {
        let mut out: BTreeMap<GroupId, Vec<&FrameSpec>> = BTreeMap::new();
        for f in self.frames.values() {
            if let (Kind::Mode, Some(g)) = (f.kind, &f.group) {
                out.entry(g.clone()).or_default().push(f);
            }
        }
        out
    }
}

/// A place on the path, with its payload (the application's own: a
/// sub-workspace's id).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub frame: FrameId,
    #[serde(default, skip_serializing_if = "Json::is_null")]
    pub payload: Json,
}

impl Entry {
    pub fn new(frame: FrameId) -> Self {
        Self {
            frame,
            payload: Json::Null,
        }
    }
}

/// An active mode.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModeEntry {
    pub frame: FrameId,
    pub group: GroupId,
    #[serde(default)]
    pub life: Life,
    #[serde(default, skip_serializing_if = "Json::is_null")]
    pub payload: Json,
    /// What a pushed mode replaced in its group, put back when it pops.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub under: Option<Box<ModeEntry>>,
}

/// The stack: the place path, the active modes, a revision.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct State {
    /// Incremented by every transition.
    pub rev: u64,
    /// The places from the root's child to the top.
    #[serde(default)]
    pub places: Vec<Entry>,
    /// The active modes, at most one per group, in the order they came.
    #[serde(default)]
    pub modes: Vec<ModeEntry>,
}

impl State {
    /// The top place's id, [`ROOT`] at the root.
    pub fn place(&self) -> &str {
        self.places.last().map_or(ROOT, |e| e.frame.as_str())
    }

    /// How many places deep (0 at the root).
    pub fn depth(&self) -> usize {
        self.places.len()
    }

    /// The mode on in a group.
    pub fn mode_of(&self, group: &str) -> Option<&FrameId> {
        self.modes
            .iter()
            .find(|m| m.group.as_str() == group)
            .map(|m| &m.frame)
    }

    /// Whether a frame is on (a place on the path or an active mode).
    pub fn is_on(&self, frame: &FrameId) -> bool {
        self.places.iter().any(|e| e.frame == *frame)
            || self.modes.iter().any(|m| m.frame == *frame)
    }

    /// Whether a pushed mode is on (what `nav.back` pops first).
    pub fn has_pushed(&self) -> bool {
        self.modes.iter().any(|m| m.life == Life::Pushed)
    }

    /// The place path as a key: `root/demo.album/demo.photo`, payloads
    /// included when they are not null (`demo.album=3`).
    pub fn path_key(&self) -> String {
        let mut key = ROOT.to_string();
        for e in &self.places {
            key.push('/');
            key.push_str(e.frame.as_str());
            if !e.payload.is_null() {
                key.push('=');
                key.push_str(&e.payload.to_string());
            }
        }
        key
    }

    /// The projection's key: the path, then the active modes by group
    /// (`root/demo.album|orientation=demo.culling`). A layout change made
    /// in a mode belongs to this key, so leaving the mode reverts it.
    pub fn projection_key(&self) -> String {
        let mut modes: Vec<(&str, &str)> = self
            .modes
            .iter()
            .map(|m| (m.group.as_str(), m.frame.as_str()))
            .collect();
        modes.sort();
        let mut key = self.path_key();
        for (g, f) in modes {
            key.push('|');
            key.push_str(g);
            key.push('=');
            key.push_str(f);
        }
        key
    }

    /// The context keys the stack sets: `place`, `placeDepth`,
    /// `modePushed`, `mode.<group>` for every group of the registry (empty
    /// when none is on), and the keys of the top place and active modes.
    pub fn when_keys(&self, registry: &FrameRegistry) -> Vec<(String, String)> {
        let mut out = vec![
            ("place".to_string(), self.place().to_string()),
            ("placeDepth".to_string(), self.depth().to_string()),
            (
                "modePushed".to_string(),
                if self.has_pushed() { "true" } else { "false" }.to_string(),
            ),
        ];
        let mut groups: Vec<String> = registry.groups().keys().map(|g| g.to_string()).collect();
        for m in &self.modes {
            if !groups.iter().any(|g| g == m.group.as_str()) {
                groups.push(m.group.to_string());
            }
        }
        for g in groups {
            let on = self.mode_of(&g).map(|f| f.to_string()).unwrap_or_default();
            out.push((format!("mode.{g}"), on));
        }
        let on = self
            .places
            .last()
            .map(|e| &e.frame)
            .into_iter()
            .chain(self.modes.iter().map(|m| &m.frame));
        for f in on {
            if let Some(spec) = registry.get(f) {
                out.extend(spec.keys.iter().cloned());
            }
        }
        out
    }
}

/// A transition asked of the owner.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// Enters a place under the top one.
    Enter {
        frame: FrameId,
        #[serde(default, skip_serializing_if = "Json::is_null")]
        payload: Json,
    },
    /// Leaves the top place.
    Leave,
    /// Leaves places until `depth` are left (0: the root).
    Jump { depth: usize },
    /// Goes to a place path (a crumb from elsewhere, "Go there").
    Go {
        #[serde(default)]
        places: Vec<Entry>,
    },
    /// Sets a mode (sticky), replacing its group's.
    SetMode {
        mode: FrameId,
        #[serde(default, skip_serializing_if = "Json::is_null")]
        payload: Json,
    },
    /// Pushes a mode (temporary), replacing its group's until popped.
    PushMode {
        mode: FrameId,
        #[serde(default, skip_serializing_if = "Json::is_null")]
        payload: Json,
    },
    /// Pops the last pushed mode.
    PopMode,
    /// Turns a group's mode off.
    ClearMode { group: GroupId },
    /// Back: pops the last pushed mode, else leaves the top place.
    Back,
}

impl Request {
    pub fn enter(frame: FrameId) -> Self {
        Request::Enter {
            frame,
            payload: Json::Null,
        }
    }

    pub fn set(mode: FrameId) -> Self {
        Request::SetMode {
            mode,
            payload: Json::Null,
        }
    }

    pub fn push(mode: FrameId) -> Self {
        Request::PushMode {
            mode,
            payload: Json::Null,
        }
    }

    /// The group a hold must not cover for this request to go.
    fn group(&self, registry: &FrameRegistry, state: &State) -> Option<GroupId> {
        match self {
            Request::Enter { .. } | Request::Leave | Request::Jump { .. } | Request::Go { .. } => {
                Some(PLACES)
            }
            Request::SetMode { mode, .. } | Request::PushMode { mode, .. } => {
                registry.get(mode).and_then(|s| s.group.clone())
            }
            Request::PopMode => state
                .modes
                .iter()
                .rev()
                .find(|m| m.life == Life::Pushed)
                .map(|m| m.group.clone()),
            Request::ClearMode { group } => Some(group.clone()),
            Request::Back => None,
        }
    }
}

/// A request with its conditions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    pub request: Request,
    /// Refused as stale unless the state is still at this revision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub if_rev: Option<u64>,
    /// Checked, answered, not applied.
    #[serde(default)]
    pub dry_run: bool,
}

impl Envelope {
    pub fn new(request: Request) -> Self {
        Self {
            request,
            if_rev: None,
            dry_run: false,
        }
    }

    pub fn if_rev(mut self, rev: u64) -> Self {
        self.if_rev = Some(rev);
        self
    }

    pub fn dry_run(mut self) -> Self {
        self.dry_run = true;
        self
    }
}

/// A short lock on groups of the stack: a drag in progress, a batch
/// writing, an agent holding the image. Requests touching a held group
/// are refused with its reason.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hold {
    pub token: u64,
    /// The groups held ([`PLACES`] for places); empty: the whole stack.
    #[serde(default)]
    pub groups: Vec<GroupId>,
    pub reason: String,
    /// Who holds it (`user`, `agent:<name>`, a plugin).
    pub by: String,
}

impl Hold {
    /// Whether it holds a group.
    pub fn covers(&self, group: &GroupId) -> bool {
        self.groups.is_empty() || self.groups.contains(group)
    }
}

/// Why a transition did not happen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RefusalKind {
    /// No such frame, or not of the right kind.
    Unknown,
    /// A guard said no.
    Guard,
    /// A hold covers the group.
    Held,
    /// `if_rev` no longer matches.
    Stale,
    /// Nothing to do (leaving the root, setting the mode that is on).
    Nothing,
    /// Redirected in a loop.
    Loop,
}

/// A refusal, with its reason and who refused.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Refusal {
    pub kind: RefusalKind,
    pub reason: String,
    /// A frame's guard, a hold's holder, `stack`.
    pub by: String,
    /// The revision the state is at.
    pub rev: u64,
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.reason, self.by)
    }
}

impl std::error::Error for Refusal {}

/// A transition applied (or, for a dry run, that would be).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Transition {
    pub state: State,
    /// Frames turned on, in order.
    pub entered: Vec<FrameId>,
    /// Frames turned off, top first.
    pub left: Vec<FrameId>,
    /// The request applied instead, when a guard redirected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirected: Option<Request>,
}

fn refuse(kind: RefusalKind, reason: impl Into<String>, by: &str, state: &State) -> Refusal {
    Refusal {
        kind,
        reason: reason.into(),
        by: by.into(),
        rev: state.rev,
    }
}

fn spec_of<'a>(
    registry: &'a FrameRegistry,
    frame: &FrameId,
    kind: Kind,
    state: &State,
) -> Result<&'a FrameSpec, Refusal> {
    match registry.get(frame) {
        Some(s) if s.kind == kind => Ok(s),
        Some(_) => Err(refuse(
            RefusalKind::Unknown,
            format!(
                "`{frame}` is not a {}",
                if kind == Kind::Place { "place" } else { "mode" }
            ),
            "stack",
            state,
        )),
        None => Err(refuse(
            RefusalKind::Unknown,
            format!("no frame `{frame}`"),
            "stack",
            state,
        )),
    }
}

/// What a guard said: go on, or another request.
enum Checked {
    Go,
    Redirect(Request),
}

fn guard(
    guard: &Option<Guard>,
    frame: &FrameId,
    payload: &Json,
    entering: bool,
    state: &State,
) -> Result<Checked, Refusal> {
    let Some(g) = guard else {
        return Ok(Checked::Go);
    };
    match g(&GuardCall {
        frame,
        payload,
        entering,
        state,
    }) {
        Verdict::Go => Ok(Checked::Go),
        Verdict::Refuse(reason) => Err(refuse(RefusalKind::Guard, reason, frame.as_str(), state)),
        Verdict::Redirect(r) => Ok(Checked::Redirect(r)),
    }
}

/// Checks a guard; on a redirection, returns early with the redirected
/// transition.
macro_rules! check {
    ($g:expr, $frame:expr, $payload:expr, $entering:expr, $state:expr, $reg:expr, $holds:expr, $hops:expr) => {
        if let Checked::Redirect(r) = guard($g, $frame, $payload, $entering, $state)? {
            return redirect($state, $reg, $holds, r, $hops);
        }
    };
}

fn redirect(
    state: &State,
    registry: &FrameRegistry,
    holds: &[Hold],
    r: Request,
    hops: usize,
) -> Result<Transition, Refusal> {
    if hops >= HOPS {
        return Err(refuse(
            RefusalKind::Loop,
            "redirected too many times",
            "stack",
            state,
        ));
    }
    let mut t = step(state, registry, holds, &r, hops + 1)?;
    t.redirected.get_or_insert(r);
    Ok(t)
}

/// Applies a request to a state: the new state and what was entered and
/// left, or why not. Pure: the owner writes and publishes.
pub fn apply(
    state: &State,
    registry: &FrameRegistry,
    holds: &[Hold],
    envelope: &Envelope,
) -> Result<Transition, Refusal> {
    if let Some(rev) = envelope.if_rev
        && rev != state.rev
    {
        return Err(refuse(
            RefusalKind::Stale,
            format!("the stack is at revision {}, not {rev}", state.rev),
            "stack",
            state,
        ));
    }
    step(state, registry, holds, &envelope.request, 0)
}

fn held(
    request: &Request,
    registry: &FrameRegistry,
    holds: &[Hold],
    state: &State,
) -> Result<(), Refusal> {
    let Some(group) = request.group(registry, state) else {
        return Ok(());
    };
    match holds.iter().find(|h| h.covers(&group)) {
        Some(h) => Err(refuse(RefusalKind::Held, h.reason.clone(), &h.by, state)),
        None => Ok(()),
    }
}

fn step(
    state: &State,
    registry: &FrameRegistry,
    holds: &[Hold],
    request: &Request,
    hops: usize,
) -> Result<Transition, Refusal> {
    let nothing = |why: &str| Err(refuse(RefusalKind::Nothing, why, "stack", state));
    if let Request::Back = request {
        return if state.has_pushed() {
            step(state, registry, holds, &Request::PopMode, hops)
        } else if !state.places.is_empty() {
            step(state, registry, holds, &Request::Leave, hops)
        } else {
            nothing("there is nothing to go back to")
        };
    }
    held(request, registry, holds, state)?;
    let mut next = state.clone();
    let mut entered = Vec::new();
    let mut left = Vec::new();
    match request {
        Request::Enter { frame, payload } => {
            let spec = spec_of(registry, frame, Kind::Place, state)?;
            check!(
                &spec.can_enter,
                frame,
                payload,
                true,
                state,
                registry,
                holds,
                hops
            );
            next.places.push(Entry {
                frame: frame.clone(),
                payload: payload.clone(),
            });
            entered.push(frame.clone());
        }
        Request::Leave => {
            let Some(top) = state.places.last() else {
                return nothing("the root has no parent");
            };
            if let Some(spec) = registry.get(&top.frame) {
                check!(
                    &spec.can_leave,
                    &top.frame,
                    &top.payload,
                    false,
                    state,
                    registry,
                    holds,
                    hops
                );
            }
            next.places.pop();
            left.push(top.frame.clone());
        }
        Request::Jump { depth } => {
            if *depth >= state.places.len() {
                return nothing("already there");
            }
            for e in state.places[*depth..].iter().rev() {
                if let Some(spec) = registry.get(&e.frame) {
                    check!(
                        &spec.can_leave,
                        &e.frame,
                        &e.payload,
                        false,
                        state,
                        registry,
                        holds,
                        hops
                    );
                }
                left.push(e.frame.clone());
            }
            next.places.truncate(*depth);
        }
        Request::Go { places } => {
            let common = state
                .places
                .iter()
                .zip(places)
                .take_while(|(a, b)| a == b)
                .count();
            if common == state.places.len() && common == places.len() {
                return nothing("already there");
            }
            for e in state.places[common..].iter().rev() {
                if let Some(spec) = registry.get(&e.frame) {
                    check!(
                        &spec.can_leave,
                        &e.frame,
                        &e.payload,
                        false,
                        state,
                        registry,
                        holds,
                        hops
                    );
                }
                left.push(e.frame.clone());
            }
            for e in &places[common..] {
                let spec = spec_of(registry, &e.frame, Kind::Place, state)?;
                check!(
                    &spec.can_enter,
                    &e.frame,
                    &e.payload,
                    true,
                    state,
                    registry,
                    holds,
                    hops
                );
                entered.push(e.frame.clone());
            }
            next.places = places.clone();
        }
        Request::SetMode { mode, payload } | Request::PushMode { mode, payload } => {
            let pushed = matches!(request, Request::PushMode { .. });
            let spec = spec_of(registry, mode, Kind::Mode, state)?;
            let group = spec.group.clone().expect("a mode has a group");
            let at = state.modes.iter().position(|m| m.group == group);
            let life = if pushed { Life::Pushed } else { Life::Set };
            if let Some(i) = at {
                let old = &state.modes[i];
                if old.frame == *mode && old.life == life && old.payload == *payload {
                    return nothing("this mode is on already");
                }
                if old.frame != *mode
                    && let Some(old_spec) = registry.get(&old.frame)
                {
                    check!(
                        &old_spec.can_leave,
                        &old.frame,
                        &old.payload,
                        false,
                        state,
                        registry,
                        holds,
                        hops
                    );
                }
            }
            check!(
                &spec.can_enter,
                mode,
                payload,
                true,
                state,
                registry,
                holds,
                hops
            );
            let old = at.map(|i| next.modes.remove(i));
            if let Some(o) = &old {
                left.push(o.frame.clone());
            }
            let entry = ModeEntry {
                frame: mode.clone(),
                group,
                life,
                payload: payload.clone(),
                under: if pushed { old.map(Box::new) } else { None },
            };
            match at {
                Some(i) => next.modes.insert(i, entry),
                None => next.modes.push(entry),
            }
            entered.push(mode.clone());
        }
        Request::PopMode => {
            let Some(i) = state.modes.iter().rposition(|m| m.life == Life::Pushed) else {
                return nothing("no mode is pushed");
            };
            let m = &state.modes[i];
            if let Some(spec) = registry.get(&m.frame) {
                check!(
                    &spec.can_leave,
                    &m.frame,
                    &m.payload,
                    false,
                    state,
                    registry,
                    holds,
                    hops
                );
            }
            let m = next.modes.remove(i);
            left.push(m.frame.clone());
            if let Some(under) = m.under {
                entered.push(under.frame.clone());
                next.modes.insert(i, *under);
            }
        }
        Request::ClearMode { group } => {
            let Some(i) = state.modes.iter().position(|m| m.group == *group) else {
                return nothing("no mode of this group is on");
            };
            let m = &state.modes[i];
            if let Some(spec) = registry.get(&m.frame) {
                check!(
                    &spec.can_leave,
                    &m.frame,
                    &m.payload,
                    false,
                    state,
                    registry,
                    holds,
                    hops
                );
            }
            left.push(next.modes.remove(i).frame);
        }
        Request::Back => unreachable!("handled above"),
    }
    next.rev = state.rev + 1;
    Ok(Transition {
        state: next,
        entered,
        left,
        redirected: None,
    })
}

/// A frame dropped on restore, and why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dropped {
    pub frame: FrameId,
    pub why: String,
}

/// Brings a saved state back against today's frames: a place that is
/// unknown, transient or refused by `valid` (its payload no longer
/// exists) cuts the path there (its children go with it); a mode is
/// dropped alone (a pushed one gives its place back to what it
/// replaced). Restoring never fails; the revision is kept.
pub fn restore(
    saved: &State,
    registry: &FrameRegistry,
    valid: &dyn Fn(&FrameId, &Json) -> Option<String>,
) -> (State, Vec<Dropped>) {
    let check = |frame: &FrameId, payload: &Json, kind: Kind| -> Option<String> {
        match registry.get(frame) {
            None => Some("it is not declared any more".into()),
            Some(s) if s.kind != kind => Some("it is not of the same kind any more".into()),
            Some(s) if s.transient => Some("it is transient".into()),
            Some(_) => valid(frame, payload),
        }
    };
    let mut out = State {
        rev: saved.rev,
        ..State::default()
    };
    let mut dropped = Vec::new();
    let mut cut = false;
    for e in &saved.places {
        if cut {
            dropped.push(Dropped {
                frame: e.frame.clone(),
                why: "a place above it was dropped".into(),
            });
            continue;
        }
        match check(&e.frame, &e.payload, Kind::Place) {
            None => out.places.push(e.clone()),
            Some(why) => {
                dropped.push(Dropped {
                    frame: e.frame.clone(),
                    why,
                });
                cut = true;
            }
        }
    }
    for m in &saved.modes {
        let mut current = Some(m.clone());
        while let Some(mut entry) = current.take() {
            let mut why = check(&entry.frame, &entry.payload, Kind::Mode);
            if why.is_none() {
                let spec_group = registry.get(&entry.frame).and_then(|s| s.group.clone());
                if spec_group.as_ref() != Some(&entry.group) {
                    why = Some("its group changed".into());
                } else if out.modes.iter().any(|o| o.group == entry.group) {
                    why = Some("another mode of its group is on".into());
                }
            }
            match why {
                None => {
                    // What it replaced comes back only if it is valid.
                    if let Some(under) = entry.under.take() {
                        let ok = check(&under.frame, &under.payload, Kind::Mode).is_none()
                            && registry.get(&under.frame).and_then(|s| s.group.clone())
                                == Some(entry.group.clone());
                        if ok {
                            entry.under = Some(under);
                        } else {
                            dropped.push(Dropped {
                                frame: under.frame.clone(),
                                why: "what it replaced is not valid any more".into(),
                            });
                        }
                    }
                    out.modes.push(entry);
                }
                Some(why) => {
                    dropped.push(Dropped {
                        frame: entry.frame.clone(),
                        why,
                    });
                    current = entry.under.map(|u| *u);
                }
            }
        }
    }
    (out, dropped)
}

/// The `context` document: the stack as its owner writes it at each
/// transition.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ContextDoc {
    pub state: State,
}

/// The `context` document's schema.
pub fn schema() -> Schema {
    Schema::new("context", 1).migration(0, same)
}

/// What the projection keeps for one projection key: the focused panel
/// and the values components remember there (a list's scroll offset).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ViewState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus: Option<PanelId>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub values: BTreeMap<String, Json>,
}

/// The `view` document: the projection's layouts and view states, by
/// projection key ([`State::projection_key`]).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ViewDoc {
    #[serde(default)]
    pub layouts: BTreeMap<String, Layout>,
    #[serde(default)]
    pub views: BTreeMap<String, ViewState>,
}

/// The `view` document's schema.
pub fn view_schema() -> Schema {
    Schema::new("view", 1).migration(0, same)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const ALBUM: FrameId = FrameId::from_static("t.album");
    const PHOTO: FrameId = FrameId::from_static("t.photo");
    const MASK: FrameId = FrameId::from_static("t.mask");
    const LOCKED: FrameId = FrameId::from_static("t.locked");
    const CULLING: FrameId = FrameId::from_static("t.culling");
    const REVIEW: FrameId = FrameId::from_static("t.review");
    const COMPARE: FrameId = FrameId::from_static("t.compare");
    const AGENT: FrameId = FrameId::from_static("t.agent");
    const ORIENTATION: GroupId = GroupId::from_static("orientation");
    const VIEW: GroupId = GroupId::from_static("view");

    fn registry() -> FrameRegistry {
        let mut r = FrameRegistry::new();
        r.insert(FrameSpec::place(ALBUM, "Album").key("albumOpen", "true"))
            .unwrap();
        r.insert(FrameSpec::place(PHOTO, "Photo").can_leave(|c| {
            if c.payload == &json!("dirty") {
                Verdict::Refuse("unsaved changes".into())
            } else {
                Verdict::Go
            }
        }))
        .unwrap();
        r.insert(FrameSpec::place(MASK, "Mask")).unwrap();
        // Entering it lands on the album instead.
        r.insert(
            FrameSpec::place(LOCKED, "Locked")
                .can_enter(|_| Verdict::Redirect(Request::enter(ALBUM))),
        )
        .unwrap();
        r.insert(FrameSpec::mode(CULLING, ORIENTATION, "Culling"))
            .unwrap();
        r.insert(FrameSpec::mode(REVIEW, ORIENTATION, "Review"))
            .unwrap();
        r.insert(FrameSpec::mode(COMPARE, VIEW, "Compare").show(PanelId::from_static("t.second")))
            .unwrap();
        r.insert(FrameSpec::mode(AGENT, GroupId::from_static("driver"), "Agent").transient())
            .unwrap();
        r
    }

    fn go(s: &State, r: &FrameRegistry, req: Request) -> State {
        apply(s, r, &[], &Envelope::new(req)).unwrap().state
    }

    #[test]
    fn places_stack_and_leave_with_their_guards() {
        let r = registry();
        let s = go(&State::default(), &r, Request::enter(ALBUM));
        let s = go(&s, &r, Request::enter(PHOTO));
        let s = go(&s, &r, Request::enter(MASK));
        assert_eq!(s.depth(), 3);
        assert_eq!(s.place(), "t.mask");
        assert_eq!(s.path_key(), "root/t.album/t.photo/t.mask");
        let t = apply(&s, &r, &[], &Envelope::new(Request::Jump { depth: 1 })).unwrap();
        assert_eq!(t.left, [MASK, PHOTO]);
        assert_eq!(t.state.place(), "t.album");
        // A guard keeps a dirty photo.
        let dirty = go(
            &State::default(),
            &r,
            Request::Enter {
                frame: PHOTO,
                payload: json!("dirty"),
            },
        );
        let e = apply(&dirty, &r, &[], &Envelope::new(Request::Leave)).unwrap_err();
        assert_eq!(e.kind, RefusalKind::Guard);
        assert_eq!(e.by, "t.photo");
        // Nothing above the root; a mode is not a place.
        assert_eq!(
            apply(&State::default(), &r, &[], &Envelope::new(Request::Leave))
                .unwrap_err()
                .kind,
            RefusalKind::Nothing
        );
        assert_eq!(
            apply(
                &State::default(),
                &r,
                &[],
                &Envelope::new(Request::enter(CULLING))
            )
            .unwrap_err()
            .kind,
            RefusalKind::Unknown
        );
        // Go: from one path to another, common prefix kept.
        let t = apply(
            &s,
            &r,
            &[],
            &Envelope::new(Request::Go {
                places: vec![Entry::new(ALBUM), Entry::new(MASK)],
            }),
        )
        .unwrap();
        assert_eq!(t.left, [MASK, PHOTO]);
        assert_eq!(t.entered, [MASK]);
        assert_eq!(t.state.path_key(), "root/t.album/t.mask");
    }

    #[test]
    fn a_guard_redirects() {
        let r = registry();
        let t = apply(
            &State::default(),
            &r,
            &[],
            &Envelope::new(Request::enter(LOCKED)),
        )
        .unwrap();
        assert_eq!(t.state.place(), "t.album");
        assert_eq!(t.redirected, Some(Request::enter(ALBUM)));
    }

    #[test]
    fn groups_are_exclusive_and_combine_across() {
        let r = registry();
        let s = go(&State::default(), &r, Request::set(CULLING));
        let t = apply(&s, &r, &[], &Envelope::new(Request::set(REVIEW))).unwrap();
        assert_eq!(t.left, [CULLING]);
        assert_eq!(t.entered, [REVIEW]);
        let s = go(&t.state, &r, Request::set(COMPARE));
        assert_eq!(s.mode_of("orientation"), Some(&REVIEW));
        assert_eq!(s.mode_of("view"), Some(&COMPARE));
        assert_eq!(
            s.projection_key(),
            "root|orientation=t.review|view=t.compare"
        );
        // Setting what is on: nothing to do.
        assert_eq!(
            apply(&s, &r, &[], &Envelope::new(Request::set(REVIEW)))
                .unwrap_err()
                .kind,
            RefusalKind::Nothing
        );
        let s = go(&s, &r, Request::ClearMode { group: VIEW });
        assert_eq!(s.mode_of("view"), None);
        let keys = s.when_keys(&r);
        assert!(keys.contains(&("mode.orientation".into(), "t.review".into())));
        assert!(keys.contains(&("mode.view".into(), String::new())));
    }

    #[test]
    fn pushed_modes_pop_back_to_what_they_replaced() {
        let r = registry();
        let s = go(&State::default(), &r, Request::set(CULLING));
        let s = go(&s, &r, Request::enter(ALBUM));
        let s = go(&s, &r, Request::push(REVIEW));
        assert!(s.has_pushed());
        assert_eq!(s.mode_of("orientation"), Some(&REVIEW));
        // Back pops the pushed mode first, then leaves places.
        let t = apply(&s, &r, &[], &Envelope::new(Request::Back)).unwrap();
        assert_eq!(t.left, [REVIEW]);
        assert_eq!(t.entered, [CULLING]);
        assert_eq!(t.state.mode_of("orientation"), Some(&CULLING));
        assert_eq!(t.state.place(), "t.album");
        let s = go(&t.state, &r, Request::Back);
        assert_eq!(s.place(), ROOT);
        // A set mode stays: nothing pushed, nothing to go back to.
        assert_eq!(
            apply(&s, &r, &[], &Envelope::new(Request::Back))
                .unwrap_err()
                .kind,
            RefusalKind::Nothing
        );
        assert_eq!(s.mode_of("orientation"), Some(&CULLING));
    }

    #[test]
    fn holds_refuse_with_their_reason_and_stale_revisions_too() {
        let r = registry();
        let hold = Hold {
            token: 1,
            groups: vec![ORIENTATION],
            reason: "a drag is in progress".into(),
            by: "user".into(),
        };
        let e = apply(
            &State::default(),
            &r,
            std::slice::from_ref(&hold),
            &Envelope::new(Request::set(CULLING)),
        )
        .unwrap_err();
        assert_eq!(e.kind, RefusalKind::Held);
        assert_eq!(e.reason, "a drag is in progress");
        // Other groups and places go on.
        assert!(
            apply(
                &State::default(),
                &r,
                std::slice::from_ref(&hold),
                &Envelope::new(Request::set(COMPARE))
            )
            .is_ok()
        );
        assert!(
            apply(
                &State::default(),
                &r,
                std::slice::from_ref(&hold),
                &Envelope::new(Request::enter(ALBUM))
            )
            .is_ok()
        );
        // A hold of everything.
        let all = Hold {
            groups: Vec::new(),
            ..hold
        };
        assert!(
            apply(
                &State::default(),
                &r,
                &[all],
                &Envelope::new(Request::enter(ALBUM))
            )
            .is_err()
        );
        let s = go(&State::default(), &r, Request::enter(ALBUM));
        let e = apply(&s, &r, &[], &Envelope::new(Request::Leave).if_rev(0)).unwrap_err();
        assert_eq!(e.kind, RefusalKind::Stale);
        assert!(apply(&s, &r, &[], &Envelope::new(Request::Leave).if_rev(1)).is_ok());
    }

    #[test]
    fn restore_cuts_at_the_first_invalid_place_and_drops_modes_alone() {
        let r = registry();
        let mut s = State::default();
        for req in [
            Request::enter(ALBUM),
            Request::Enter {
                frame: PHOTO,
                payload: json!(7),
            },
            Request::enter(MASK),
            Request::set(CULLING),
            Request::push(REVIEW),
            Request::set(AGENT),
            Request::push(COMPARE),
        ] {
            s = go(&s, &r, req);
        }
        // Everything valid, but the transient agent.
        let (back, dropped) = restore(&s, &r, &|_, _| None);
        assert_eq!(back.places, s.places);
        assert_eq!(back.rev, s.rev);
        assert_eq!(back.mode_of("orientation"), Some(&REVIEW));
        assert_eq!(back.mode_of("view"), Some(&COMPARE));
        assert_eq!(back.mode_of("driver"), None);
        assert_eq!(dropped.len(), 1);
        assert_eq!(dropped[0].frame, AGENT);
        // Photo 7 is gone: the path is cut there, the mask with it.
        let (back, dropped) = restore(&s, &r, &|f, p| {
            (*f == PHOTO && *p == json!(7)).then(|| "deleted".to_string())
        });
        assert_eq!(back.path_key(), "root/t.album");
        assert_eq!(dropped[0].why, "deleted");
        assert_eq!(dropped[1].frame, MASK);
        // Review is not declared any more: culling, which it replaced,
        // comes back.
        let mut fewer = registry();
        fewer.remove(&REVIEW);
        let (back, dropped) = restore(&s, &fewer, &|_, _| None);
        assert_eq!(back.mode_of("orientation"), Some(&CULLING));
        assert!(dropped.iter().any(|d| d.frame == REVIEW));
    }

    #[test]
    fn the_stack_round_trips_through_its_document() {
        let r = registry();
        let mut s = State::default();
        for req in [
            Request::Enter {
                frame: ALBUM,
                payload: json!({ "id": 3 }),
            },
            Request::set(CULLING),
            Request::push(REVIEW),
        ] {
            s = go(&s, &r, req);
        }
        let text = schema().save(&ContextDoc { state: s.clone() }).unwrap();
        let back = schema().load::<ContextDoc>(&text).unwrap().value;
        assert_eq!(back.state, s);
        let req: Request =
            serde_json::from_value(json!({ "op": "enter", "frame": "t.album" })).unwrap();
        assert_eq!(req, Request::enter(ALBUM));
        let env: Envelope =
            serde_json::from_value(json!({ "request": { "op": "back" }, "dry_run": true }))
                .unwrap();
        assert!(env.dry_run);
        let mut view = ViewDoc::default();
        view.views.insert(
            s.projection_key(),
            ViewState {
                focus: Some(PanelId::from_static("t.list")),
                values: [("scroll".to_string(), json!(120.0))].into(),
            },
        );
        let text = view_schema().save(&view).unwrap();
        assert_eq!(view_schema().load::<ViewDoc>(&text).unwrap().value, view);
    }

    #[test]
    fn a_frames_clause_names_its_key() {
        let r = registry();
        assert_eq!(r.get(&ALBUM).unwrap().clause(), "place == 't.album'");
        assert_eq!(
            r.get(&CULLING).unwrap().clause(),
            "mode.orientation == 't.culling'"
        );
        let w = crate::when::When::parse(&r.get(&CULLING).unwrap().clause()).unwrap();
        let s = go(&State::default(), &r, Request::set(CULLING));
        let mut cx = crate::when::WhenContext::new();
        for (k, v) in s.when_keys(&r) {
            cx.set(&k, &v);
        }
        assert!(w.eval(&cx));
        let s = go(&s, &r, Request::enter(ALBUM));
        assert!(
            s.when_keys(&r)
                .contains(&("albumOpen".into(), "true".into()))
        );
        assert!(
            FrameRegistry::new()
                .insert(FrameSpec {
                    group: None,
                    ..FrameSpec::mode(CULLING, ORIENTATION, "x")
                })
                .is_err()
        );
    }
}
