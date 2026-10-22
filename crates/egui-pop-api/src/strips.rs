//! Slim strips, as a plugin ([`StripsPlugin`], opt-in): the **activity
//! bar**, a column of icons along the start edge, one per panel that
//! lives in the start side bar (an icon shows its panel, a click on the
//! shown one folds the side bar to the strip, `Ctrl+B` too), and the
//! **status bar** with **indicators**: small items whose state is fed by
//! the host's events (a job's progress, a connection, a count), each
//! with an optional command run on a click.
//!
//! An indicator is a reducer over the journal: egui-pop hands it every
//! new event whose name starts with its prefix, in order, and draws its
//! state. Nothing polls: the host publishes, the indicator follows.
//!
//! ```
//! use egui_pop_api::strips::{Indicator, IndicatorState, Tone};
//!
//! // Jobs: `job.started` adds one, `job.done` removes one.
//! let jobs = Indicator::new("jobs", "Jobs", "job.").reduce(|state, event| {
//!     let n = state.count.unwrap_or(0);
//!     match event.name.as_str() {
//!         "job.started" => state.count = Some(n + 1),
//!         "job.done" => state.count = Some(n.saturating_sub(1)),
//!         _ => {}
//!     }
//!     state.text = format!("{} running", state.count.unwrap_or(0));
//!     state.tone = if state.count == Some(0) { Tone::Neutral } else { Tone::Accent };
//! });
//! # let _ = jobs;
//! ```
//!
//! An indicator may follow the context ([`Indicator::when`]): a `when`
//! clause over the context keys (the context stack's `place`,
//! `mode.<group>`, a frame's own keys), the grammar of commands. The bar
//! shows it only where its clause holds; hidden, it keeps reducing
//! events, so its state is right the moment it shows again.
//! [`Indicator::during`] writes the clause of a frame for it.
//!
//! With the context stack ([`crate::contexts`]) installed too, the
//! status bar starts with **where the user is**: the breadcrumb (a click
//! on a crumb goes back to that place; the current one does nothing),
//! then a chip per active mode of a group declared
//! [`FrameSpec::visible`] (a click leaves the mode; a pushed mode shows a
//! back mark and goes back to what it replaced; a mode whose guard
//! refuses keeps its chip and says why in its tooltip). On a narrow
//! window the middle crumbs collapse into `…` (a menu of them) first,
//! then the heaviest indicators give way; chips never hide, since they
//! show active states. [`StripsPlugin::breadcrumb`] turns this off.
//!
//! The user hides status items and orders them (an item's context
//! menu, or `status.item.hide` and `status.item.move`); only status
//! items can be muted. The choices ([`Choices`]) are kept per place of
//! the context stack, in its `view` document, so a bar arranged for the
//! darkroom comes back as it was; a place without choices of its own
//! takes the root's. Without the context stack they last the session.
//!
//! Agents read what the bar shows: [`status`] lists every item with
//! why it is hidden (its `when`, the user, a narrow bar), `context.read`
//! gives the visible ones in its `status` field, and the `strips` debug
//! collector shows the breadcrumb, the chips, the items and the choices.
//!
//! Steps (ADR 0011): `workbench.fold` (`{ "area", "folded" }`): an area
//! folded to its strip or unfolded.

use std::fmt;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Mutex;

use egui::{Color32, Context, Rect, RichText, WidgetType};
use egui::{Sense, Ui, vec2};
use egui_pop_core::a11y::{self, Politeness};
use egui_pop_core::debug::FnCollector;
use egui_pop_core::hover_card::{self, Card, HoverCard};
use egui_pop_core::icon::Icon;
use egui_pop_core::indicator::Indicator as Mark;
use egui_pop_core::kit::{BoxStyle, Kit, KitCx, Part, Side, State as KitState, paint_indicator};
use egui_pop_core::lock;
use egui_pop_core::overlay::{self, Kind as OverlayKind, OverlaySpec};
use egui_pop_core::placement::Side as Toward;
use egui_pop_core::state::Slot;
use egui_pop_core::strings;
use egui_pop_core::theme::Tokens;
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::context::FrameSpec;
use egui_pop_model::context::ROOT;
use egui_pop_model::ids::{BlockId, CommandId, PanelId, PluginId};
use egui_pop_model::keymap::BindOptions;
use egui_pop_model::panels::Area;
use egui_pop_model::when::{When, WhenContext};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};

use crate::app::{AppHandle, MENU_VIEW};
use crate::contexts::{self, Chip, Envelope};
use crate::host::Event;
use crate::services::ServiceToken;
use crate::strings as txt;
use crate::zones::{self, Block};
use crate::{Ctx, Plugin, PluginError};

/// The indicators the last pass left out for want of room.
const NARROWED: Slot<Vec<String>> = Slot::new("api.strips.narrowed");

/// The pass [`NARROWED`] was worked out in.
const NARROWED_PASS: Slot<u64> = Slot::new("api.strips.narrowed_pass");

/// The status item whose popover is open.
const OPEN: Slot<String> = Slot::new("api.strips.open");

/// Where a status item was drawn last (the overflow button for one that
/// gave way), per item: its popover's anchor.
const ANCHOR: Slot<Rect> = Slot::new("api.strips.anchor");

/// The status item with the keyboard focus last pass.
const FOCUSED: Slot<String> = Slot::new("api.strips.focused");

/// An item's errors last pass, and since when it shows errors (motion
/// clock seconds), per item.
const ERRORS_SEEN: Slot<(u64, f64)> = Slot::new("api.strips.errors_seen");

/// Whether a chip was hovered last pass (its tooltip then asks the
/// owner whether the mode may be left), per chip.
const CHIP_HOVERED: Slot<bool> = Slot::new("api.strips.chip_hovered");

/// Folds or unfolds the start side bar.
pub const CMD_TOGGLE_SIDE_BAR: CommandId = CommandId::from_static("view.side_bar.toggle");
/// Hides a status item where the user is, or shows it again.
pub const CMD_HIDE_ITEM: CommandId = CommandId::from_static("status.item.hide");
/// Moves a status item one step towards the bar's start or end.
pub const CMD_MOVE_ITEM: CommandId = CommandId::from_static("status.item.move");
/// Opens a status item's popover, or closes it.
pub const CMD_OPEN_ITEM: CommandId = CommandId::from_static("status.item.open");
/// Opens a status item's hover card, as hovering it does.
pub const CMD_HOVER_ITEM: CommandId = CommandId::from_static("status.item.hover");

/// The status bar's state: the choices when there is no context stack
/// to keep them per place.
pub const STRIPS: ServiceToken<StripsService> = ServiceToken::new("app.strips");

/// The name the choices are kept under, per place
/// (`contexts::place_value`).
pub const CHOICES_VALUE: &str = "egui_pop.status";

/// The user's choices for the status bar in a place: the items hidden,
/// and their order from the bar's end (the items not listed follow, by
/// weight).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Choices {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hidden: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub order: Vec<String>,
}

impl Choices {
    /// Whether nothing was chosen.
    pub fn is_empty(&self) -> bool {
        self.hidden.is_empty() && self.order.is_empty()
    }
}

/// The status bar's own state.
#[derive(Debug, Default)]
pub struct StripsService {
    /// The choices, without a context stack.
    session: Mutex<Choices>,
}

