//! Undo (ADR 0014): histories owned by the kernel, routed by focus,
//! projected by the interface.
//!
//! - **The owner** implements [`History`]: its scopes (one history
//!   each: a document's edit, a collection's marks), their steps as
//!   [`StepInfo`]s, and the requests `undo`, `redo`, `jump` and `group`.
//!   The host gives it ([`Host::history`](crate::host::Host::history));
//!   without one, egui-pop's [`StoreHistory`] answers over the value
//!   store's application history ([`APP`]). The inverse of a step stays
//!   with the owner; the interface only reads and asks.
//! - **Local scopes** are a component's own history, kept in the
//!   interface and never persisted: the theme editor's draft, the
//!   settings window's values ([`StoreScope`] over the store's
//!   [`SETTINGS`] history). A component registers one for its focus
//!   surface ([`Ctx::local_scope`](crate::Ctx::local_scope)).
//! - **Focus picks the history** ([`target`]). Ctrl+Z, outside a text
//!   field (which keeps its own undo), undoes in the innermost focused
//!   surface that has a local scope; otherwise the newest step among the
//!   owner's scopes active in the window (a scope's focus targets drawn;
//!   none: always active). Redo goes back to the scope last undone.
//! - **Undo, redo and jump are steps** (ADR 0011): `edit.undo`,
//!   `edit.redo` and `edit.jump` ([`STEP_JUMP`]), their arguments naming
//!   the scope, so a before-hook vetoes one or redirects it. A gesture in
//!   progress in the scope refuses them. Each answer is published:
//!   `history.changed` (the scope, its revision and head) or
//!   `history.refused` (the scope, the kind and the reason); the owner's
//!   own moves (a new step) are published at the next pass.
//! - **Merging and groups**: writes with a merge key one second apart
//!   are one step (`values::Set::merge`), the focus leaving seals them;
//!   [`History::group`] opens an explicit group (an import, an agent's
//!   request) whose steps are one entry.
//!
//! ```
//! use egui_pop_api::App;
//! use egui_pop_api::history;
//! use egui_pop_core::lock;
//! use egui_pop_model::commands::Origin;
//! use egui_pop_model::ids::OwnerId;
//! use egui_pop_model::values::{Change, Descriptor, Set, Value};
//!
//! let app = App::builder().build(&egui::Context::default()).unwrap();
//! let h = app.handle();
//! let store = h.store();
//! lock(&store).declare(&OwnerId::USER, "doc/n", Descriptor::int("N", 0, 9, 0)).unwrap();
//! lock(&store).set(Set::new("doc/n", Change::To(Value::Int(3)), Origin::User)).unwrap();
//! assert!(history::can_undo(h));
//! assert!(history::undo(h, &Origin::User).ok());
//! assert_eq!(lock(&store).get("doc/n").unwrap().value, Value::Int(0));
//! assert_eq!(history::line(h, &history::APP).unwrap().redo.len(), 1);
//! ```

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::fmt;
use std::rc::Rc;

use egui_pop_core::binding::SharedStore;
use egui_pop_core::{focus, lock, strings};
use egui_pop_model::clock::ManualClock;
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::{OwnerId, StepId};
use egui_pop_model::values::{self, Store};
use serde::Serialize;
use serde_json::{Value as Json, json};

pub use egui_pop_model::history::{
    APP, CAP, Jump, Label, Line, MERGE_WINDOW, MergeMark, Refusal, RefusalKind, Request, SETTINGS,
    ScopeId, ScopeInfo, StepInfo, merges, newest, plan_jump,
};

use crate::app::{AppHandle, STEP_REDO, STEP_UNDO};
use crate::host::{Pending, source_of};
use crate::steps::Outcome;

/// A jump to a step of a scope (`{ "scope", "to", "origin" }`, `to` 0:
/// the start): vetoable like an undo; its result names the steps walked.
pub const STEP_JUMP: StepId = StepId::from_static("edit.jump");

/// What the owner answers a request.
#[derive(Debug)]
pub enum Answer {
    /// Done (or, for a dry run, would be), or refused.
    Done(Result<Walked, Refusal>),
    /// A remote owner answers later; it publishes the result on its bus.
    Later(Pending),
}

