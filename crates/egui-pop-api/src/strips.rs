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
//! Steps (ADR 0011): `workbench.fold` (`{ "area", "folded" }`): an area
//! folded to its strip or unfolded.

use std::fmt;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Mutex;

use egui::{Color32, RichText, WidgetType};
use egui::{Sense, Ui, vec2};
use egui_pop_core::a11y;
use egui_pop_core::icon::Icon;
use egui_pop_core::indicator::Indicator as Mark;
use egui_pop_core::kit::{BoxStyle, Kit, KitCx, Part, Side, State as KitState, paint_indicator};
use egui_pop_core::lock;
use egui_pop_core::state::Slot;
use egui_pop_core::strings;
use egui_pop_core::theme::Tokens;
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::context::FrameSpec;
use egui_pop_model::context::ROOT;
use egui_pop_model::ids::{BlockId, CommandId, PluginId};
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

/// Whether a chip was hovered last pass (its tooltip then asks the
/// owner whether the mode may be left), per chip.
const CHIP_HOVERED: Slot<bool> = Slot::new("api.strips.chip_hovered");

/// Folds or unfolds the start side bar.
pub const CMD_TOGGLE_SIDE_BAR: CommandId = CommandId::from_static("view.side_bar.toggle");
/// Hides a status item where the user is, or shows it again.
pub const CMD_HIDE_ITEM: CommandId = CommandId::from_static("status.item.hide");
/// Moves a status item one step towards the bar's start or end.
pub const CMD_MOVE_ITEM: CommandId = CommandId::from_static("status.item.move");

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
        let value = if c.is_empty() {
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
/// its start, among the items the bar shows here.
pub fn move_item(app: &AppHandle, id: &str, toward_end: bool) {
    let mut order: Vec<String> = shown(app).into_iter().map(|(i, _)| i.id).collect();
    let Some(at) = order.iter().position(|o| o == id) else {
        return;
    };
    // From the end: towards the end is towards the list's front.
    let to = if toward_end {
        at.checked_sub(1)
    } else {
        Some(at + 1).filter(|t| *t < order.len())
    };
    let Some(to) = to else {
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

/// The indicators in the bar's order, from its end: the order chosen,
/// then the others, lightest first.
fn ordered(app: &AppHandle, choices: &Choices) -> Vec<(Indicator, IndicatorState)> {
    let mut list = app.indicators();
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
        if self.breadcrumb {
            ctx.block(
                zones::STATUS_BAR,
                Block::add(
                    BlockId::from_static("egui_pop.status_context"),
                    0,
                    status_context,
                ),
            );
        }
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
    let mut w = small_width(ui, &indicator_text(state)) + 2.0 * ui.spacing().button_padding.x;
    if state.busy.is_some() {
        w += Tokens::of(ui.ctx()).size.icon_inline + spacing;
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
    // The breadcrumb gives way first: the room left once the chips and
    // every indicator shown have theirs.
    let crumb_widths: Vec<f32> = crumbs
        .iter()
        .map(|c| small_width(ui, c) + 2.0 * pad + spacing)
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

/// The indicators, at the status bar's end. On a narrow bar the
/// heaviest give way.
fn indicators(ui: &mut Ui, app: &AppHandle) {
    let mut list = shown(app);
    let hidden: Vec<(String, String)> = {
        let c = choices(app);
        app.indicators()
            .into_iter()
            .filter(|(i, _)| c.hidden.contains(&i.id))
            .map(|(i, _)| (i.id, i.label))
            .collect()
    };
    if list.is_empty() && hidden.is_empty() {
        return;
    }
    let mut room = ui.available_width();
    let fits = list
        .iter()
        .take_while(|(_, s)| {
            room -= indicator_width(ui, s);
            room >= 0.0
        })
        .count();
    list.truncate(fits);
    let t = Tokens::of(ui.ctx());
    let kit = Kit::of(ui.ctx());
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        // Under the items: the bar's own menu, the hidden items to show.
        if !hidden.is_empty() {
            let bar = ui.interact(
                ui.max_rect(),
                ui.id().with("egui_pop_status_bar"),
                Sense::click(),
            );
            bar.context_menu(|ui| show_hidden_menu(ui, app, &hidden));
        }
        for (ind, state) in list {
            // The kit's `status-indicator`, colours only: quiet is its own
            // box, the others its tones.
            let box_ = kit.status_indicator.style_toned(state.tone, KitState::REST);
            let color = box_.text;
            let text = indicator_text(&state);
            let r = ui.add(
                egui::Button::new(egui::RichText::new(&text).color(color).small()).frame(false),
            );
            // Right to left: the spinner comes after the button in code,
            // before it on screen.
            let busy = egui_pop_core::motion::Busy::new(
                egui::Id::new(("egui_pop_indicator_busy", &ind.id)),
                state.busy.unwrap_or(0.0),
            )
            .visible(ui.ctx(), state.busy.is_some());
            if busy {
                let side = t.size.icon_inline;
                let (rect, _) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
                egui_pop_core::progress::paint_spinner(ui, rect, color);
            }
            if let Some(p) = state.progress {
                // The button carries the text; the line is drawn only:
                // the track in `border-color`, the fill in
                // `indicator-color`, along the bottom.
                let cx = KitCx {
                    painter: ui.painter(),
                    ppp: ui.ctx().pixels_per_point(),
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
            let r = if state.detail.is_empty() {
                r
            } else {
                r.on_hover_text(&state.detail)
            };
            r.context_menu(|ui| {
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
                    show_hidden_menu(ui, app, &hidden);
                }
            });
            if r.clicked()
                && let Some(c) = &ind.command
            {
                app.run(c.clone(), Json::Null);
            }
        }
    });
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