/// The choices of the place shown: its own, else the root's (the
/// session's without a context stack).
pub fn choices(app: &AppHandle) -> Choices {
    if app.service(&contexts::CONTEXTS).is_some() {
        let read = |v: Option<Json>| v.and_then(|v| serde_json::from_value::<Choices>(v).ok());
        return read(contexts::place_value(app, CHOICES_VALUE))
            .or_else(|| read(contexts::place_value_of(app, ROOT, CHOICES_VALUE)))
            .unwrap_or_default();
    }
    app.service(&STRIPS)
        .map(|s| lock(&s.session).clone())
        .unwrap_or_default()
}

/// Keeps the choices for the place shown.
fn set_choices(app: &AppHandle, c: Choices) {
    if app.service(&contexts::CONTEXTS).is_some() {
        // Forgetting a place's choices hands it the root's: an empty
        // choice is kept as such where the root has choices to inherit.
        let at_root = contexts::state(app).is_none_or(|s| s.place() == ROOT);
        let root_chose = contexts::place_value_of(app, ROOT, CHOICES_VALUE).is_some();
        let value = if c.is_empty() && (at_root || !root_chose) {
            Json::Null
        } else {
            serde_json::to_value(&c).unwrap_or(Json::Null)
        };
        contexts::set_place_value(app, CHOICES_VALUE, value);
    } else if let Some(s) = app.service(&STRIPS) {
        *lock(&s.session) = c;
    }
    app.egui().request_repaint();
}

/// Hides a status item where the user is, or shows it again.
pub fn hide(app: &AppHandle, id: &str, hidden: bool) {
    let mut c = choices(app);
    c.hidden.retain(|h| h != id);
    if hidden {
        c.hidden.push(id.to_string());
    }
    set_choices(app, c);
}

/// Moves a status item one step towards the bar's end (`toward_end`) or
/// its start, among the items of its group (start or end) the bar shows
/// here.
pub fn move_item(app: &AppHandle, id: &str, toward_end: bool) {
    let list = shown(app);
    let mut order: Vec<String> = list.iter().map(|(i, _)| i.id.clone()).collect();
    let Some(at) = order.iter().position(|o| o == id) else {
        return;
    };
    let align = list[at].0.align;
    let group: Vec<usize> = list
        .iter()
        .enumerate()
        .filter(|(_, (i, _))| i.align == align)
        .map(|(k, _)| k)
        .collect();
    let g = group.iter().position(|k| *k == at).unwrap_or_default();
    // The end group is listed from the bar's end, the start group from
    // its start.
    let later = match align {
        Alignment::End => !toward_end,
        Alignment::Start => toward_end,
    };
    let to = if later {
        group.get(g + 1)
    } else {
        g.checked_sub(1).and_then(|g| group.get(g))
    };
    let Some(&to) = to else {
        return;
    };
    order.swap(at, to);
    let mut c = choices(app);
    // The items hidden or not shown here keep their places after.
    let rest: Vec<String> = c
        .order
        .iter()
        .filter(|o| !order.contains(o))
        .cloned()
        .collect();
    order.extend(rest);
    c.order = order;
    set_choices(app, c);
}

/// Why a status item is not shown.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HiddenBy {
    /// Its `when` clause does not hold here (or does not parse).
    When(String),
    /// The user hid it here.
    User,
    /// The bar was too narrow for it last pass.
    Narrow,
}

/// A status item as the bar has it now.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StatusEntry {
    pub id: String,
    pub text: String,
    pub tone: Tone,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<f32>,
    /// Which end of the bar it sits at.
    pub align: Alignment,
    /// The higher, the longer it stays on a narrow bar.
    #[serde(skip_serializing_if = "is_zero_i32")]
    pub priority: i32,
    /// Busy: a spinner before its text.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub busy: bool,
    #[serde(skip_serializing_if = "is_zero")]
    pub errors: u64,
    #[serde(skip_serializing_if = "is_zero")]
    pub warnings: u64,
    /// It opens a popover on a click (`status.item.open`).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub popover: bool,
    /// Its popover is open.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub open: bool,
    /// Its hover card, as hovering it shows it now.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card: Option<Card>,
    /// Why it is not shown; `None` when it is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden_by: Option<HiddenBy>,
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

fn is_zero_i32(n: &i32) -> bool {
    *n == 0
}

/// Every status item in the bar's order from its end, with why it is
/// hidden. Empty without the strips plugin.
pub fn status(app: &AppHandle) -> Vec<StatusEntry> {
    if app.service(&STRIPS).is_none() {
        return Vec::new();
    }
    let cx = app.context();
    let c = choices(app);
    let narrowed = NARROWED.get(&app.egui()).unwrap_or_default();
    let open = OPEN.get(&app.egui());
    ordered(app, &c)
        .into_iter()
        .map(|(i, s)| {
            let hidden_by = if !i.applies(&cx) {
                Some(HiddenBy::When(i.when.clone().unwrap_or_default()))
            } else if c.hidden.contains(&i.id) {
                Some(HiddenBy::User)
            } else if narrowed.contains(&i.id) {
                Some(HiddenBy::Narrow)
            } else {
                None
            };
            let card = i.card(&s, app);
            StatusEntry {
                open: open.as_deref() == Some(i.id.as_str()),
                popover: i.popover.is_some(),
                align: i.align,
                priority: i.priority,
                busy: s.busy.is_some(),
                errors: s.errors,
                warnings: s.warnings,
                card,
                id: i.id,
                text: s.text,
                tone: s.tone,
                count: s.count,
                progress: s.progress,
                hidden_by,
            }
        })
        .collect()
}

/// The status items the bar shows, as `context.read` gives them.
pub fn visible(app: &AppHandle) -> Vec<StatusEntry> {
    status(app)
        .into_iter()
        .filter(|i| i.hidden_by.is_none())
        .collect()
}

/// The status item whose popover is open.
pub fn open_item(app: &AppHandle) -> Option<String> {
    OPEN.get(&app.egui())
}

/// Opens a status item's popover (closing any other), or closes it.
/// False when there is no such item, or it has no popover.
pub fn open_popover(app: &AppHandle, id: &str, open: bool) -> bool {
    let ctx = app.egui();
    if !app
        .indicators()
        .iter()
        .any(|(i, _)| i.id == id && i.popover.is_some())
    {
        return false;
    }
    if open {
        hover_card::close(&ctx, card_id(id));
        OPEN.set(&ctx, id.to_string());
    } else if OPEN.get(&ctx).as_deref() == Some(id) {
        OPEN.remove(&ctx);
    }
    ctx.request_repaint();
    true
}

/// The status item with the keyboard focus last pass.
pub fn focused_item(app: &AppHandle) -> Option<String> {
    FOCUSED.get(&app.egui())
}

/// Opens a status item's hover card as the keyboard does (the focus on
/// its first link). False when it has no card, or is not in the bar.
pub fn open_card(app: &AppHandle, id: &str) -> bool {
    let shown = status(app)
        .into_iter()
        .any(|e| e.id == id && e.card.is_some() && e.hidden_by.is_none());
    if shown {
        hover_card::open(&app.egui(), card_id(id));
    }
    shown
}

/// A status item's hover card.
fn card_id(id: &str) -> egui::Id {
    HoverCard::new(("egui_pop.status_card", id)).id()
}