/// An undo, a redo or a jump, as made.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Walked {
    pub scope: ScopeId,
    /// The steps undone or redone, in the order walked; empty: there was
    /// nothing (or the jump was already there).
    pub steps: Vec<StepInfo>,
    /// The scope's revision after it.
    pub rev: u64,
    /// The scope's head after it (0: nothing done).
    pub head: u64,
    /// A dry run: nothing changed.
    pub dry_run: bool,
}

/// The owner of the application's histories, as the interface reaches it.
pub trait History {
    /// The scopes there are.
    fn scopes(&self) -> Vec<ScopeInfo>;

    /// A scope's steps; `None`: no such scope.
    fn line(&self, scope: &ScopeId) -> Option<Line>;

    /// A scope's revision, read every pass (cheaper than its line).
    fn rev(&self, scope: &ScopeId) -> u64 {
        self.line(scope).map_or(0, |l| l.rev)
    }

    /// A scope's line cut to its head and its next step: what the routing
    /// and the menus read, several times a pass.
    fn tips(&self, scope: &ScopeId) -> Option<Line> {
        self.line(scope).map(|l| tips_of(&l))
    }

    /// Undoes the scope's head.
    fn undo(&mut self, request: &Request, origin: &Origin) -> Answer;

    /// Redoes the scope's next step.
    fn redo(&mut self, request: &Request, origin: &Origin) -> Answer;

    /// Brings the scope to step `to` (0: the start), all or nothing.
    fn jump(&mut self, request: &Request, to: u64, origin: &Origin) -> Answer;

    /// Opens a group: what `origin` does until [`History::close`] is one
    /// step labelled `label`. `None`: the owner groups nothing.
    fn group(&mut self, _origin: &Origin, _label: &Label) -> Option<u64> {
        None
    }

    /// Closes a group: kept as one step, or (`keep` false) put back.
    fn close(&mut self, _group: u64, _keep: bool) {}

    /// A gesture still going in the scope: undo, redo and jump wait for
    /// it.
    fn in_progress(&self, _scope: &ScopeId) -> Option<Label> {
        None
    }
}

/// egui-pop's default owner: the value store's histories, the
/// application's ([`APP`]) by default.
#[derive(Clone)]
pub struct StoreHistory {
    store: SharedStore,
    scopes: Vec<ScopeInfo>,
}

impl fmt::Debug for StoreHistory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StoreHistory")
            .field("scopes", &self.scopes)
            .finish_non_exhaustive()
    }
}

impl StoreHistory {
    /// The store's application history, as one scope.
    pub fn new(store: SharedStore) -> Self {
        Self::with_scopes(store, vec![ScopeInfo::new(APP, "Application")])
    }

    /// The store's histories named by `scopes`.
    pub fn with_scopes(store: SharedStore, scopes: Vec<ScopeInfo>) -> Self {
        Self { store, scopes }
    }

    fn known(&self, scope: &ScopeId) -> Result<(), Refusal> {
        if self.scopes.iter().any(|s| s.id == *scope) {
            Ok(())
        } else {
            Err(unknown(scope))
        }
    }

    fn answer(
        &self,
        request: &Request,
        f: impl FnOnce(&mut Store) -> Result<Vec<StepInfo>, Refusal>,
    ) -> Answer {
        Answer::Done(self.known(&request.scope).and_then(|()| {
            let mut s = lock(&self.store);
            request.check(s.history_rev(&request.scope))?;
            if let Some(label) = s.gesture_open(&request.scope) {
                return Err(in_progress(label));
            }
            let steps = f(&mut s)?;
            let line = store_line(&s, &request.scope);
            Ok(Walked {
                scope: request.scope.clone(),
                steps,
                rev: line.rev,
                head: line.head_id(),
                dry_run: request.dry_run,
            })
        }))
    }
}

impl History for StoreHistory {
    fn scopes(&self) -> Vec<ScopeInfo> {
        let cap = lock(&self.store).history_cap();
        self.scopes
            .iter()
            .cloned()
            .map(|mut s| {
                s.cap = cap;
                s
            })
            .collect()
    }

    fn line(&self, scope: &ScopeId) -> Option<Line> {
        self.known(scope).ok()?;
        Some(store_line(&lock(&self.store), scope))
    }

    fn rev(&self, scope: &ScopeId) -> u64 {
        lock(&self.store).history_rev(scope)
    }

