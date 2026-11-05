//! The application-wide selection: what the user selected and what the
//! pointer is over, shared by every panel.
//!
//! A tree, a list and a canvas showing the same things select and hover
//! them together: a click in one highlights the item in the others, and
//! hovering a row lights up the same item everywhere. The selection is
//! one [`SelectionService`] (the [`SELECTION`] token), provided by
//! [`SelectionPlugin`]; widgets opt in (`TreeView::shared`,
//! `ListView::shared` in `egui_pop_views`, or a canvas of the
//! application's calling [`SelectionService::click`] and
//! [`SelectionService::hover`]).
//!
//! - **Items** are [`SelectionItem`]s: a string id each, a path such as
//!   `photos/2024/0012.raw` or `entity:/world/points`. Widgets with
//!   different keys meet on it.
//! - **Gestures** ([`Gesture`]): a click replaces, Ctrl (Cmd on macOS)
//!   toggles, Shift selects a range in the order the widget clicked gives,
//!   Ctrl+Shift adds that range.
//! - **Hover**: one item at most, said by the widget under the pointer
//!   each pass; it goes when that widget says it hovers nothing, or stops
//!   saying anything.
//! - **Changes**: each bumps a revision; subscribers are called
//!   ([`SelectionService::subscribe`]), the journal says
//!   `selection.changed`, and the context keys follow: `hasSelection`,
//!   `selectionCount`, `selectionCanBack` and `selectionCanForward`.
//! - **History**: the past selections, walked by `selection.back` and
//!   `selection.forward` (Alt+Left and Alt+Right while the focus is in a
//!   widget bound to the selection, its [`SURFACE`]). A selection is not
//!   an edit: undo does not see it.
//! - **Agents**: the `selection` collector, and the `selection.set`,
//!   `selection.add`, `selection.clear` and `selection.hover` commands.
//!
//! ```
//! use egui_pop_api::selection::{Gesture, SelectionItem, SelectionService};
//!
//! let s = SelectionService::new();
//! let order: Vec<SelectionItem> = ["a", "b", "c", "d"].map(SelectionItem::from).to_vec();
//! s.click(&order[1], Gesture::Replace, &order, "list");
//! s.click(&order[3], Gesture::Range, &order, "list");
//! assert_eq!(s.items(), order[1..].to_vec());
//! s.click(&order[2], Gesture::Toggle, &order, "list");
//! assert_eq!(s.items(), vec![order[1].clone(), order[3].clone()]);
//! assert!(s.back("test"));
//! assert_eq!(s.len(), 3);
//! ```

use std::collections::HashSet;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};

use egui::Modifiers;
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::ids::{CommandId, PluginId};
use egui_pop_model::keymap::BindOptions;
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};

use crate::hooks::On;
use crate::services::ServiceToken;
use crate::{AppHandle, Ctx, Plugin, PluginError};

/// The plugin's id.
pub const SELECTION_PLUGIN: PluginId = PluginId::from_static("egui_pop.selection");

/// The selection service.
pub const SELECTION: ServiceToken<SelectionService> = ServiceToken::new("egui_pop.selection");

/// The focus surface of the widgets bound to the selection: Alt+Left and
/// Alt+Right go back and forward through the selections there, and the
/// context stack's `nav.back` keeps them elsewhere.
pub const SURFACE: &str = "selection";

/// The context key: something is selected (`true` or `false`).
pub const CONTEXT_HAS: &str = "hasSelection";
/// The context key: how many items are selected.
pub const CONTEXT_COUNT: &str = "selectionCount";
/// The context key: `selection.back` has a selection to go back to.
pub const CONTEXT_CAN_BACK: &str = "selectionCanBack";
/// The context key: `selection.forward` has a selection to go to.
pub const CONTEXT_CAN_FORWARD: &str = "selectionCanForward";