/// What the `strips` collector shows.
fn describe(app: &AppHandle) -> Json {
    let place = contexts::state(app).map(|s| s.place().to_string());
    json!({
        "breadcrumb": contexts::crumbs(app),
        "breadcrumb_icons": contexts::crumb_icons(app),
        "chips": contexts::chips(app),
        "items": status(app),
        "open": OPEN.get(&app.egui()),
        "place": place,
        "choices": choices(app),
    })
}

/// The indicators in the bar's order, from its end: the order chosen,
/// then the others, lightest first.
fn ordered(app: &AppHandle, choices: &Choices) -> Vec<(Indicator, IndicatorState)> {
    // A panel's indicator shows on its strip icon, not in the bar.
    let mut list: Vec<_> = app
        .indicators()
        .into_iter()
        .filter(|(i, _)| i.panel.is_none())
        .collect();
    let rank = |id: &str| {
        choices
            .order
            .iter()
            .position(|o| o == id)
            .unwrap_or(usize::MAX)
    };
    // Stable: the weight order stays among the items not ordered.
    list.sort_by_key(|(i, _)| rank(&i.id));
    list
}

/// How an indicator looks: egui-pop's one tone ([`egui_pop_core::kit::Tone`]).
/// `Neutral` (nothing to say) is the `status-indicator`'s own box, drawn
/// muted; `Accent` says something is going on; `Record` that something
/// records.
pub use egui_pop_core::kit::Tone;

/// What an indicator shows.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct IndicatorState {
    pub text: String,
    pub tone: Tone,
    /// A count, shown as a badge.
    pub count: Option<u64>,
    /// Progress from 0 to 1, shown as a thin bar under the text.
    pub progress: Option<f32>,
    /// Errors to report: a count after the text, with the error icon in
    /// the danger tone; turning to errors pulses once and is announced.
    pub errors: u64,
    /// Warnings to report: a count with the warning icon.
    pub warnings: u64,
    /// Busy since (motion clock seconds, `egui_pop_core::motion::now`):
    /// a spinner before the text after `motion.spinner.delay`, kept
    /// `motion.spinner.min`.
    pub busy: Option<f64>,
    /// A tooltip.
    pub detail: String,
    /// The last event that changed it.
    pub seq: u64,
}

/// A reducer: updates a state from an event.
pub type ReduceFn = Rc<dyn Fn(&mut IndicatorState, &Event)>;

/// A popover's content: a small layout drawn while it is open.
pub type PopoverFn = Rc<dyn Fn(&mut Ui, &AppHandle)>;

/// A hover card's content, built from the item's state when shown (and
/// when an agent reads it).
pub type HoverFn = Rc<dyn Fn(&IndicatorState, &AppHandle) -> Card>;

/// Which end of the status bar an item sits at.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Alignment {
    /// After the breadcrumb and the chips, lightest first.
    Start,
    /// At the bar's end, lightest last (nearest the end).
    #[default]
    End,
}

/// An indicator, as declared.
#[derive(Clone)]
pub struct Indicator {
    pub id: String,
    pub label: String,
    /// The event names it follows (a prefix: `job.`).
    pub prefix: String,
    /// Run on a click.
    pub command: Option<CommandId>,
    /// Lighter first, from the end of the status bar.
    pub weight: i32,
    /// Where it shows: a `when` clause over the context keys (none:
    /// everywhere).
    pub when: Option<String>,
    /// Which end of the bar it sits at.
    pub align: Alignment,
    /// On a narrow bar, the items of higher priority stay longer; among
    /// equals, the lighter.
    pub priority: i32,
    /// The panel it speaks for ([`Indicator::panel`]): shown on that
    /// panel's icon in a folded side area's strip instead of in the
    /// status bar.
    pub panel: Option<PanelId>,
    pub(crate) popover: Option<PopoverFn>,
    pub(crate) hover: Option<HoverFn>,
    pub(crate) reduce: ReduceFn,
    pub(crate) initial: IndicatorState,
    /// The clause, parsed once.
    parsed: Option<Rc<Result<When, String>>>,
}

impl fmt::Debug for Indicator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Indicator")
            .field("id", &self.id)
            .field("prefix", &self.prefix)
            .field("when", &self.when)
            .field("align", &self.align)
            .field("priority", &self.priority)
            .field("panel", &self.panel)
            .field("popover", &self.popover.is_some())
            .field("hover", &self.hover.is_some())
            .finish_non_exhaustive()
    }
}

impl Indicator {
    /// An indicator following the events named `prefix...`; its text is
    /// `label` until an event says otherwise.
    pub fn new(id: &str, label: &str, prefix: &str) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            prefix: prefix.into(),
            command: None,
            weight: 0,
            when: None,
            align: Alignment::End,
            priority: 0,
            panel: None,
            popover: None,
            hover: None,
            reduce: Rc::new(|_, _| {}),
            initial: IndicatorState {
                text: label.into(),
                ..IndicatorState::default()
            },
            parsed: None,
        }
    }

    /// How an event changes the state.
    pub fn reduce(mut self, f: impl Fn(&mut IndicatorState, &Event) + 'static) -> Self {
        self.reduce = Rc::new(f);
        self
    }

    /// Runs a command on a click.
    pub fn command(mut self, id: CommandId) -> Self {
        self.command = Some(id);
        self
    }

    pub fn weight(mut self, weight: i32) -> Self {
        self.weight = weight;
        self
    }

    /// Which end of the bar it sits at (the end by default).
    pub fn align(mut self, align: Alignment) -> Self {
        self.align = align;
        self
    }

    /// How long it stays on a narrow bar: the higher, the longer (0 by
    /// default). Items that give way go into the bar's `…` menu.
    pub fn priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Speaks for a panel, keyed by its id: while the panel is not
    /// shown (its side area folded, or another tab in front), the
    /// strip of its folded area shows the state on the panel's icon (a
    /// count as a badge, else errors or warnings, else a dot in its
    /// tone; `detail` joins the icon's tooltip). It is not a status bar
    /// item; agents read it in `workbench::strip` and the `workbench`
    /// collector.
    pub fn panel(mut self, panel: &PanelId) -> Self {
        self.panel = Some(panel.clone());
        self
    }

    /// A popover opened by a click (Enter from the keyboard, or
    /// `status.item.open`): a small layout next to the item, closed by
    /// Escape or a click outside, the focus back on the item. It takes
    /// the click from [`Indicator::command`], which its content can
    /// still run.
    pub fn popover(mut self, f: impl Fn(&mut Ui, &AppHandle) + 'static) -> Self {
        self.popover = Some(Rc::new(f));
        self
    }

    /// A rich hover card ([`Card`]: formatted text, rows, live values,
    /// command links) in place of the plain tooltip (`detail`), built
    /// from the state each time it shows. `Ctrl+K Ctrl+I` opens it on
    /// the item with the keyboard focus; agents read it in [`status`].
    pub fn hover(mut self, f: impl Fn(&IndicatorState, &AppHandle) -> Card + 'static) -> Self {
        self.hover = Some(Rc::new(f));
        self
    }

    /// Its hover card in a state (none without [`Indicator::hover`], or
    /// when the card is empty).
    pub fn card(&self, state: &IndicatorState, app: &AppHandle) -> Option<Card> {
        self.hover
            .as_ref()
            .map(|f| f(state, app))
            .filter(|c| !c.is_empty())
    }

    /// The state before any event.
    pub fn initial(mut self, state: IndicatorState) -> Self {
        self.initial = state;
        self
    }

    /// Shows it only where a `when` clause holds over the context keys
    /// (`place == 'photos.album'`, `mode.edit == 'masks'`), the grammar
    /// of commands. Hidden, it keeps reducing events; only its drawing
    /// is skipped. A clause that does not parse keeps it hidden and is
    /// reported when it is added (`indicator-when`).
    pub fn when(mut self, clause: &str) -> Self {
        self.when = Some(clause.into());
        self.parsed = Some(Rc::new(When::parse(clause)));
        self
    }

    /// Shows it only while a frame of the context stack is on: its
    /// clause ([`FrameSpec::clause`]: the top place, or the mode on in
    /// its group), so a frame's own status items need no clause written
    /// by hand.
    pub fn during(self, frame: &FrameSpec) -> Self {
        self.when(&frame.clause())
    }

    /// Its clause, parsed (`None` without one).
    pub fn clause(&self) -> Option<Result<&When, &str>> {
        self.parsed
            .as_deref()
            .map(|r| r.as_ref().map_err(String::as_str))
    }

    /// Whether its clause holds over the context keys (always without
    /// one; never when it does not parse).
    pub fn applies(&self, cx: &WhenContext) -> bool {
        match self.clause() {
            None => true,
            Some(Ok(w)) => w.eval(cx),
            Some(Err(_)) => false,
        }
    }
}