    fn tips(&self, scope: &ScopeId) -> Option<Line> {
        self.known(scope).ok()?;
        let s = lock(&self.store);
        Some(Line {
            undo: s
                .undo_steps_in(scope)
                .last()
                .map(|st| step_info(scope, st))
                .into_iter()
                .collect(),
            redo: s
                .redo_steps_in(scope)
                .last()
                .map(|st| step_info(scope, st))
                .into_iter()
                .collect(),
            rev: s.history_rev(scope),
        })
    }

    fn undo(&mut self, request: &Request, origin: &Origin) -> Answer {
        self.answer(request, |s| store_walk(s, request, origin, false))
    }

    fn redo(&mut self, request: &Request, origin: &Origin) -> Answer {
        self.answer(request, |s| store_walk(s, request, origin, true))
    }

    fn jump(&mut self, request: &Request, to: u64, origin: &Origin) -> Answer {
        self.answer(request, |s| store_jump(s, request, to, origin))
    }

    fn group(&mut self, origin: &Origin, label: &Label) -> Option<u64> {
        Some(lock(&self.store).begin_group(origin.clone(), &label.text).0)
    }

    fn close(&mut self, group: u64, keep: bool) {
        let mut s = lock(&self.store);
        let g = values::GestureId(group);
        if keep {
            s.end(g);
        } else {
            s.cancel(g);
        }
    }

    fn in_progress(&self, scope: &ScopeId) -> Option<Label> {
        lock(&self.store).gesture_open(scope).map(Label::text)
    }
}

/// A line cut to its head and its next step.
pub fn tips_of(line: &Line) -> Line {
    Line {
        undo: line.head().cloned().into_iter().collect(),
        redo: line.next().cloned().into_iter().collect(),
        rev: line.rev,
    }
}

fn unknown(scope: &ScopeId) -> Refusal {
    Refusal::new(RefusalKind::Unknown, format!("no history `{scope}`"))
}

fn in_progress(label: &str) -> Refusal {
    Refusal::new(RefusalKind::Held, format!("“{label}” is in progress"))
}

fn refused(r: values::Refused) -> Refusal {
    let kind = match r {
        values::Refused::Held { .. } => RefusalKind::Held,
        _ => RefusalKind::Owner,
    };
    Refusal::new(kind, r.to_string())
}

/// A store's step as the interface sees it.
fn step_info(scope: &ScopeId, step: &values::Step) -> StepInfo {
    StepInfo {
        id: step.id,
        scope: scope.clone(),
        label: Label::text(&step.label),
        origin: step.origin.clone(),
        group: step.group,
        place: step.place.clone(),
        at: step.at,
        in_progress: false,
    }
}

/// One of the store's histories, as a [`Line`].
pub fn store_line(store: &Store, scope: &ScopeId) -> Line {
    Line {
        undo: store
            .undo_steps_in(scope)
            .iter()
            .map(|s| step_info(scope, s))
            .collect(),
        redo: store
            .redo_steps_in(scope)
            .iter()
            .map(|s| step_info(scope, s))
            .collect(),
        rev: store.history_rev(scope),
    }
}

fn store_walk(
    s: &mut Store,
    request: &Request,
    origin: &Origin,
    redo: bool,
) -> Result<Vec<StepInfo>, Refusal> {
    let scope = &request.scope;
    if request.dry_run {
        let step = s.check_walk(scope, redo).map_err(refused)?;
        return Ok(step.map(|st| step_info(scope, st)).into_iter().collect());
    }
    let step = if redo {
        s.redo_in(scope, origin)
    } else {
        s.undo_in(scope, origin)
    }
    .map_err(refused)?;
    Ok(step.map(|st| step_info(scope, &st)).into_iter().collect())
}

fn store_jump(
    s: &mut Store,
    request: &Request,
    to: u64,
    origin: &Origin,
) -> Result<Vec<StepInfo>, Refusal> {
    let scope = &request.scope;
    let line = store_line(s, scope);
    let jump = plan_jump(&line, to)?;
    let walked = walked_by(&line, jump);
    if request.dry_run {
        let list = if jump.redo {
            s.redo_steps_in(scope)
        } else {
            s.undo_steps_in(scope)
        };
        // Every step on the way must be free: try the first held.
        for st in list.iter().rev().take(jump.count) {
            if let Some((p, _, _)) = st.changes.iter().find(|(p, _, _)| s.held_by(p).is_some()) {
                return Err(Refusal::new(
                    RefusalKind::Held,
                    format!("`{p}` is held by a gesture in progress"),
                ));
            }
        }
        return Ok(walked);
    }
    s.jump_in(scope, to, origin).map_err(refused)?;
    Ok(walked)
}

