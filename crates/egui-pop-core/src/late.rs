//! Late values: a cell for a value that arrives after asynchronous work
//! (a volume's free space, a folder's size, a count from a database),
//! in four states ([`Late`]):
//!
//! - **pending**: a [`Skeleton`] line, or a delayed [`Spinner`]
//!   ([`Waiting::Spinner`]), in a box sized like the expected value
//!   ([`LateValue::sample`], [`LateValue::size`]), so nothing moves when
//!   the value lands;
//! - **fresh**: the value, as text in a [`TextRole`] or drawn by a
//!   closure ([`LateValue::show_with`]);
//! - **refreshing**: the last value, a small spinner beside it;
//! - **failed**: an error icon, the reason and a Retry button;
//!   [`LateResponse::retry`] says it was clicked (the application runs
//!   the work again).
//!
//! The cell keeps the spinner's room in every state, so its size is the
//! same pending, fresh and refreshing. Its accessible node keeps the
//! value's name; its value says the state (the value, "Loading", the
//! error) and it is `busy` while the work runs.
//!
//! ```
//! use egui_pop_core::late::{Late, LateValue};
//!
//! fn ui(ui: &mut egui::Ui, free: &Late<String>) {
//!     let cell = LateValue::new("Free space", free.clone())
//!         .sample("000.0 GB")
//!         .show(ui);
//!     if cell.retry {
//!         // Run the work again.
//!     }
//! }
//! ```

use std::fmt::Display;

use egui::{
    Align, FontSelection, Id, Layout, Response, Role, Sense, TextWrapMode, Ui, Vec2, WidgetInfo,
    WidgetText, vec2,
};

use crate::icon::Icon;
use crate::placeholder::Skeleton;
use crate::progress::Spinner;
use crate::strings::{self, LATE_LOADING, LATE_RETRY, LATE_UPDATING, LOADING};
use crate::theme::{TextRole, Tokens};
use crate::ui::role_text;

/// Where a late value stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Late<T> {
    /// Not known yet: the work runs.
    Pending,
    /// Known.
    Fresh(T),
    /// Known, and being worked out again: the last value stays.
    Refreshing(T),
    /// The work failed, for this reason.
    Failed(String),
}

impl<T> Late<T> {
    /// The value known, fresh or refreshing.
    pub fn value(&self) -> Option<&T> {
        match self {
            Late::Fresh(v) | Late::Refreshing(v) => Some(v),
            Late::Pending | Late::Failed(_) => None,
        }
    }

    /// Whether the work runs (pending or refreshing).
    pub fn is_busy(&self) -> bool {
        matches!(self, Late::Pending | Late::Refreshing(_))
    }

    pub fn as_ref(&self) -> Late<&T> {
        match self {
            Late::Pending => Late::Pending,
            Late::Fresh(v) => Late::Fresh(v),
            Late::Refreshing(v) => Late::Refreshing(v),
            Late::Failed(e) => Late::Failed(e.clone()),
        }
    }

    /// The same state, the value mapped.
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Late<U> {
        match self {
            Late::Pending => Late::Pending,
            Late::Fresh(v) => Late::Fresh(f(v)),
            Late::Refreshing(v) => Late::Refreshing(f(v)),
            Late::Failed(e) => Late::Failed(e),
        }
    }
}

/// What stands in for a pending value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Waiting {
    /// A skeleton line, at once.
    #[default]
    Skeleton,
    /// A spinner, delayed when the work's start is known
    /// ([`LateValue::since`]).
    Spinner,
}

/// What [`LateValue::show`] returns.
#[derive(Debug)]
pub struct LateResponse {
    /// The whole cell.
    pub response: Response,
    /// Whether Retry was clicked (failed state only).
    pub retry: bool,
}

/// A cell showing a value that arrives late, in its four states.
#[derive(Clone, Debug)]
#[must_use = "show it with `show` or `show_with`"]
pub struct LateValue<T> {
    name: String,
    late: Late<T>,
    sample: Option<String>,
    size: Vec2,
    role: TextRole,
    waiting: Waiting,
    since: Option<f64>,
    id_salt: Option<Id>,
}

impl<T> LateValue<T> {
    /// The value's name ("Free space": its accessible name) and state.
    pub fn new(name: impl Into<String>, late: Late<T>) -> Self {
        Self {
            name: name.into(),
            late,
            sample: None,
            size: Vec2::ZERO,
            role: TextRole::Body,
            waiting: Waiting::Skeleton,
            since: None,
            id_salt: None,
        }
    }

    /// A text as wide as the value is expected to be ("000.0 GB"): the
    /// cell's room while pending, kept once the value lands.
    pub fn sample(mut self, text: impl Into<String>) -> Self {
        self.sample = Some(text.into());
        self
    }

    /// The value's room at least (for [`LateValue::show_with`], whose
    /// drawing the cell cannot measure beforehand).
    pub fn size(mut self, size: Vec2) -> Self {
        self.size = size;
        self
    }

    /// The text role of the value and of its sample ([`TextRole::Body`]
    /// by default).
    pub fn role(mut self, role: TextRole) -> Self {
        self.role = role;
        self
    }

    /// What stands in for a pending value.
    pub fn waiting(mut self, waiting: Waiting) -> Self {
        self.waiting = waiting;
        self
    }

    /// When the work started (motion clock seconds): the spinners then
    /// wait `motion.spinner.delay` before showing, and stay
    /// `motion.spinner.min` once shown ([`Spinner::delayed`]).
    pub fn since(mut self, seconds: f64) -> Self {
        self.since = Some(seconds);
        self
    }