/// The plugin.
#[derive(Clone, Debug)]
pub struct StripsPlugin {
    activity_bar: bool,
    breadcrumb: bool,
}

impl Default for StripsPlugin {
    fn default() -> Self {
        Self {
            activity_bar: true,
            breadcrumb: true,
        }
    }
}

impl StripsPlugin {
    /// The status bar's indicators only, no activity bar.
    pub fn status_only() -> Self {
        Self {
            activity_bar: false,
            ..Self::default()
        }
    }

    /// Whether the status bar starts with the context stack's breadcrumb
    /// and mode chips, when the context plugin is installed (on by
    /// default). An application showing them elsewhere turns it off.
    pub fn breadcrumb(mut self, on: bool) -> Self {
        self.breadcrumb = on;
        self
    }
}

impl Plugin for StripsPlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("egui_pop.strips")
    }

    fn title(&self) -> String {
        "Strips".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.service(&STRIPS, StripsService::default())?;
        let w = ctx.app().downgrade();
        ctx.collector(
            FnCollector::new(
                "strips",
                "The status bar: the breadcrumb and the mode chips at its start (`chips`: each mode on, `visible` when its group shows in the bar), every status item in the bar's order from its end with its state and why it is hidden (`hidden_by`: the `when` clause that does not hold, `user`, `narrow`), the place shown and the user's choices there.",
            )
            .snapshot(move |_| match w.get() {
                Ok(app) => describe(&app),
                Err(_) => Json::Null,
            }),
        )?;
        ctx.command(
            CommandSpec::new(CMD_HIDE_ITEM, "Hide a status item", "View")
                .description("Hides a status bar item (`id`) where the user is, or shows it again (`hidden`: false).")
                .args(json!({ "id": "jobs", "hidden": true })),
            |call| {
                let id = call.args["id"].as_str().ok_or("`id` is missing")?;
                if !call.app.indicators().iter().any(|(i, _)| i.id == id) {
                    return Err(format!("no status item `{id}`"));
                }
                let hidden = call.args["hidden"].as_bool().unwrap_or(true);
                hide(call.app, id, hidden);
                Ok(json!({ "hidden": hidden }))
            },
        )?
        .command(
            CommandSpec::new(CMD_MOVE_ITEM, "Move a status item", "View")
                .description("Moves a status bar item (`id`) one step towards its start or end (`toward`: start or end), where the user is.")
                .args(json!({ "id": "jobs", "toward": "end" })),
            |call| {
                let id = call.args["id"].as_str().ok_or("`id` is missing")?;
                let toward_end = match call.args["toward"].as_str() {
                    Some("end") => true,
                    Some("start") => false,
                    _ => return Err("`toward` is start or end".into()),
                };
                move_item(call.app, id, toward_end);
                Ok(json!({ "order": choices(call.app).order }))
            },
        )?
        .command(
            CommandSpec::new(CMD_OPEN_ITEM, "Open a status item", "View")
                .description("Opens the popover of a status bar item (`id`), as a click on it does, or closes it (`open`: false).")
                .args(json!({ "id": "jobs", "open": true })),
            |call| {
                let id = call.args["id"].as_str().ok_or("`id` is missing")?;
                let open = call.args["open"].as_bool().unwrap_or(true);
                if !open_popover(call.app, id, open) {
                    return Err(format!("no status item `{id}` with a popover"));
                }
                Ok(json!({ "open": open }))
            },
        )?
        .command(
            CommandSpec::new(CMD_HOVER_ITEM, "Show a status item's card", "View")
                .description("Opens the hover card of a status bar item (`id`; by default the one with the keyboard focus), as hovering it does. Escape closes it.")
                .key_with("Ctrl+K Ctrl+I", BindOptions::surface("statusBar"))
                .args(json!({ "id": "jobs" })),
            |call| {
                let id = match call.args["id"].as_str() {
                    Some(id) => id.to_string(),
                    None => focused_item(call.app).ok_or("`id` is missing")?,
                };
                if !open_card(call.app, &id) {
                    return Err(format!("no status item `{id}` with a card in the bar"));
                }
                Ok(json!({ "id": id }))
            },
        )?;
        ctx.command(
            CommandSpec::new(CMD_TOGGLE_SIDE_BAR, "Fold the side bar", "View")
                .description("Folds the start side bar to its strip, or unfolds it.")
                .key("Ctrl+B")
                .menu(MENU_VIEW, "3_layout", 0),
            |call| {
                let folded = call.app.layout().folded.contains(&Area::StartSide);
                call.app.fold_area(Area::StartSide, !folded);
                Ok(json!({ "folded": !folded }))
            },
        )?;
        if self.activity_bar {
            ctx.block(
                zones::ACTIVITY_BAR,
                Block::add(BlockId::from_static("egui_pop.activity"), 0, activity_bar),
            );
        }
        // The bar's start: the breadcrumb and the chips, then the items
        // aligned at the start.
        let breadcrumb = self.breadcrumb;
        ctx.block(
            zones::STATUS_BAR,
            Block::add(
                BlockId::from_static("egui_pop.status_context"),
                0,
                move |ui, app| {
                    if breadcrumb {
                        status_context(ui, app);
                    }
                    start_items(ui, app);
                },
            ),
        );
        ctx.block(
            zones::STATUS_BAR,
            Block::add(
                BlockId::from_static("egui_pop.indicators"),
                1000,
                indicators,
            ),
        );
        Ok(())
    }
}