/// The steps a jump walks over, in the order walked.
fn walked_by(line: &Line, jump: Jump) -> Vec<StepInfo> {
    let list = if jump.redo { &line.redo } else { &line.undo };
    list.iter().rev().take(jump.count).cloned().collect()
}

/// A component's own history, kept in the interface (ADR 0014): the
/// theme editor's draft, the settings window's values. Registered for a
/// focus surface with [`Ctx::local_scope`](crate::Ctx::local_scope):
/// while the focus is in that surface, Ctrl+Z undoes here.
pub trait LocalScope {
    /// The scope (its `local` set).
    fn info(&self) -> ScopeInfo;

    /// Its steps.
    fn line(&self, app: &AppHandle) -> Line;

    /// Undoes its head: the step undone, `None` when there was none.
    fn undo(&self, app: &AppHandle, origin: &Origin) -> Result<Option<StepInfo>, Refusal>;

    /// Redoes its next step.
    fn redo(&self, app: &AppHandle, origin: &Origin) -> Result<Option<StepInfo>, Refusal>;

    /// A gesture still going: undo and redo wait for it.
    fn in_progress(&self, _app: &AppHandle) -> Option<Label> {
        None
    }
}

/// A local scope over one of the value store's histories: what is
/// written while [`values::enter_history`] names it goes there (the
/// settings window's form).
#[derive(Clone, Debug)]
pub struct StoreScope {
    info: ScopeInfo,
}

impl StoreScope {
    pub fn new(scope: ScopeId, title: &str) -> Self {
        let mut info = ScopeInfo::new(scope, title);
        info.local = true;
        Self { info }
    }

    fn walk(
        &self,
        app: &AppHandle,
        origin: &Origin,
        redo: bool,
    ) -> Result<Option<StepInfo>, Refusal> {
        let store = app.store();
        let mut s = lock(&store);
        if let Some(label) = s.gesture_open(&self.info.id) {
            return Err(in_progress(label));
        }
        let step = if redo {
            s.redo_in(&self.info.id, origin)
        } else {
            s.undo_in(&self.info.id, origin)
        }
        .map_err(refused)?;
        Ok(step.map(|st| step_info(&self.info.id, &st)))
    }
}

impl LocalScope for StoreScope {
    fn info(&self) -> ScopeInfo {
        self.info.clone()
    }

    fn line(&self, app: &AppHandle) -> Line {
        store_line(&lock(&app.store()), &self.info.id)
    }

    fn undo(&self, app: &AppHandle, origin: &Origin) -> Result<Option<StepInfo>, Refusal> {
        self.walk(app, origin, false)
    }

    fn redo(&self, app: &AppHandle, origin: &Origin) -> Result<Option<StepInfo>, Refusal> {
        self.walk(app, origin, true)
    }

    fn in_progress(&self, app: &AppHandle) -> Option<Label> {
        lock(&app.store())
            .gesture_open(&self.info.id)
            .map(Label::text)
    }
}

/// A local scope registered: by whom, for which surface.
type Registered = (OwnerId, String, Rc<dyn LocalScope>);

/// What the interface keeps: the local scopes, the scopes undone last
/// (for redo), the revisions published, the clock it gives the store.
#[derive(Default)]
pub(crate) struct Kept {
    locals: RefCell<Vec<Registered>>,
    undone: RefCell<Vec<ScopeId>>,
    revs: RefCell<BTreeMap<ScopeId, u64>>,
    focused: Cell<Option<egui::Id>>,
    clock: RefCell<Option<ManualClock>>,
}

/// Registers a local scope for a focus surface (`Ctx::local_scope`).
pub(crate) fn register(app: &AppHandle, owner: &OwnerId, surface: &str, scope: Rc<dyn LocalScope>) {
    let kept = app.local::<Kept>();
    let mut locals = kept.locals.borrow_mut();
    locals.retain(|(_, s, _)| s != surface);
    locals.push((owner.clone(), surface.into(), scope));
}

/// A plugin is gone: its local scopes go.
pub(crate) fn remove_owner(app: &AppHandle, owner: &OwnerId) {
    app.local::<Kept>()
        .locals
        .borrow_mut()
        .retain(|(o, _, _)| o != owner);
}