/// Back to the selection before (Alt+Left in a bound widget).
pub const CMD_BACK: CommandId = CommandId::from_static("selection.back");
/// Forward to the selection gone back from (Alt+Right in a bound widget).
pub const CMD_FORWARD: CommandId = CommandId::from_static("selection.forward");
/// Selects these items, and only them.
pub const CMD_SET: CommandId = CommandId::from_static("selection.set");
/// Adds items to the selection.
pub const CMD_ADD: CommandId = CommandId::from_static("selection.add");
/// Selects nothing.
pub const CMD_CLEAR: CommandId = CommandId::from_static("selection.clear");
/// Hovers an item, or none.
pub const CMD_HOVER: CommandId = CommandId::from_static("selection.hover");

/// The journal's event after a change of the selection or of the hovered
/// item.
pub const EVENT_CHANGED: &str = "selection.changed";

/// How many past selections the history keeps.
pub const HISTORY_CAP: usize = 100;

/// The source a change made by an agent's command names.
pub const SOURCE_COMMAND: &str = "command";

/// An item of the selection: a string id the application chooses, the
/// same in every widget showing the item (a path, `kind:id`, a number).
///
/// A string rather than a typed key: widgets with different key types (a
/// tree by path, a table by row id, a canvas by entity) meet on it, it
/// goes through the journal, the history and the agents' commands as it
/// is, and it clones for the cost of a reference count.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SelectionItem(Arc<str>);

impl SelectionItem {
    pub fn new(id: impl Into<Arc<str>>) -> Self {
        Self(id.into())
    }

    /// The item of a widget's key: a string key as it is, any other its
    /// JSON text (`12`, `[1,2]`).
    pub fn from_key<K: Serialize + ?Sized>(key: &K) -> Self {
        match serde_json::to_value(key) {
            Ok(Json::String(s)) => Self::new(s),
            Ok(v) => Self::new(v.to_string()),
            Err(_) => Self::new(""),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SelectionItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&*self.0, f)
    }
}

impl fmt::Display for SelectionItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for SelectionItem {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for SelectionItem {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

/// What a click does to the selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gesture {
    /// Selects the item alone (a plain click).
    Replace,
    /// Adds the item, or removes it when selected (Ctrl, Cmd on macOS).
    Toggle,
    /// Selects from the anchor to the item, in the order the widget
    /// gives, and only that (Shift).
    Range,
    /// Adds the range from the anchor to the item (Ctrl+Shift).
    AddRange,
}

impl Gesture {
    /// The gesture of a click with these modifiers.
    pub fn of(modifiers: Modifiers) -> Self {
        match (modifiers.command, modifiers.shift) {
            (true, true) => Gesture::AddRange,
            (false, true) => Gesture::Range,
            (true, false) => Gesture::Toggle,
            (false, false) => Gesture::Replace,
        }
    }
}

/// What changed, as subscribers get it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Change {
    /// The selected items after the change, in the order selected.
    pub items: Vec<SelectionItem>,
    pub hovered: Option<SelectionItem>,
    /// The selection's revision (the hover has its own).
    pub revision: u64,
    /// Whether the selected items changed (else only the hovered one).
    pub selection: bool,
    /// Who changed it: a widget's id, `command`, `history`.
    pub source: String,
}

/// A subscription, to give back to [`SelectionService::unsubscribe`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Subscription(u64);

type Listener = Arc<dyn Fn(&Change) + Send + Sync>;

struct Hover {
    item: SelectionItem,
    source: String,
    /// The egui pass it was last said in (`None`: until said otherwise,
    /// an agent's).
    pass: Option<u64>,
}

#[derive(Default)]
struct Inner {
    items: Vec<SelectionItem>,
    set: HashSet<SelectionItem>,
    anchor: Option<SelectionItem>,
    hovered: Option<Hover>,
    revision: u64,
    hover_revision: u64,
    source: String,
    back: Vec<Vec<SelectionItem>>,
    forward: Vec<Vec<SelectionItem>>,
    listeners: Vec<(u64, Listener)>,
    next_listener: u64,
    /// The revisions the plugin last published (selection, hover).
    published: (u64, u64),
}