/// The activity bar: an icon per panel of the start side.
fn activity_bar(ui: &mut Ui, app: &AppHandle) {
    let t = Tokens::of(ui.ctx());
    let kit = Kit::of(ui.ctx());
    let layout = app.layout();
    let folded = layout.folded.contains(&Area::StartSide);
    let shown: Vec<_> = layout
        .active_in(Area::StartSide)
        .into_iter()
        .cloned()
        .collect();
    let panels: Vec<_> = app
        .panels()
        .into_iter()
        .filter(|p| !p.bare)
        .filter(|p| {
            layout
                .area_of(&p.id)
                .map_or(p.area == Area::StartSide, |a| a == Area::StartSide)
        })
        .collect();
    ui.vertical_centered(|ui| {
        for p in panels {
            let icon = Icon::by_name(&p.icon);
            let title = egui_pop_core::strings::panel_title(ui.ctx(), p.id.as_str(), &p.title);
            let on = !folded && shown.contains(&p.id);
            let side = t.size.icon_panel + 2.0 * t.space.s;
            let (rect, r) = ui.allocate_exact_size(vec2(side, side), Sense::click());
            egui_pop_core::a11y::name(&r, egui::WidgetType::Button, &title);
            // Selected, never hovered then: the shown panel's item keeps
            // its look under the pointer.
            let state = KitState::REST
                .with(KitState::HOVER, r.hovered() && !on)
                .with(KitState::ACTIVE, r.is_pointer_button_down_on())
                .with(KitState::FOCUS, r.has_focus())
                .with(KitState::SELECTED, on);
            let part = Part {
                rect,
                room: rect,
                state,
                label: None,
            };
            let cx = KitCx {
                painter: ui.painter(),
                ppp: ui.ctx().pixels_per_point(),
            };
            kit.activity_bar_item.paint(&cx, &part);
            let color = kit.activity_bar_item.style(state).text;
            let inner = rect.shrink(t.space.s);
            match icon {
                Some(i) => egui_pop_core::icon::Source::from(i).paint(ui, inner, color),
                None => {
                    let letter = title.chars().next().unwrap_or('?').to_string();
                    ui.painter().text(
                        inner.center(),
                        egui::Align2::CENTER_CENTER,
                        letter,
                        egui::FontId::proportional(t.font.heading),
                        color,
                    );
                }
            }
            let r = r.on_hover_text(&title);
            if r.clicked() {
                if on {
                    app.fold_area(Area::StartSide, true);
                } else {
                    app.focus_panel(&p.id);
                    if folded {
                        app.fold_area(Area::StartSide, false);
                    }
                }
            }
        }
    });
}

/// The indicators the bar shows, from its end: those whose clause holds
/// and that the user did not hide here, in the order chosen.
fn shown(app: &AppHandle) -> Vec<(Indicator, IndicatorState)> {
    let cx = app.context();
    let c = choices(app);
    ordered(app, &c)
        .into_iter()
        .filter(|(i, _)| i.applies(&cx) && !c.hidden.contains(&i.id))
        .collect()
}

/// A text's width in the status bar's small font.
fn small_width(ui: &Ui, text: &str) -> f32 {
    let font = egui::TextStyle::Small.resolve(ui.style());
    ui.ctx()
        .fonts_mut(|f| f.layout_no_wrap(text.to_string(), font, Color32::PLACEHOLDER))
        .size()
        .x
}

/// What an indicator says, its count after its text.
fn indicator_text(state: &IndicatorState) -> String {
    match state.count {
        Some(n) => format!("{} ({n})", state.text),
        None => state.text.clone(),
    }
}

/// The room an indicator takes in the bar, its spacing included.
fn indicator_width(ui: &Ui, state: &IndicatorState) -> f32 {
    let spacing = ui.spacing().item_spacing.x;
    let icon = Tokens::of(ui.ctx()).size.icon_inline;
    let mut w = small_width(ui, &indicator_text(state)) + 2.0 * ui.spacing().button_padding.x;
    if state.busy.is_some() {
        w += icon + spacing;
    }
    for n in [state.errors, state.warnings] {
        if n > 0 {
            w += icon + small_width(ui, &n.to_string()) + 2.0 * spacing;
        }
    }
    w + spacing
}

/// A part of the breadcrumb as drawn: a crumb, or `…` standing for the
/// crumbs collapsed in it.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Seg {
    Crumb(usize),
    More(Range<usize>),
}

/// The breadcrumb's parts that fit in `budget`: every crumb, else the
/// root, `…` and as many crumbs from the end as fit, else `…` and the
/// last crumb. `widths` are the crumbs' (spacing included), `sep` a
/// separator's, `more` the `…` crumb's.
fn fit_crumbs(widths: &[f32], sep: f32, more: f32, budget: f32) -> Vec<Seg> {
    let n = widths.len();
    let all: Vec<Seg> = (0..n).map(Seg::Crumb).collect();
    let width = |segs: &[Seg]| -> f32 {
        let w: f32 = segs
            .iter()
            .map(|s| match s {
                Seg::Crumb(i) => widths[*i],
                Seg::More(_) => more,
            })
            .sum();
        w + sep * segs.len().saturating_sub(1) as f32
    };
    if n <= 2 || width(&all) <= budget {
        return all;
    }
    for tail in (1..=n - 2).rev() {
        let mut segs = vec![Seg::Crumb(0), Seg::More(1..n - tail)];
        segs.extend((n - tail..n).map(Seg::Crumb));
        if width(&segs) <= budget {
            return segs;
        }
    }
    vec![Seg::More(0..n - 1), Seg::Crumb(n - 1)]
}

/// A chip's text: its title, after a back mark for a pushed mode.
fn chip_text(chip: &Chip) -> String {
    if chip.pushed {
        format!("‹ {}", chip.title)
    } else {
        chip.title.clone()
    }
}