/// Runs `f` on the owner: the host's, else a [`StoreHistory`] over the
/// host's store. `None` while the host dispatches a command.
pub fn with_owner<R>(app: &AppHandle, f: impl FnOnce(&mut dyn History) -> R) -> Option<R> {
    app.with_host_mut(|h| {
        let store = h.store();
        match h.history() {
            Some(owner) => f(owner),
            None => f(&mut StoreHistory::new(store)),
        }
    })
}

/// Where an undo or a redo goes.
#[derive(Clone)]
pub enum Target {
    /// A component's own history.
    Local(Rc<dyn LocalScope>),
    /// One of the owner's scopes.
    Owner(ScopeId),
}

impl fmt::Debug for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Target::Local(l) => write!(f, "Local({})", l.info().id),
            Target::Owner(s) => write!(f, "Owner({s})"),
        }
    }
}

impl Target {
    /// The scope it names.
    pub fn scope(&self) -> ScopeId {
        match self {
            Target::Local(l) => l.info().id,
            Target::Owner(s) => s.clone(),
        }
    }

    /// Its steps now.
    pub fn line(&self, app: &AppHandle) -> Option<Line> {
        match self {
            Target::Local(l) => Some(l.line(app)),
            Target::Owner(s) => with_owner(app, |h| h.line(s)).flatten(),
        }
    }
}

/// The local scope of the innermost focused surface that has one.
fn focused_local(app: &AppHandle) -> Option<Rc<dyn LocalScope>> {
    let chain = focus::chain_names(&app.egui());
    let kept = app.local::<Kept>();
    let locals = kept.locals.borrow();
    chain.iter().rev().find_map(|name| {
        locals
            .iter()
            .find(|(_, s, _)| s == name)
            .map(|(_, _, l)| l.clone())
    })
}

/// The names of the surfaces drawn in the last pass.
fn shown(app: &AppHandle) -> Vec<String> {
    focus::scopes(&app.egui())
        .into_iter()
        .map(|r| r.name)
        .collect()
}

/// Whether a scope's focus targets are drawn (a panel's id stands for
/// its surface).
fn is_active(info: &ScopeInfo, shown: &[String]) -> bool {
    info.focus.is_empty()
        || info.focus.iter().any(|f| {
            let surface = crate::app::scope_name(f);
            shown.iter().any(|s| *s == *f || *s == surface)
        })
}

/// The owner's scopes active in the window, with their heads and next
/// steps.
fn active_lines(app: &AppHandle) -> Vec<Line> {
    let shown = shown(app);
    with_owner(app, |h| {
        h.scopes()
            .iter()
            .filter(|s| is_active(s, &shown))
            .filter_map(|s| h.tips(&s.id))
            .collect()
    })
    .unwrap_or_default()
}

/// The scopes the owner has active in the window now.
pub fn active(app: &AppHandle) -> Vec<ScopeId> {
    let shown = shown(app);
    with_owner(app, |h| {
        h.scopes()
            .into_iter()
            .filter(|s| is_active(s, &shown))
            .map(|s| s.id)
            .collect()
    })
    .unwrap_or_default()
}

/// Where Ctrl+Z (`redo` false) or Ctrl+Shift+Z goes now: the focused
/// local scope, else the owner's active scope with the newest head (for
/// a redo, the scope last undone that can still redo, else the first
/// active one that can). `None`: no history at all.
pub fn target(app: &AppHandle, redo: bool) -> Option<Target> {
    if let Some(l) = focused_local(app) {
        return Some(Target::Local(l));
    }
    let lines = active_lines(app);
    if !redo {
        return newest(&lines)
            .cloned()
            .or_else(|| lines.first().and_then(scope_of))
            .map(Target::Owner);
    }
    let undone = app.local::<Kept>().undone.borrow().clone();
    let can = |s: &ScopeId| {
        lines
            .iter()
            .any(|l| l.next().is_some_and(|n| n.scope == *s))
    };
    undone
        .iter()
        .rev()
        .find(|s| can(s))
        .cloned()
        .or_else(|| lines.iter().find_map(|l| l.next().map(|n| n.scope.clone())))
        .or_else(|| lines.first().and_then(scope_of))
        .map(Target::Owner)
}

fn scope_of(line: &Line) -> Option<ScopeId> {
    line.undo
        .first()
        .or(line.redo.first())
        .map(|s| s.scope.clone())
}