impl Inner {
    fn assign(&mut self, items: Vec<SelectionItem>) {
        self.set.clear();
        self.items.clear();
        for i in items {
            if self.set.insert(i.clone()) {
                self.items.push(i);
            }
        }
    }
}

/// The application-wide selection; see the [module](self).
///
/// Thread-safe: an application's worker may select too. Every change
/// calls the subscribers once the lock is released and asks egui for a
/// repaint.
pub struct SelectionService {
    inner: Mutex<Inner>,
    egui: Mutex<Option<egui::Context>>,
}

impl fmt::Debug for SelectionService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = self.lock();
        f.debug_struct("SelectionService")
            .field("items", &s.items)
            .field("hovered", &s.hovered.as_ref().map(|h| &h.item))
            .field("revision", &s.revision)
            .finish_non_exhaustive()
    }
}

impl Default for SelectionService {
    fn default() -> Self {
        Self::new()
    }
}

impl SelectionService {
    /// An empty selection, on its own (the plugin provides the
    /// application's).
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner::default()),
            egui: Mutex::new(None),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The context asked to repaint after a change.
    pub fn set_egui(&self, ctx: &egui::Context) {
        *self.egui.lock().unwrap_or_else(|e| e.into_inner()) = Some(ctx.clone());
    }

    /// The selected items, in the order they were selected.
    pub fn items(&self) -> Vec<SelectionItem> {
        self.lock().items.clone()
    }

    pub fn contains(&self, item: &SelectionItem) -> bool {
        self.lock().set.contains(item)
    }

    pub fn len(&self) -> usize {
        self.lock().items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lock().items.is_empty()
    }

    /// Where a range (Shift) starts: the item last clicked.
    pub fn anchor(&self) -> Option<SelectionItem> {
        self.lock().anchor.clone()
    }

    /// The item under the pointer, in whichever widget.
    pub fn hovered(&self) -> Option<SelectionItem> {
        self.lock().hovered.as_ref().map(|h| h.item.clone())
    }

    pub fn is_hovered(&self, item: &SelectionItem) -> bool {
        self.lock()
            .hovered
            .as_ref()
            .is_some_and(|h| h.item == *item)
    }

    /// Bumped by each change of the selected items: a widget compares it
    /// with the one it last saw to know whether to look again.
    pub fn revision(&self) -> u64 {
        self.lock().revision
    }

    /// Bumped by each change of the hovered item.
    pub fn hover_revision(&self) -> u64 {
        self.lock().hover_revision
    }

    /// Who made the last change of the selected items.
    pub fn source(&self) -> String {
        self.lock().source.clone()
    }

    /// Calls `f` after each change (also of the hovered item), from the
    /// thread making it, the lock released.
    pub fn subscribe(&self, f: impl Fn(&Change) + Send + Sync + 'static) -> Subscription {
        let mut s = self.lock();
        s.next_listener += 1;
        let id = s.next_listener;
        s.listeners.push((id, Arc::new(f)));
        Subscription(id)
    }

    pub fn unsubscribe(&self, subscription: Subscription) {
        self.lock()
            .listeners
            .retain(|(id, _)| *id != subscription.0);
    }

    /// Selects these items, and only them.
    pub fn set(&self, items: impl IntoIterator<Item = SelectionItem>, source: &str) -> bool {
        let items: Vec<SelectionItem> = items.into_iter().collect();
        let anchor = items.last().cloned();
        self.change(source, true, |s| {
            s.assign(items);
            s.anchor = anchor;
        })
    }

    /// Adds these items (those already selected stay where they are).
    pub fn add(&self, items: impl IntoIterator<Item = SelectionItem>, source: &str) -> bool {
        let items: Vec<SelectionItem> = items.into_iter().collect();
        self.change(source, true, |s| {
            let mut all = s.items.clone();
            all.extend(items.iter().cloned());
            s.assign(all);
            if let Some(last) = items.last() {
                s.anchor = Some(last.clone());
            }
        })
    }

    /// Adds `add` and removes `remove`, as one change (one step of the
    /// history): what a widget's Ctrl gesture does to its own items while
    /// keeping the others'.
    pub fn apply(
        &self,
        add: impl IntoIterator<Item = SelectionItem>,
        remove: impl IntoIterator<Item = SelectionItem>,
        source: &str,
    ) -> bool {
        let add: Vec<SelectionItem> = add.into_iter().collect();
        let gone: HashSet<SelectionItem> = remove.into_iter().collect();
        self.change(source, true, |s| {
            let mut all: Vec<SelectionItem> = s
                .items
                .iter()
                .filter(|i| !gone.contains(i))
                .cloned()
                .collect();
            all.extend(add.iter().cloned());
            s.assign(all);
            if let Some(last) = add.last() {
                s.anchor = Some(last.clone());
            }
        })
    }

    /// Removes these items.
    pub fn remove(&self, items: impl IntoIterator<Item = SelectionItem>, source: &str) -> bool {
        let gone: HashSet<SelectionItem> = items.into_iter().collect();
        self.change(source, true, |s| {
            let kept = s
                .items
                .iter()
                .filter(|i| !gone.contains(i))
                .cloned()
                .collect();
            s.assign(kept);
        })
    }

    /// Adds the item, or removes it when selected.
    pub fn toggle(&self, item: &SelectionItem, source: &str) -> bool {
        self.change(source, true, |s| {
            let mut all = s.items.clone();
            if s.set.contains(item) {
                all.retain(|i| i != item);
            } else {
                all.push(item.clone());
            }
            s.assign(all);
            s.anchor = Some(item.clone());
        })
    }

    /// Selects nothing.
    pub fn clear(&self, source: &str) -> bool {
        self.change(source, true, |s| {
            s.assign(Vec::new());
            s.anchor = None;
        })
    }

    /// A click on `item` in a widget showing `order` (its items in the
    /// order shown, for a range; empty when it has no order). A range
    /// whose anchor is not in `order` selects the item alone. Returns
    /// whether the selection changed.
    pub fn click(
        &self,
        item: &SelectionItem,
        gesture: Gesture,
        order: &[SelectionItem],
        source: &str,
    ) -> bool {
        match gesture {
            Gesture::Replace => self.set([item.clone()], source),
            Gesture::Toggle => self.toggle(item, source),
            Gesture::Range | Gesture::AddRange => {
                let anchor = self.anchor();
                let range = anchor
                    .as_ref()
                    .and_then(|a| range(order, a, item))
                    .unwrap_or_else(|| vec![item.clone()]);
                self.change(source, true, |s| {
                    let mut all = if gesture == Gesture::AddRange {
                        s.items.clone()
                    } else {
                        Vec::new()
                    };
                    all.extend(range);
                    s.assign(all);
                    // The anchor stays: the next Shift+click ranges from it.
                    s.anchor = anchor.clone().or_else(|| Some(item.clone()));
                })
            }
        }
    }

    /// What the widget `source` hovers this pass (`None`: nothing of
    /// it). A widget says it every pass it is drawn: its `None` clears a
    /// hover it said, never another widget's; a hover no widget repeats
    /// goes after a pass (the plugin sees to it).
    pub fn hover(&self, source: &str, item: Option<&SelectionItem>) {
        let pass = self
            .egui
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|c| c.cumulative_pass_nr());
        self.set_hover(source, item, pass.or(Some(0)));
    }

    /// Hovers an item until said otherwise (an agent's), or none.
    pub fn hover_until_changed(&self, item: Option<&SelectionItem>, source: &str) {
        let item = item.cloned();
        let owner = source.to_string();
        self.change(source, false, move |s| {
            s.hovered = item.map(|item| Hover {
                item,
                source: owner,
                pass: None,
            });
        });
    }

    fn set_hover(&self, source: &str, item: Option<&SelectionItem>, pass: Option<u64>) {
        {
            let mut s = self.lock();
            match (&mut s.hovered, item) {
                // The same: only its pass moves, no change.
                (Some(h), Some(i)) if h.item == *i => {
                    h.source = source.to_string();
                    h.pass = pass;
                    return;
                }
                (Some(h), None) if h.source != source => return,
                (None, None) => return,
                _ => {}
            }
        }
        let item = item.cloned();
        let owner = source.to_string();
        self.change(source, false, move |s| {
            s.hovered = item.map(|item| Hover {
                item,
                source: owner,
                pass,
            });
        });
    }

    /// Clears a widget's hover not said since pass `before` (the widget
    /// is no longer drawn). The plugin calls it every pass.
    pub fn expire_hover(&self, before: u64) {
        let stale = self
            .lock()
            .hovered
            .as_ref()
            .and_then(|h| h.pass.filter(|p| *p < before).map(|_| h.source.clone()));
        if let Some(source) = stale {
            self.set_hover(&source, None, None);
        }
    }

    /// Whether there is a selection to go back to.
    pub fn can_back(&self) -> bool {
        !self.lock().back.is_empty()
    }

    /// Whether there is a selection to go forward to.
    pub fn can_forward(&self) -> bool {
        !self.lock().forward.is_empty()
    }

    /// Back to the selection before; `false` when there is none.
    pub fn back(&self, source: &str) -> bool {
        self.walk(source, true)
    }

    /// Forward to the selection gone back from; `false` when there is
    /// none.
    pub fn forward(&self, source: &str) -> bool {
        self.walk(source, false)
    }

    fn walk(&self, source: &str, back: bool) -> bool {
        let listeners;
        let change;
        {
            let mut s = self.lock();
            let to = if back { s.back.pop() } else { s.forward.pop() };
            let Some(to) = to else { return false };
            let from = std::mem::take(&mut s.items);
            if back {
                s.forward.push(from);
            } else {
                s.back.push(from);
            }
            s.anchor = to.last().cloned();
            s.assign(to);
            s.revision += 1;
            s.source = source.to_string();
            listeners = s
                .listeners
                .iter()
                .map(|(_, l)| l.clone())
                .collect::<Vec<_>>();
            change = describe_change(&s, true, source);
        }
        self.notify(listeners, &change);
        true
    }

    /// Applies `f`; when the selected items (or, `selection` false, the
    /// hovered one) changed, bumps the revision, keeps the history and
    /// tells the subscribers. Returns whether anything changed.
    fn change(&self, source: &str, selection: bool, f: impl FnOnce(&mut Inner)) -> bool {
        let listeners;
        let change;
        {
            let mut s = self.lock();
            let before = s.items.clone();
            let hovered = s.hovered.as_ref().map(|h| h.item.clone());
            f(&mut s);
            if selection {
                if s.items == before {
                    return false;
                }
                s.back.push(before);
                if s.back.len() > HISTORY_CAP {
                    s.back.remove(0);
                }
                s.forward.clear();
                s.revision += 1;
                s.source = source.to_string();
            } else {
                if s.hovered.as_ref().map(|h| &h.item) == hovered.as_ref() {
                    return false;
                }
                s.hover_revision += 1;
            }
            listeners = s
                .listeners
                .iter()
                .map(|(_, l)| l.clone())
                .collect::<Vec<_>>();
            change = describe_change(&s, selection, source);
        }
        self.notify(listeners, &change);
        true
    }

    fn notify(&self, listeners: Vec<Listener>, change: &Change) {
        for l in listeners {
            l(change);
        }
        if let Some(ctx) = self.egui.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            ctx.request_repaint();
        }
    }

    /// The selection as the collector shows it.
    pub fn describe(&self) -> Json {
        let s = self.lock();
        json!({
            "items": s.items,
            "count": s.items.len(),
            "hovered": s.hovered.as_ref().map(|h| &h.item),
            "hovered_by": s.hovered.as_ref().map(|h| &h.source),
            "anchor": s.anchor,
            "revision": s.revision,
            "source": s.source,
            "back": s.back.len(),
            "forward": s.forward.len(),
        })
    }

    /// The revisions not published yet, marked published.
    fn unpublished(&self) -> Option<Change> {
        let mut s = self.lock();
        let now = (s.revision, s.hover_revision);
        if s.published == now {
            return None;
        }
        let selection = s.published.0 != now.0;
        s.published = now;
        let source = s.source.clone();
        Some(describe_change(&s, selection, &source))
    }
}