/// The status bar's start: where the user is (the breadcrumb) and what is
/// active (a chip per mode of a visible group).
fn status_context(ui: &mut Ui, app: &AppHandle) {
    let Some(svc) = app.service(&contexts::CONTEXTS) else {
        return;
    };
    let crumbs = contexts::crumbs(app);
    if crumbs.is_empty() {
        return;
    }
    let chips: Vec<Chip> = contexts::chips(app)
        .into_iter()
        .filter(|c| c.visible)
        .collect();
    let t = Tokens::of(ui.ctx());
    let spacing = ui.spacing().item_spacing.x;
    let pad = ui.spacing().button_padding.x;
    let icons = crumb_icons(app, &t, crumbs.len());
    // The breadcrumb gives way first: the room left once the chips and
    // every indicator shown have theirs.
    let crumb_widths: Vec<f32> = crumbs
        .iter()
        .zip(&icons)
        .map(|(c, icon)| {
            let mark = icon.map_or(0.0, |_| t.size.icon_inline + spacing);
            small_width(ui, c) + 2.0 * pad + spacing + mark
        })
        .collect();
    let sep = small_width(ui, "›") + spacing;
    let more = small_width(ui, "…") + 2.0 * pad + spacing;
    let chips_width: f32 = chips
        .iter()
        .map(|c| {
            let icon = if c.icon.is_some() {
                t.size.icon_inline + spacing
            } else {
                0.0
            };
            small_width(ui, &chip_text(c)) + 2.0 * t.space.s + spacing + icon
        })
        .sum();
    let indicators_width: f32 = shown(app).iter().map(|(_, s)| indicator_width(ui, s)).sum();
    let budget = ui.available_width() - chips_width - indicators_width - t.space.s;
    let segs = fit_crumbs(&crumb_widths, sep, more, budget);
    let here = crumbs.len() - 1;
    let mut jump_to = None;
    for (k, seg) in segs.iter().enumerate() {
        if k > 0 {
            ui.label(RichText::new("›").small().weak());
        }
        if let Seg::Crumb(i) = seg
            && let Some(icon) = icons[*i]
        {
            let tone = if *i == here {
                egui_pop_core::icon::Tone::Strong
            } else {
                egui_pop_core::icon::Tone::Muted
            };
            ui.add(egui_pop_core::icon::IconWidget::new(icon).tone(tone));
            egui_pop_core::icon::registry::record(ui.ctx(), "breadcrumb", &crumbs[*i], icon.name());
        }
        match seg {
            Seg::Crumb(i) if *i == here => {
                let r = ui.label(RichText::new(&crumbs[*i]).small().strong());
                let name =
                    strings::tr_args(ui.ctx(), &txt::CONTEXT_HERE, &[("place", &crumbs[*i])]);
                a11y::name(&r, WidgetType::Label, &name);
            }
            Seg::Crumb(i) => {
                let name =
                    strings::tr_args(ui.ctx(), &txt::CONTEXT_GO_TO, &[("place", &crumbs[*i])]);
                let r = ui.add(egui::Button::new(RichText::new(&crumbs[*i]).small()).frame(false));
                a11y::name(&r, WidgetType::Button, &name);
                if r.on_hover_text(name).clicked() {
                    jump_to = Some(*i);
                }
            }
            Seg::More(range) => {
                let name = strings::tr(ui.ctx(), &txt::CONTEXT_MORE_PLACES);
                let r = ui.menu_button(RichText::new("…").small(), |ui| {
                    for i in range.clone() {
                        let go = strings::tr_args(
                            ui.ctx(),
                            &txt::CONTEXT_GO_TO,
                            &[("place", &crumbs[i])],
                        );
                        let b = ui.button(&crumbs[i]);
                        a11y::name(&b, WidgetType::Button, &go);
                        if b.clicked() {
                            jump_to = Some(i);
                            ui.close();
                        }
                    }
                });
                a11y::name(&r.response, WidgetType::Button, &name);
                r.response.on_hover_text(name);
            }
        }
    }
    if svc.switching() {
        ui.spinner();
    }
    if !chips.is_empty() {
        ui.add_space(t.space.s);
    }
    let mut leave = None;
    for chip in &chips {
        let tone = chip
            .tone
            .as_deref()
            .and_then(Tone::from_name)
            .unwrap_or(Tone::Accent);
        if let Some(icon) = chip.icon.as_deref().and_then(Icon::by_name) {
            ui.add(Mark::icon(icon).tone(tone));
        }
        let ctx = ui.ctx().clone();
        let action = strings::tr_args(
            &ctx,
            if chip.pushed {
                &txt::CONTEXT_BACK_FROM_MODE
            } else {
                &txt::CONTEXT_LEAVE_MODE
            },
            &[("mode", &chip.title)],
        );
        let hovered = CHIP_HOVERED.keyed(("egui_pop.status_chip", chip.frame.as_str()));
        // Asked only under the pointer: a guard is the owner's code.
        let tip = if hovered.get(&ctx) == Some(true) {
            contexts::why_not(app, chip.leave()).map(|reason| {
                strings::tr_args(
                    &ctx,
                    &txt::CONTEXT_CANNOT_LEAVE,
                    &[("mode", &chip.title), ("reason", &reason)],
                )
            })
        } else {
            None
        };
        let r = ui.add(
            Mark::text(chip_text(chip))
                .tone(tone)
                .name(chip.title.clone())
                .interactive(action.clone())
                .tooltip(tip.unwrap_or(action)),
        );
        hovered.set(&ctx, r.hovered());
        if r.clicked() {
            leave = Some(chip.leave());
        }
    }
    if let Some(depth) = jump_to {
        contexts::jump(app, depth);
    }
    if let Some(request) = leave {
        contexts::request(app, Envelope::new(request), &Origin::User);
    }
}

/// Each crumb's icon: its place's (`FrameSpec::icon`, the placeholder for
/// a name that does not resolve: the `unknown-icon` diagnostic says it),
/// none in the compact density, where the bar has no room for them.
fn crumb_icons(app: &AppHandle, t: &Tokens, n: usize) -> Vec<Option<Icon>> {
    if t.resolved().spec().density == egui_pop_core::theme::Density::Compact {
        return vec![None; n];
    }
    let mut icons: Vec<Option<Icon>> = contexts::crumb_icons(app)
        .into_iter()
        .map(|name| {
            name.map(|n| {
                egui_pop_core::icon::registry::lookup(&n)
                    .unwrap_or(egui_pop_core::icon::PLACEHOLDER)
            })
        })
        .collect();
    icons.resize(n, None);
    icons
}

/// The items that give way in `room`, by index: the items are kept by
/// priority (highest first), then in the bar's order (lightest first),
/// while they fit; once one gives way, the overflow button takes `more`.
/// `items` are each item's priority and width, in the bar's order.
fn fit(items: &[(i32, f32)], room: f32, more: f32) -> Vec<usize> {
    let mut order: Vec<usize> = (0..items.len()).collect();
    // Stable: the bar's order among equals.
    order.sort_by_key(|k| std::cmp::Reverse(items[*k].0));
    let kept = |room: f32| {
        let mut left = room;
        order
            .iter()
            .take_while(|k| {
                left -= items[**k].1;
                left >= 0.0
            })
            .count()
    };
    let mut n = kept(room);
    if n < items.len() {
        n = kept(room - more);
    }
    let mut out = order[n..].to_vec();
    out.sort_unstable();
    out
}

/// The room the overflow button takes.
fn more_width(ui: &Ui) -> f32 {
    small_width(ui, "…") + 2.0 * ui.spacing().button_padding.x + ui.spacing().item_spacing.x
}

/// The items shown that give way this pass, worked out once per pass by
/// the first block drawing items (the bar's start), in the room left
/// after the breadcrumb and the chips.
fn narrowed(ui: &Ui, list: &[(Indicator, IndicatorState)]) -> Vec<String> {
    let ctx = ui.ctx();
    let pass = ctx.cumulative_pass_nr();
    if NARROWED_PASS.get(ctx) == Some(pass) {
        return NARROWED.get(ctx).unwrap_or_default();
    }
    let items: Vec<(i32, f32)> = list
        .iter()
        .map(|(i, s)| (i.priority, indicator_width(ui, s)))
        .collect();
    let out: Vec<String> = fit(&items, ui.available_width(), more_width(ui))
        .into_iter()
        .map(|k| list[k].0.id.clone())
        .collect();
    NARROWED.set(ctx, out.clone());
    NARROWED_PASS.set(ctx, pass);
    out
}

/// The items hidden by the user here: their ids and labels.
fn user_hidden(app: &AppHandle) -> Vec<(String, String)> {
    let c = choices(app);
    app.indicators()
        .into_iter()
        .filter(|(i, _)| c.hidden.contains(&i.id))
        .map(|(i, _)| (i.id, i.label))
        .collect()
}