/// The scope named `scope`: a local scope's, else the owner's.
pub fn scope_target(app: &AppHandle, scope: &ScopeId) -> Option<Target> {
    let kept = app.local::<Kept>();
    if let Some(l) = kept
        .locals
        .borrow()
        .iter()
        .find(|(_, _, l)| l.info().id == *scope)
    {
        return Some(Target::Local(l.2.clone()));
    }
    with_owner(app, |h| h.scopes().iter().any(|s| s.id == *scope))
        .unwrap_or(false)
        .then(|| Target::Owner(scope.clone()))
}

/// Every scope: the local ones, then the owner's.
pub fn scopes(app: &AppHandle) -> Vec<ScopeInfo> {
    let mut out: Vec<ScopeInfo> = app
        .local::<Kept>()
        .locals
        .borrow()
        .iter()
        .map(|(_, _, l)| l.info())
        .collect();
    out.extend(with_owner(app, |h| h.scopes()).unwrap_or_default());
    out
}

/// A scope's steps, local or the owner's.
pub fn line(app: &AppHandle, scope: &ScopeId) -> Option<Line> {
    scope_target(app, scope)?.line(app)
}

/// Whether Ctrl+Z would undo something now (`canUndo`).
pub fn can_undo(app: &AppHandle) -> bool {
    head_label(app, false).is_some()
}

/// Whether Ctrl+Shift+Z would redo something now (`canRedo`).
pub fn can_redo(app: &AppHandle) -> bool {
    head_label(app, true).is_some()
}

/// What Ctrl+Z would undo, as the Edit menu names it.
pub fn undo_label(app: &AppHandle) -> Option<String> {
    head_label(app, false)
}

/// What Ctrl+Shift+Z would redo.
pub fn redo_label(app: &AppHandle) -> Option<String> {
    head_label(app, true)
}

fn head_label(app: &AppHandle, redo: bool) -> Option<String> {
    let line = match target(app, redo)? {
        Target::Owner(s) => with_owner(app, |h| h.tips(&s)).flatten()?,
        t @ Target::Local(_) => t.line(app)?,
    };
    let step = if redo { line.next() } else { line.head() }?;
    Some(label_text(&app.egui(), &step.label))
}

/// A label in the interface's language: its message when it has one,
/// else its text.
pub fn label_text(ctx: &egui::Context, label: &Label) -> String {
    match &label.key {
        Some(key) => {
            let args: Vec<(&str, &str)> = label
                .args
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();
            strings::lookup_args(ctx, key, &label.text, &args)
        }
        None => label.text.clone(),
    }
}

/// What a walk asks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Walk {
    Undo,
    Redo,
    Jump(u64),
}

/// Undoes or redoes (or jumps) in a target, now or as a dry run.
fn walk(
    app: &AppHandle,
    target: &Target,
    what: Walk,
    request: &Request,
    origin: &Origin,
) -> Result<Option<Walked>, Refusal> {
    match target {
        Target::Owner(scope) => {
            let mut request = request.clone();
            request.scope = scope.clone();
            let answer = with_owner(app, |h| {
                if let Some(label) = h.in_progress(scope) {
                    return Answer::Done(Err(in_progress(&label.text)));
                }
                match what {
                    Walk::Undo => h.undo(&request, origin),
                    Walk::Redo => h.redo(&request, origin),
                    Walk::Jump(to) => h.jump(&request, to, origin),
                }
            })
            .ok_or_else(|| Refusal::new(RefusalKind::Owner, "the host is dispatching a command"))?;
            match answer {
                Answer::Done(r) => r.map(Some),
                Answer::Later(_) => Ok(None),
            }
        }
        Target::Local(l) => {
            let info = l.info();
            let line = l.line(app);
            request.check(line.rev)?;
            if let Some(label) = l.in_progress(app) {
                return Err(in_progress(&label.text));
            }
            let (redo, count) = match what {
                Walk::Undo => (false, usize::from(line.head().is_some())),
                Walk::Redo => (true, usize::from(line.next().is_some())),
                Walk::Jump(to) => {
                    let j = plan_jump(&line, to)?;
                    (j.redo, j.count)
                }
            };
            let mut steps = Vec::new();
            if request.dry_run {
                steps = walked_by(&line, Jump { redo, count });
            } else {
                for _ in 0..count {
                    let step = if redo {
                        l.redo(app, origin)?
                    } else {
                        l.undo(app, origin)?
                    };
                    match step {
                        Some(s) => steps.push(s),
                        None => break,
                    }
                }
            }
            let after = l.line(app);
            Ok(Some(Walked {
                scope: info.id,
                steps,
                rev: after.rev,
                head: after.head_id(),
                dry_run: request.dry_run,
            }))
        }
    }
}