fn describe_change(s: &Inner, selection: bool, source: &str) -> Change {
    Change {
        items: s.items.clone(),
        hovered: s.hovered.as_ref().map(|h| h.item.clone()),
        revision: s.revision,
        selection,
        source: source.to_string(),
    }
}

/// The items of `order` from `a` to `b`, both included, in that order's
/// direction; `None` when either is not in it.
fn range(
    order: &[SelectionItem],
    a: &SelectionItem,
    b: &SelectionItem,
) -> Option<Vec<SelectionItem>> {
    let i = order.iter().position(|x| x == a)?;
    let j = order.iter().position(|x| x == b)?;
    let (lo, hi) = if i <= j { (i, j) } else { (j, i) };
    Some(order[lo..=hi].to_vec())
}

/// Draws `add` in the selection's focus surface ([`SURFACE`]): a widget
/// of the application's bound to the selection (a canvas) opens it so
/// Alt+Left and Alt+Right walk the selection's history while it has the
/// focus.
pub fn surface<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui_pop_core::focus::scope(ui, SURFACE, &[], add)
}

/// Sets the context keys and publishes what changed since the last call.
pub fn sync(app: &AppHandle) {
    let Some(s) = app.service(&SELECTION) else {
        return;
    };
    let say = |b: bool| if b { "true" } else { "false" };
    app.set_context(CONTEXT_HAS, say(!s.is_empty()));
    app.set_context(CONTEXT_COUNT, &s.len().to_string());
    app.set_context(CONTEXT_CAN_BACK, say(s.can_back()));
    app.set_context(CONTEXT_CAN_FORWARD, say(s.can_forward()));
    if let Some(change) = s.unpublished() {
        app.publish(
            "egui_pop.selection",
            None,
            EVENT_CHANGED,
            serde_json::to_value(&change).unwrap_or(Json::Null),
        );
    }
}