/// The items aligned at the bar's start, after the breadcrumb and the
/// chips, lightest first.
fn start_items(ui: &mut Ui, app: &AppHandle) {
    let list = shown(app);
    if list.is_empty() {
        return;
    }
    let narrowed = narrowed(ui, &list);
    let hidden = user_hidden(app);
    for (ind, state) in &list {
        if ind.align == Alignment::Start && !narrowed.contains(&ind.id) {
            item(ui, app, ind, state, &hidden, false);
        }
    }
}

/// The indicators at the status bar's end, then the `…` menu of the
/// items that gave way on a narrow bar.
fn indicators(ui: &mut Ui, app: &AppHandle) {
    let list = shown(app);
    let hidden = user_hidden(app);
    let open = OPEN.get(ui.ctx());
    if list.is_empty() {
        if open.is_some() {
            OPEN.remove(ui.ctx());
        }
        if hidden.is_empty() {
            return;
        }
    }
    let narrowed = narrowed(ui, &list);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        // Under the items: the bar's own menu, the hidden items to show.
        if !hidden.is_empty() {
            let bar = ui.interact(
                ui.max_rect(),
                ui.id().with("egui_pop_status_bar"),
                Sense::click(),
            );
            egui_pop_core::pointer::claim::context_menu(&bar, |ui| {
                show_hidden_menu(ui, app, &hidden)
            });
        }
        for (ind, state) in &list {
            if ind.align == Alignment::End && !narrowed.contains(&ind.id) {
                item(ui, app, ind, state, &hidden, true);
            }
        }
        let gave_way: Vec<&(Indicator, IndicatorState)> = list
            .iter()
            .filter(|(i, _)| narrowed.contains(&i.id))
            .collect();
        if !gave_way.is_empty() {
            overflow(ui, app, &gave_way);
        }
    });
    // An item no longer in the bar closes its popover.
    if let Some(open) = open
        && !list.iter().any(|(i, _)| i.id == open)
    {
        OPEN.remove(ui.ctx());
    }
}

/// The `…` button: a menu of the items that gave way; an entry acts as a
/// click on its item, a popover opening next to the button.
fn overflow(ui: &mut Ui, app: &AppHandle, gave_way: &[&(Indicator, IndicatorState)]) {
    let ctx = ui.ctx().clone();
    let name = strings::tr(&ctx, &txt::STATUS_MORE);
    let r = ui.menu_button(RichText::new("…").small(), |ui| {
        for (ind, state) in gave_way {
            if ui.button(indicator_text(state)).clicked() {
                activate(app, ind);
                ui.close();
            }
        }
    });
    a11y::name(&r.response, WidgetType::Button, &name);
    let rect = r.response.rect;
    r.response.on_hover_text(name);
    let open = OPEN.get(&ctx);
    for (ind, _) in gave_way {
        ANCHOR.keyed(ind.id.as_str()).set(&ctx, rect);
        if open.as_deref() == Some(ind.id.as_str()) {
            popover(&ctx, app, ind, rect);
        }
    }
}

/// A click on an item: its popover opens or closes, else its command
/// runs.
fn activate(app: &AppHandle, ind: &Indicator) {
    if ind.popover.is_some() {
        let open = OPEN.get(&app.egui()).as_deref() == Some(ind.id.as_str());
        open_popover(app, &ind.id, !open);
    } else if let Some(c) = &ind.command {
        app.run(c.clone(), Json::Null);
    }
}

/// An item's popover, next to `anchor`, while it is open: Escape or a
/// click outside closes it (the overlay stack's rules), the focus going
/// back to the item.
fn popover(ctx: &Context, app: &AppHandle, ind: &Indicator, anchor: Rect) {
    let Some(f) = ind.popover.clone() else {
        return;
    };
    let spec = OverlaySpec::new(
        ("egui_pop.status_popover", ind.id.as_str()),
        &ind.label,
        OverlayKind::Popover,
    );
    let mut open = true;
    let width = Tokens::of(ctx).size.dialog_width.min(420.0);
    overlay::show(
        ctx,
        &spec,
        &mut open,
        Some(overlay::Anchor {
            rect: anchor,
            side: Toward::Top,
        }),
        |ui| {
            ui.set_max_width(width);
            f(ui, app);
        },
    );
    if !open {
        OPEN.remove(ctx);
    }
}

/// Since when an item shows errors (motion clock seconds), for its
/// pulse; turning to errors is announced once, politely.
fn errors_since(ctx: &Context, ind: &Indicator, state: &IndicatorState) -> Option<f64> {
    let slot = ERRORS_SEEN.keyed(ind.id.as_str());
    let (before, since) = slot.get(ctx).unwrap_or((0, 0.0));
    let since = if state.errors > 0 && before == 0 {
        let what = strings::tr_args(
            ctx,
            &txt::STATUS_ERRORS,
            &[("count", &state.errors.to_string())],
        );
        let text = strings::tr_args(
            ctx,
            &txt::STATUS_ANNOUNCE,
            &[("item", &ind.label), ("state", &what)],
        );
        a11y::announce(ctx, text, Politeness::Polite);
        egui_pop_core::motion::now(ctx)
    } else {
        since
    };
    if before != state.errors {
        slot.set(ctx, (state.errors, since));
    }
    (state.errors > 0).then_some(since)
}

/// What an item says to a screen reader beside its text: its counts, its
/// progress, its card (else its tooltip).
fn item_description(ctx: &Context, state: &IndicatorState, card: Option<&Card>) -> String {
    let mut parts = Vec::new();
    if state.errors > 0 {
        parts.push(strings::tr_args(
            ctx,
            &txt::STATUS_ERRORS,
            &[("count", &state.errors.to_string())],
        ));
    }
    if state.warnings > 0 {
        parts.push(strings::tr_args(
            ctx,
            &txt::STATUS_WARNINGS,
            &[("count", &state.warnings.to_string())],
        ));
    }
    if let Some(p) = state.progress {
        parts.push(format!("{}%", (p.clamp(0.0, 1.0) * 100.0).round()));
    }
    match card {
        Some(c) => parts.push(c.plain_text()),
        None if !state.detail.is_empty() => parts.push(state.detail.clone()),
        None => {}
    }
    parts.join("\n")
}

/// A part of an item, in reading order.
#[derive(Clone, Copy)]
enum ItemPart {
    Spinner,
    Text,
    Errors,
    Warnings,
}

