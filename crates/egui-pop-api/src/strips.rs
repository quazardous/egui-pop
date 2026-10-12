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
//!     state.tone = if state.count == Some(0) { Tone::Quiet } else { Tone::Active };
//! });
//! # let _ = jobs;
//! ```
//!
//! Steps (ADR 0011): `workbench.fold` (`{ "area", "folded" }`): an area
//! folded to its strip or unfolded.

use std::fmt;
use std::rc::Rc;

use egui::{Sense, Ui, vec2};
use egui_pop_core::icon::Icon;
use egui_pop_core::theme::Tokens;
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::ids::{BlockId, CommandId, PluginId};
use egui_pop_model::panels::Area;
use serde::Serialize;
use serde_json::{Value as Json, json};

use crate::app::{AppHandle, MENU_VIEW};
use crate::host::Event;
use crate::zones::{self, Block};
use crate::{Ctx, Plugin, PluginError};

/// Folds or unfolds the start side bar.
pub const CMD_TOGGLE_SIDE_BAR: CommandId = CommandId::from_static("view.side_bar.toggle");

/// How an indicator looks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Tone {
    /// Nothing to say: drawn muted.
    #[default]
    Quiet,
    /// Something is going on.
    Active,
    Success,
    Warning,
    Error,
}

/// What an indicator shows.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct IndicatorState {
    pub text: String,
    pub tone: Tone,
    /// A count, shown as a badge.
    pub count: Option<u64>,
    /// Progress from 0 to 1, shown as a bar under the text.
    pub progress: Option<f32>,
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
    pub(crate) reduce: ReduceFn,
    pub(crate) initial: IndicatorState,
}

impl fmt::Debug for Indicator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Indicator")
            .field("id", &self.id)
            .field("prefix", &self.prefix)
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
            reduce: Rc::new(|_, _| {}),
            initial: IndicatorState {
                text: label.into(),
                ..IndicatorState::default()
            },
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
}

/// The plugin.
#[derive(Clone, Debug)]
pub struct StripsPlugin {
    activity_bar: bool,
}

impl Default for StripsPlugin {
    fn default() -> Self {
        Self { activity_bar: true }
    }
}

impl StripsPlugin {
    /// The status bar's indicators only, no activity bar.
    pub fn status_only() -> Self {
        Self {
            activity_bar: false,
        }
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
            let on = !folded && shown.contains(&p.id);
            let side = t.size.icon_panel + 2.0 * t.space.s;
            let (rect, r) = ui.allocate_exact_size(vec2(side, side), Sense::click());
            egui_pop_core::a11y::name(&r, egui::WidgetType::Button, &p.title);
            if on {
                ui.painter()
                    .rect_filled(rect, t.radius.widget, t.accent.subtle);
            } else if r.hovered() {
                ui.painter()
                    .rect_filled(rect, t.radius.widget, t.surface.hover);
            }
            let color = if on || r.hovered() {
                t.text.strong
            } else {
                t.text.muted
            };
            let inner = rect.shrink(t.space.s);
            match icon {
                Some(i) => egui_pop_core::icon::Source::from(i).paint(ui, inner, color),
                None => {
                    let letter = p.title.chars().next().unwrap_or('?').to_string();
                    ui.painter().text(
                        inner.center(),
                        egui::Align2::CENTER_CENTER,
                        letter,
                        egui::FontId::proportional(t.font.heading),
                        color,
                    );
                }
            }
            let r = r.on_hover_text(&p.title);
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

/// The indicators, at the status bar's end.
fn indicators(ui: &mut Ui, app: &AppHandle) {
    let list = app.indicators();
    if list.is_empty() {
        return;
    }
    let t = Tokens::of(ui.ctx());
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        for (ind, state) in list {
            let color = match state.tone {
                Tone::Quiet => t.text.muted,
                Tone::Active => t.accent.default,
                Tone::Success => t.status.success,
                Tone::Warning => t.status.warning,
                Tone::Error => t.status.danger,
            };
            let mut text = state.text.clone();
            if let Some(n) = state.count {
                text = format!("{text} ({n})");
            }
            let r = ui.add(
                egui::Button::new(egui::RichText::new(&text).color(color).small()).frame(false),
            );
            if let Some(p) = state.progress {
                let bar = egui::Rect::from_min_size(
                    egui::pos2(r.rect.left(), r.rect.bottom() - 2.0),
                    vec2(r.rect.width() * p.clamp(0.0, 1.0), 2.0),
                );
                ui.painter().rect_filled(bar, 0.0, color);
            }
            let r = if state.detail.is_empty() {
                r
            } else {
                r.on_hover_text(&state.detail)
            };
            if r.clicked()
                && let Some(c) = &ind.command
            {
                app.run(c.clone(), Json::Null);
            }
        }
    });
}