    /// Tells cells of the same name apart (the delayed spinners' state).
    pub fn id_salt(mut self, salt: impl std::hash::Hash + std::fmt::Debug) -> Self {
        self.id_salt = Some(Id::NULL.with(salt));
        self
    }

    /// Draws the cell, the value drawn by `add` in the value's room.
    pub fn show_with(self, ui: &mut Ui, add: impl FnOnce(&mut Ui, &T)) -> LateResponse {
        self.cell(ui, None, add)
    }

    fn cell(
        self,
        ui: &mut Ui,
        value_text: Option<String>,
        add: impl FnOnce(&mut Ui, &T),
    ) -> LateResponse {
        let ctx = ui.ctx().clone();
        let t = Tokens::of(&ctx);
        let measure = |ui: &Ui, text: &str| {
            WidgetText::from(role_text(ui, self.role, text))
                .into_galley(
                    ui,
                    Some(TextWrapMode::Extend),
                    f32::INFINITY,
                    FontSelection::Default,
                )
                .size()
        };
        let row = measure(ui, self.sample.as_deref().unwrap_or("0"));
        let sample_w = if self.sample.is_some() { row.x } else { 0.0 };
        let text_w = value_text.as_deref().map_or(0.0, |s| measure(ui, s).x);
        let height = row.y.max(self.size.y);
        let width = sample_w.max(self.size.x).max(text_w).max(1.0);
        let side = (height * 0.8).round().max(8.0);
        let gap = t.space.s;
        let total = vec2(width + gap + side, height);
        let id = ui.scope_id().with(("late-value", &self.name, self.id_salt));
        let name = self.name.clone();
        let since = self.since;
        let spinner = |label: String| {
            let s = Spinner::new().size(side).name(label);
            match since {
                Some(at) => s.delayed(id, at, true),
                None => s,
            }
        };
        let mut retry = false;
        let inner = ui.allocate_ui_with_layout(total, Layout::left_to_right(Align::Center), |ui| {
            ui.set_min_size(total);
            ui.spacing_mut().item_spacing.x = gap;
            let room = vec2(width, height);
            let in_room = |ui: &mut Ui, add: &mut dyn FnMut(&mut Ui)| {
                ui.allocate_ui_with_layout(room, Layout::left_to_right(Align::Center), |ui| {
                    ui.set_min_size(room);
                    add(ui);
                });
            };
            let slot = |ui: &mut Ui, busy: Option<String>| {
                ui.allocate_ui_with_layout(
                    vec2(side, height),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.set_min_size(vec2(side, height));
                        if let Some(label) = busy {
                            ui.add(spinner(label));
                        }
                    },
                );
            };
            let tr = |key: &strings::StrKey| strings::tr_args(&ctx, key, &[("name", &name)]);
            match &self.late {
                Late::Pending => {
                    let label = tr(&LATE_LOADING);
                    in_room(ui, &mut |ui| match self.waiting {
                        Waiting::Skeleton => {
                            ui.add(Skeleton::text(1).width(width).label(label.clone()));
                        }
                        Waiting::Spinner => {
                            ui.add(spinner(label.clone()));
                        }
                    });
                    slot(ui, None);
                }
                Late::Fresh(v) => {
                    let mut add = Some(add);
                    in_room(ui, &mut |ui| {
                        if let Some(add) = add.take() {
                            add(ui, v);
                        }
                    });
                    slot(ui, None);
                }
                Late::Refreshing(v) => {
                    let mut add = Some(add);
                    in_room(ui, &mut |ui| {
                        if let Some(add) = add.take() {
                            add(ui, v);
                        }
                    });
                    slot(ui, Some(tr(&LATE_UPDATING)));
                }
                Late::Failed(error) => {
                    let danger = t.status.danger;
                    let (icon, _) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
                    if ui.is_rect_visible(icon) {
                        Icon::Error.paint(ui.painter(), icon, danger);
                    }
                    ui.add(
                        egui::Label::new(role_text(ui, self.role, error.as_str()).color(danger))
                            .wrap_mode(TextWrapMode::Extend),
                    );
                    let button = ui.small_button(strings::tr(&ctx, &strings::RETRY));
                    let retry_name = tr(&LATE_RETRY);
                    ctx.accesskit_node_builder(button.id, |node| node.set_label(retry_name));
                    retry = button.clicked();
                }
            }
        });
        let rect = inner.response.rect;
        let response = ui.interact(rect, id, Sense::hover());
        let enabled = ui.is_enabled();
        response.widget_info(|| WidgetInfo::labeled(Role::Label, enabled, &self.name));
        let value = match &self.late {
            Late::Pending => Some(strings::tr(&ctx, &LOADING)),
            Late::Fresh(_) | Late::Refreshing(_) => value_text,
            Late::Failed(error) => Some(error.clone()),
        };
        let busy = self.late.is_busy();
        ctx.accesskit_node_builder(response.id, |node| {
            match value {
                Some(v) => node.set_value(v),
                None => node.clear_value(),
            }
            if busy {
                node.set_busy();
            } else {
                node.clear_busy();
            }
        });
        LateResponse { response, retry }
    }
}

impl<T: Display> LateValue<T> {
    /// Draws the cell, the value as text in its role.
    pub fn show(self, ui: &mut Ui) -> LateResponse {
        let text = self.late.value().map(ToString::to_string);
        let role = self.role;
        let shown = text.clone();
        self.cell(ui, text, move |ui, _| {
            let text = shown.unwrap_or_default();
            ui.add(egui::Label::new(role_text(ui, role, text)).wrap_mode(TextWrapMode::Extend));
        })
    }
}