/// The application's selection, or why there is none.
fn service(app: &AppHandle) -> Result<Arc<SelectionService>, String> {
    app.service(&SELECTION)
        .ok_or_else(|| "the selection plugin is not connected".to_string())
}

/// The `items` argument: a list of ids (a single id is taken too).
fn items_arg(args: &Json) -> Result<Vec<SelectionItem>, String> {
    match args.get("items") {
        Some(Json::Array(a)) => a
            .iter()
            .map(|v| {
                v.as_str()
                    .map(SelectionItem::from)
                    .ok_or_else(|| format!("`items`: {v} is not a string"))
            })
            .collect(),
        Some(Json::String(s)) => Ok(vec![SelectionItem::from(s.as_str())]),
        Some(Json::Null) | None => Err("`items` is missing".into()),
        Some(v) => Err(format!("`items`: {v} is not a list of ids")),
    }
}

/// The application-wide selection: its service, commands, keys, context
/// keys and collector.
#[derive(Debug, Default)]
pub struct SelectionPlugin;

impl Plugin for SelectionPlugin {
    fn id(&self) -> PluginId {
        SELECTION_PLUGIN
    }

    fn title(&self) -> String {
        "Selection".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        const CAT: &str = "Selection";
        let provided = SelectionService::new();
        provided.set_egui(&ctx.app().egui());
        ctx.service(&SELECTION, provided)?;
        let shared = ctx.app().service(&SELECTION).expect("just provided");
        let done = |call: &crate::Call<'_>, changed: bool| -> Result<Json, String> {
            sync(call.app);
            let s = service(call.app)?;
            Ok(json!({ "changed": changed, "items": s.items(), "count": s.len() }))
        };
        ctx.command(
            CommandSpec::new(CMD_BACK, "Back to the previous selection", CAT)
                .icon("chevron-left")
                .description("Selects again what was selected before the last change.")
                .when(CONTEXT_CAN_BACK)
                .key_with("Alt+Left", BindOptions::surface(SURFACE)),
            move |call| {
                let changed = service(call.app)?.back("history");
                done(call, changed)
            },
        )?
        .command(
            CommandSpec::new(CMD_FORWARD, "Forward to the next selection", CAT)
                .icon("chevron-right")
                .description("Selects again what was selected before going back.")
                .when(CONTEXT_CAN_FORWARD)
                .key_with("Alt+Right", BindOptions::surface(SURFACE)),
            move |call| {
                let changed = service(call.app)?.forward("history");
                done(call, changed)
            },
        )?
        .command(
            CommandSpec::new(CMD_SET, "Select items", CAT)
                .description("Selects these items (`items`, their ids), and only them.")
                .hidden()
                .args(json!({ "items": ["photos/2024/0012.raw"] })),
            move |call| {
                let items = items_arg(call.args)?;
                let changed = service(call.app)?.set(items, SOURCE_COMMAND);
                done(call, changed)
            },
        )?
        .command(
            CommandSpec::new(CMD_ADD, "Add items to the selection", CAT)
                .description("Adds these items (`items`, their ids) to the selection.")
                .hidden()
                .args(json!({ "items": ["photos/2024/0013.raw"] })),
            move |call| {
                let items = items_arg(call.args)?;
                let changed = service(call.app)?.add(items, SOURCE_COMMAND);
                done(call, changed)
            },
        )?
        .command(
            CommandSpec::new(CMD_CLEAR, "Clear the selection", CAT)
                .description("Selects nothing.")
                .hidden(),
            move |call| {
                let changed = service(call.app)?.clear(SOURCE_COMMAND);
                done(call, changed)
            },
        )?
        .command(
            CommandSpec::new(CMD_HOVER, "Hover an item", CAT)
                .description("Hovers an item (`item`, its id; null for none) in every widget showing it, until a widget or another call hovers something else.")
                .hidden()
                .args(json!({ "item": "photos/2024/0012.raw" })),
            move |call| {
                let item = match call.args.get("item") {
                    Some(Json::String(s)) => Some(SelectionItem::from(s.as_str())),
                    Some(Json::Null) | None => None,
                    Some(v) => return Err(format!("`item`: {v} is not an id")),
                };
                let s = service(call.app)?;
                s.hover_until_changed(item.as_ref(), SOURCE_COMMAND);
                sync(call.app);
                Ok(json!({ "hovered": s.hovered() }))
            },
        )?;
        ctx.collector(
            egui_pop_core::debug::FnCollector::new(
                "selection",
                "The application-wide selection: the items selected (ids, in the order selected), their count, the item hovered and by which widget, the anchor of a range, the revision, who made the last change, and how many selections back and forward go to.",
            )
            .snapshot(move |_| shared.describe()),
        )?;
        sync(&ctx.app());
        ctx.on(On::Pass, |app, _| {
            if let Some(s) = app.service(&SELECTION) {
                // A hover not said this pass or the last: its widget is gone.
                let pass = app.egui().cumulative_pass_nr();
                s.expire_hover(pass.saturating_sub(1));
            }
            sync(app);
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(ids: &[&str]) -> Vec<SelectionItem> {
        ids.iter().map(|s| SelectionItem::from(*s)).collect()
    }

    #[test]
    fn gestures_replace_toggle_and_range() {
        let s = SelectionService::new();
        let order = items(&["a", "b", "c", "d", "e"]);
        s.click(&order[1], Gesture::Replace, &order, "t");
        s.click(&order[3], Gesture::Range, &order, "t");
        assert_eq!(s.items(), items(&["b", "c", "d"]));
        // Shift again ranges from the same anchor.
        s.click(&order[0], Gesture::Range, &order, "t");
        assert_eq!(s.items(), items(&["a", "b"]));
        s.click(&order[4], Gesture::Toggle, &order, "t");
        assert_eq!(s.items(), items(&["a", "b", "e"]));
        s.click(&order[0], Gesture::Toggle, &order, "t");
        assert_eq!(s.items(), items(&["b", "e"]));
        // Ctrl+Shift adds the range from the anchor (`a`, the last toggled).
        s.click(&order[2], Gesture::AddRange, &order, "t");
        assert_eq!(s.items(), items(&["b", "e", "a", "c"]));
        // An anchor outside this order: the item alone.
        let other = items(&["x", "y"]);
        s.click(&other[1], Gesture::Range, &other, "t");
        assert_eq!(s.items(), items(&["y"]));
        assert_eq!(Gesture::of(Modifiers::COMMAND), Gesture::Toggle);
        assert_eq!(Gesture::of(Modifiers::SHIFT), Gesture::Range);
        assert_eq!(Gesture::of(Modifiers::NONE), Gesture::Replace);
    }

    #[test]
    fn history_goes_back_and_forward_and_a_change_drops_forward() {
        let s = SelectionService::new();
        s.set(items(&["a"]), "t");
        s.set(items(&["b"]), "t");
        s.set(items(&["c"]), "t");
        assert!(s.back("h"));
        assert_eq!(s.items(), items(&["b"]));
        assert!(s.back("h"));
        assert_eq!(s.items(), items(&["a"]));
        assert!(s.forward("h"));
        assert_eq!(s.items(), items(&["b"]));
        assert!(s.can_forward());
        s.set(items(&["d"]), "t");
        assert!(!s.can_forward());
        assert!(!s.forward("h"));
        assert!(s.back("h"));
        assert_eq!(s.items(), items(&["b"]));
        // The same selection again is no change and no step.
        let rev = s.revision();
        assert!(!s.set(items(&["b"]), "t"));
        assert_eq!(s.revision(), rev);
    }

    #[test]
    fn hover_belongs_to_the_widget_that_said_it() {
        let s = SelectionService::new();
        let a = SelectionItem::from("a");
        s.hover("tree", Some(&a));
        assert!(s.is_hovered(&a));
        // Another widget hovering nothing leaves it.
        s.hover("list", None);
        assert_eq!(s.hovered(), Some(a.clone()));
        s.hover("tree", None);
        assert_eq!(s.hovered(), None);
        // Not said again: it expires.
        s.set_hover("tree", Some(&a), Some(4));
        s.expire_hover(4);
        assert!(s.is_hovered(&a));
        s.expire_hover(5);
        assert_eq!(s.hovered(), None);
        // An agent's stays.
        s.hover_until_changed(Some(&a), "command");
        s.expire_hover(1000);
        assert!(s.is_hovered(&a));
    }

    #[test]
    fn subscribers_hear_each_change_once() {
        let s = SelectionService::new();
        let heard = Arc::new(Mutex::new(Vec::new()));
        let h = heard.clone();
        let sub = s.subscribe(move |c| h.lock().unwrap().push((c.selection, c.items.len())));
        s.set(items(&["a", "b"]), "t");
        s.set(items(&["a", "b"]), "t");
        s.hover("t", Some(&SelectionItem::from("a")));
        s.unsubscribe(sub);
        s.clear("t");
        assert_eq!(*heard.lock().unwrap(), vec![(true, 2), (false, 2)]);
    }

    #[test]
    fn items_of_keys() {
        assert_eq!(SelectionItem::from_key("a/b").as_str(), "a/b");
        assert_eq!(SelectionItem::from_key(&12u32).as_str(), "12");
        assert_eq!(SelectionItem::from_key(&(1, 2)).as_str(), "[1,2]");
    }
}