/// Undoes where Ctrl+Z goes ([`target`]), through the `edit.undo` step.
pub fn undo(app: &AppHandle, origin: &Origin) -> Outcome {
    let t = target(app, false);
    run(app, t, Walk::Undo, origin)
}

/// Redoes where Ctrl+Shift+Z goes, through the `edit.redo` step.
pub fn redo(app: &AppHandle, origin: &Origin) -> Outcome {
    let t = target(app, true);
    run(app, t, Walk::Redo, origin)
}

/// Undoes in a named scope, through the `edit.undo` step.
pub fn undo_in(app: &AppHandle, scope: &ScopeId, origin: &Origin) -> Outcome {
    let t = scope_target(app, scope).unwrap_or_else(|| Target::Owner(scope.clone()));
    run(app, Some(t), Walk::Undo, origin)
}

/// Redoes in a named scope, through the `edit.redo` step.
pub fn redo_in(app: &AppHandle, scope: &ScopeId, origin: &Origin) -> Outcome {
    let t = scope_target(app, scope).unwrap_or_else(|| Target::Owner(scope.clone()));
    run(app, Some(t), Walk::Redo, origin)
}

/// Brings a scope to step `to` (0: the start), through the `edit.jump`
/// step.
pub fn jump(app: &AppHandle, scope: &ScopeId, to: u64, origin: &Origin) -> Outcome {
    let t = scope_target(app, scope).unwrap_or_else(|| Target::Owner(scope.clone()));
    run(app, Some(t), Walk::Jump(to), origin)
}

/// Runs a walk through its step: its hooks may veto it, or redirect it
/// by naming another `scope` in its arguments.
fn run(app: &AppHandle, target: Option<Target>, what: Walk, origin: &Origin) -> Outcome {
    let (step, key, done, failed) = match what {
        Walk::Undo => (&STEP_UNDO, "undone", "edit.undone", "edit.undo_failed"),
        Walk::Redo => (&STEP_REDO, "redone", "edit.redone", "edit.redo_failed"),
        Walk::Jump(_) => (&STEP_JUMP, "jumped", "edit.jumped", "edit.jump_failed"),
    };
    let ctx = app.egui();
    let line = target.as_ref().and_then(|t| t.line(app));
    let label = line
        .as_ref()
        .and_then(|l| match what {
            Walk::Undo => l.head(),
            Walk::Redo => l.next(),
            Walk::Jump(_) => None,
        })
        .map(|s| label_text(&ctx, &s.label));
    let scope = target.as_ref().map(Target::scope);
    let mut args = json!({
        "label": label,
        "origin": source_of(origin),
        "scope": scope,
    });
    if let Walk::Jump(to) = what {
        args["to"] = json!(to);
    }
    let refusal: Rc<RefCell<Option<Refusal>>> = Rc::default();
    let walked: Rc<RefCell<Option<Walked>>> = Rc::default();
    let (r2, w2) = (refusal.clone(), walked.clone());
    let who = origin.clone();
    let outcome = app.step(step, args, move |app, args| {
        // A before-hook may have named another scope.
        let asked = args
            .get("scope")
            .and_then(Json::as_str)
            .and_then(|s| ScopeId::new(s).ok());
        let target = match (&asked, target) {
            (Some(a), Some(t)) if t.scope() == *a => Some(t),
            (Some(a), _) => scope_target(app, a),
            (None, t) => t,
        };
        let Some(target) = target else {
            return Ok(json!({ key: Json::Null }));
        };
        let request = Request::new(target.scope());
        match walk(app, &target, what, &request, &who) {
            Ok(Some(w)) => {
                let name = w.steps.last().map(|s| label_text(&app.egui(), &s.label));
                let out = json!({
                    key: name,
                    "scope": w.scope,
                    "steps": w.steps,
                    "rev": w.rev,
                    "head": w.head,
                });
                *w2.borrow_mut() = Some(w);
                Ok(out)
            }
            Ok(None) => Ok(json!({ key: Json::Null, "scope": target.scope(), "pending": true })),
            Err(r) => {
                let reason = r.reason.clone();
                *r2.borrow_mut() = Some(r);
                Err(reason)
            }
        }
    });
    let source = source_of(origin);
    match &outcome {
        Outcome::Done(Ok(v)) => {
            if !v[key].is_null() {
                app.publish(
                    &source,
                    None,
                    done,
                    json!({ "label": v[key], "scope": v["scope"] }),
                );
            }
        }
        Outcome::Done(Err(e)) => {
            app.publish(&source, None, failed, json!({ "label": label, "error": e }));
        }
        Outcome::Vetoed { by, reason } => {
            app.publish(
                &source,
                None,
                failed,
                json!({ "label": label, "vetoed_by": by, "reason": reason }),
            );
            *refusal.borrow_mut() = Some(Refusal::new(RefusalKind::Vetoed, reason.clone()));
        }
    }
    if let Some(r) = refusal.borrow_mut().take() {
        app.publish(
            &source,
            None,
            "history.refused",
            json!({ "scope": scope, "kind": r.kind, "reason": r.reason }),
        );
    }
    if let Some(w) = walked.borrow_mut().take()
        && !w.dry_run
        && !w.steps.is_empty()
    {
        let kept = app.local::<Kept>();
        {
            let mut undone = kept.undone.borrow_mut();
            undone.retain(|s| *s != w.scope);
            if what == Walk::Undo || matches!(what, Walk::Jump(_)) {
                undone.push(w.scope.clone());
            }
        }
        kept.revs.borrow_mut().insert(w.scope.clone(), w.rev);
        changed(app, &w.scope, w.rev, w.head);
    }
    outcome
}