/// One status item: a spinner while busy, its text (a progress line
/// under it), its error and warning counts; its context menu, its
/// popover and its hover card. `rtl`: drawn in a right-to-left layout
/// (the bar's end), its parts then given in reverse.
fn item(
    ui: &mut Ui,
    app: &AppHandle,
    ind: &Indicator,
    state: &IndicatorState,
    hidden: &[(String, String)],
    rtl: bool,
) {
    let ctx = ui.ctx().clone();
    let t = Tokens::of(&ctx);
    let kit = Kit::of(&ctx);
    // The kit's `status-indicator`, colours only: quiet is its own box,
    // the others its tones.
    let box_ = kit.status_indicator.style_toned(state.tone, KitState::REST);
    let color = box_.text;
    let busy = egui_pop_core::motion::Busy::new(
        egui::Id::new(("egui_pop_indicator_busy", &ind.id)),
        state.busy.unwrap_or(0.0),
    )
    .visible(&ctx, state.busy.is_some());
    let since = errors_since(&ctx, ind, state);
    let mut parts = [
        ItemPart::Spinner,
        ItemPart::Text,
        ItemPart::Errors,
        ItemPart::Warnings,
    ];
    if rtl {
        parts.reverse();
    }
    let count = |ui: &mut Ui, icon: Icon, tone: Tone, n: u64, key, attention: Option<f64>| {
        let name = strings::tr_args(ui.ctx(), key, &[("count", &n.to_string())]);
        let mut mark = Mark::icon(icon).tone(tone).name(name);
        if let Some(s) = attention {
            mark = mark.attention(s);
        }
        let text = RichText::new(n.to_string())
            .small()
            .color(tone.color(ui.ctx()));
        if rtl {
            ui.label(text);
            ui.add(mark);
        } else {
            ui.add(mark);
            ui.label(text);
        }
    };
    let mut button = None;
    for part in parts {
        match part {
            ItemPart::Spinner if busy => {
                let side = t.size.icon_inline;
                let (rect, _) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
                egui_pop_core::progress::paint_spinner(ui, rect, color);
            }
            ItemPart::Text => {
                let r = ui.add(
                    egui::Button::new(RichText::new(indicator_text(state)).color(color).small())
                        .frame(false),
                );
                if let Some(p) = state.progress {
                    // The button carries the text; the line is drawn
                    // only: the track in `border-color`, the fill in
                    // `indicator-color`, along the bottom.
                    let cx = KitCx {
                        painter: ui.painter(),
                        ppp: ctx.pixels_per_point(),
                    };
                    let track = BoxStyle {
                        indicator_color: box_.border_color,
                        ..*box_
                    };
                    paint_indicator(&cx, r.rect, &track, Side::Bottom);
                    let mut fill = r.rect;
                    fill.set_width(r.rect.width() * p.clamp(0.0, 1.0));
                    paint_indicator(&cx, fill, box_, Side::Bottom);
                }
                button = Some(r);
            }
            ItemPart::Errors if state.errors > 0 => {
                count(
                    ui,
                    Icon::Error,
                    Tone::Danger,
                    state.errors,
                    &txt::STATUS_ERRORS,
                    since,
                );
            }
            ItemPart::Warnings if state.warnings > 0 => {
                count(
                    ui,
                    Icon::Warning,
                    Tone::Warning,
                    state.warnings,
                    &txt::STATUS_WARNINGS,
                    None,
                );
            }
            _ => {}
        }
    }
    let Some(mut r) = button else {
        return;
    };
    ANCHOR.keyed(ind.id.as_str()).set(&ctx, r.rect);
    if r.has_focus() {
        FOCUSED.set(&ctx, ind.id.clone());
    }
    let is_open = OPEN.get(&ctx).as_deref() == Some(ind.id.as_str());
    // A rich card, else the plain tooltip; neither over an open popover.
    let card = ind.card(state, app);
    match &card {
        Some(c) if !is_open => {
            if let Some(link) = HoverCard::new(("egui_pop.status_card", ind.id.as_str()))
                .side(Toward::Top)
                .show(ui, &r, c)
            {
                app.run(link.command, link.args);
            }
        }
        Some(_) => hover_card::close(&ctx, card_id(&ind.id)),
        None if !is_open && !state.detail.is_empty() => r = r.on_hover_text(&state.detail),
        None => {}
    }
    let description = item_description(&ctx, state, card.as_ref());
    if !description.is_empty() {
        ctx.accesskit_node_builder(r.id, |node| node.set_description(description));
    }
    egui_pop_core::pointer::claim::context_menu(&r, |ui| {
        let hide = strings::tr_args(ui.ctx(), &txt::STATUS_HIDE, &[("item", &ind.label)]);
        if ui.button(hide).clicked() {
            app.run(CMD_HIDE_ITEM, json!({ "id": ind.id, "hidden": true }));
            ui.close();
        }
        if ui
            .button(strings::tr(ui.ctx(), &txt::STATUS_MOVE_START))
            .clicked()
        {
            app.run(CMD_MOVE_ITEM, json!({ "id": ind.id, "toward": "start" }));
            ui.close();
        }
        if ui
            .button(strings::tr(ui.ctx(), &txt::STATUS_MOVE_END))
            .clicked()
        {
            app.run(CMD_MOVE_ITEM, json!({ "id": ind.id, "toward": "end" }));
            ui.close();
        }
        if !hidden.is_empty() {
            ui.separator();
            show_hidden_menu(ui, app, hidden);
        }
    });
    if r.clicked() {
        activate(app, ind);
    }
    // Drawn right after its item, so Tab goes from the item into it.
    if OPEN.get(&ctx).as_deref() == Some(ind.id.as_str()) {
        popover(&ctx, app, ind, r.rect);
    }
}

/// A menu's entries showing the hidden items again.
fn show_hidden_menu(ui: &mut Ui, app: &AppHandle, hidden: &[(String, String)]) {
    for (id, label) in hidden {
        let show = strings::tr_args(ui.ctx(), &txt::STATUS_SHOW, &[("item", label)]);
        if ui.button(show).clicked() {
            app.run(CMD_HIDE_ITEM, json!({ "id": id, "hidden": false }));
            ui.close();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_give_way_by_priority_then_the_heaviest_first() {
        // Equal priorities: the bar's order, the lightest stays.
        let items = [(0, 10.0), (0, 10.0), (0, 10.0)];
        assert!(fit(&items, 30.0, 5.0).is_empty());
        // Once one gives way, the overflow button takes its room too.
        assert_eq!(fit(&items, 29.0, 5.0), [2]);
        assert_eq!(fit(&items, 19.0, 5.0), [1, 2]);
        // A heavier item of higher priority stays before lighter ones.
        let items = [(0, 10.0), (0, 10.0), (5, 10.0)];
        assert_eq!(fit(&items, 25.0, 5.0), [1]);
        assert_eq!(fit(&items, 15.0, 5.0), [0, 1]);
        // Nothing fits: everything is in the menu.
        assert_eq!(fit(&items, 2.0, 5.0), [0, 1, 2]);
    }

    #[test]
    fn the_middle_crumbs_collapse_first() {
        let w = [10.0; 5];
        let all: Vec<Seg> = (0..5).map(Seg::Crumb).collect();
        assert_eq!(fit_crumbs(&w, 1.0, 5.0, 100.0), all);
        // 10 + 5 + 10 * 3 + 4 separators.
        assert_eq!(
            fit_crumbs(&w, 1.0, 5.0, 49.0),
            vec![
                Seg::Crumb(0),
                Seg::More(1..2),
                Seg::Crumb(2),
                Seg::Crumb(3),
                Seg::Crumb(4)
            ]
        );
        assert_eq!(
            fit_crumbs(&w, 1.0, 5.0, 27.0),
            vec![Seg::Crumb(0), Seg::More(1..4), Seg::Crumb(4)]
        );
        // Not even that: `…` and where the user is.
        assert_eq!(
            fit_crumbs(&w, 1.0, 5.0, 1.0),
            vec![Seg::More(0..4), Seg::Crumb(4)]
        );
        // Two crumbs never collapse.
        assert_eq!(
            fit_crumbs(&w[..2], 1.0, 5.0, 1.0),
            vec![Seg::Crumb(0), Seg::Crumb(1)]
        );
    }
}