fn changed(app: &AppHandle, scope: &ScopeId, rev: u64, head: u64) {
    app.publish(
        "egui_pop",
        None,
        "history.changed",
        json!({ "scope": scope, "rev": rev, "head": head }),
    );
}

/// Each pass: the store's clock moved to the interface's time, merging
/// sealed when the focus moved, and `history.changed` for the owner's
/// scopes whose revision moved since last published.
pub(crate) fn pass(app: &AppHandle) {
    let ctx = app.egui();
    let kept = app.local::<Kept>();
    let store = app.store();
    {
        let mut clock = kept.clock.borrow_mut();
        let mut s = lock(&store);
        if clock.is_none() && !s.has_clock() {
            let c = ManualClock::default();
            s.set_clock(Some(c.shared()));
            *clock = Some(c);
        }
        if let Some(c) = clock.as_ref() {
            c.set(ctx.input(|i| i.time));
        }
        let focused = focus::focused(&ctx);
        if kept.focused.replace(focused) != focused {
            s.seal();
        }
    }
    let moved: Vec<(ScopeId, u64, u64)> = {
        let revs = kept.revs.borrow();
        with_owner(app, |h| {
            h.scopes()
                .into_iter()
                .filter_map(|s| {
                    let rev = h.rev(&s.id);
                    (revs.get(&s.id).copied().unwrap_or(0) != rev)
                        .then(|| (s.id.clone(), rev, h.line(&s.id).map_or(0, |l| l.head_id())))
                })
                .collect()
        })
        .unwrap_or_default()
    };
    for (id, rev, head) in moved {
        kept.revs.borrow_mut().insert(id.clone(), rev);
        changed(app, &id, rev, head);
    }
}

/// The `history` collector's snapshot: the scopes, which is active,
/// where Ctrl+Z and Ctrl+Shift+Z go and what they would do.
pub(crate) fn snapshot(app: &AppHandle) -> Json {
    let active = active(app);
    let scopes: Vec<Json> = scopes(app)
        .into_iter()
        .map(|s| {
            let l = line(app, &s.id).unwrap_or_default();
            json!({
                "scope": s,
                "active": s.local || active.contains(&s.id),
                "rev": l.rev,
                "head": l.head_id(),
                "undo": l.undo.len(),
                "redo": l.redo.len(),
            })
        })
        .collect();
    json!({
        "can_undo": can_undo(app),
        "undo": undo_label(app),
        "undo_scope": target(app, false).map(|t| t.scope()),
        "can_redo": can_redo(app),
        "redo": redo_label(app),
        "redo_scope": target(app, true).map(|t| t.scope()),
        "scopes": scopes,
    })
}
