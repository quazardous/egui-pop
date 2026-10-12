//! Toasts, as a plugin ([`ToastsPlugin`], in `DefaultPlugins`): short
//! notices in a corner that never take the focus, each with a level, an
//! optional action (a command, run through the host) and a close button.
//! Information and success go after a few seconds (the pointer over
//! them holds them), warnings after longer, errors stay until closed.
//! Screen readers hear them through the live region (errors at once).
//!
//! The queue is data: [`Toasts::shown`] and [`Toasts::history`], the
//! debug bridge's `toasts` collector (with `dismiss`), and a
//! `toast.shown` event in the journal.
//!
//! Owned: egui-notify and egui-toast (both MIT) keep a queue of strings,
//! with no command actions and no state another plugin or an agent can
//! read; their stacking ideas are reused.
//!
//! ```
//! use egui_pop_api::toasts::{self, Toast};
//! use egui_pop_api::App;
//! use egui_pop_model::ids::CommandId;
//!
//! let app = App::builder()
//!     .plugin(toasts::ToastsPlugin)
//!     .build(&egui::Context::default())
//!     .unwrap();
//! let t = app.handle().service(&toasts::TOASTS).unwrap();
//! t.notify(Toast::success("Exported 3 images")
//!     .action("Show", CommandId::from_static("files.reveal"), serde_json::json!({ "path": "out" })));
//! assert_eq!(t.shown().len(), 1);
//! ```

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use egui::{Align2, Id, Order, Ui};
use egui_pop_core::a11y::{self, Politeness};
use egui_pop_core::debug::FnCollector;
use egui_pop_core::icon::{Icon, Size, Tone};
use egui_pop_core::lock;
use egui_pop_core::strings;
use egui_pop_core::theme::Tokens;
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::{BlockId, CommandId, PluginId};
use serde::Serialize;
use serde_json::{Value as Json, json};

use crate::app::AppHandle;
use crate::services::ServiceToken;
use crate::strings as txt;
use crate::zones::{self, Block};
use crate::{Ctx, Plugin, PluginError};

/// The toasts, as a service.
pub const TOASTS: ServiceToken<Toasts> = ServiceToken::new("app.toasts");

/// How many toasts show at once (the others wait).
pub const SHOWN: usize = 4;
/// How many past toasts are kept.
pub const HISTORY: usize = 32;

/// How much a toast matters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Info,
    Success,
    Warning,
    Error,
}

impl Level {
    /// Seconds before it goes; none: until closed.
    pub fn lifetime(self) -> Option<f64> {
        match self {
            Level::Info | Level::Success => Some(4.0),
            Level::Warning => Some(8.0),
            Level::Error => None,
        }
    }

    fn icon(self) -> (Icon, Tone) {
        match self {
            Level::Info => (Icon::Info, Tone::Info),
            Level::Success => (Icon::Success, Tone::Success),
            Level::Warning => (Icon::Warning, Tone::Warning),
            Level::Error => (Icon::Error, Tone::Danger),
        }
    }
}

/// A command offered with a toast.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ToastAction {
    pub label: String,
    pub command: CommandId,
    pub args: Json,
}

/// A notice.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Toast {
    pub level: Level,
    pub text: String,
    pub action: Option<ToastAction>,
    /// Seconds before it goes (the level's by default); none: until
    /// closed.
    pub lifetime: Option<f64>,
}

impl Toast {
    pub fn new(level: Level, text: &str) -> Self {
        Self {
            level,
            text: text.into(),
            action: None,
            lifetime: level.lifetime(),
        }
    }

    pub fn info(text: &str) -> Self {
        Self::new(Level::Info, text)
    }

    pub fn success(text: &str) -> Self {
        Self::new(Level::Success, text)
    }

    pub fn warning(text: &str) -> Self {
        Self::new(Level::Warning, text)
    }

    pub fn error(text: &str) -> Self {
        Self::new(Level::Error, text)
    }

    /// Offers a command; running it closes the toast.
    pub fn action(mut self, label: &str, command: CommandId, args: Json) -> Self {
        self.action = Some(ToastAction {
            label: label.into(),
            command,
            args,
        });
        self
    }

    /// Stays until closed.
    pub fn sticky(mut self) -> Self {
        self.lifetime = None;
        self
    }

    /// Goes after `seconds`.
    pub fn lifetime(mut self, seconds: f64) -> Self {
        self.lifetime = Some(seconds);
        self
    }
}

/// A toast in the queue.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Shown {
    pub id: u64,
    #[serde(flatten)]
    pub toast: Toast,
    /// When it goes, in the interface's seconds (set when first drawn).
    pub until: Option<f64>,
    /// Why it went: `expired`, `closed`, `action`, `dismissed`.
    pub ended: Option<&'static str>,
}

#[derive(Debug, Default)]
struct State {
    next: u64,
    queue: VecDeque<Shown>,
    history: VecDeque<Shown>,
    /// Toasts to announce and publish on the next frame.
    fresh: Vec<Shown>,
}

/// The queue of toasts.
#[derive(Clone, Debug, Default)]
pub struct Toasts(Arc<Mutex<State>>);

impl Toasts {
    /// Shows a toast (it waits while [`SHOWN`] are on screen); returns
    /// its id.
    pub fn notify(&self, toast: Toast) -> u64 {
        let mut s = lock(&self.0);
        s.next += 1;
        let shown = Shown {
            id: s.next,
            toast,
            until: None,
            ended: None,
        };
        s.fresh.push(shown.clone());
        s.queue.push_back(shown);
        s.next
    }

    /// The toasts on screen and waiting, oldest first.
    pub fn shown(&self) -> Vec<Shown> {
        lock(&self.0).queue.iter().cloned().collect()
    }

    /// The toasts gone, oldest first, with why.
    pub fn history(&self) -> Vec<Shown> {
        lock(&self.0).history.iter().cloned().collect()
    }

    /// Closes a toast; whether it was there.
    pub fn dismiss(&self, id: u64) -> bool {
        self.end(id, "dismissed").is_some()
    }

    fn end(&self, id: u64, why: &'static str) -> Option<Shown> {
        let mut s = lock(&self.0);
        let i = s.queue.iter().position(|t| t.id == id)?;
        let mut t = s.queue.remove(i)?;
        t.ended = Some(why);
        if s.history.len() == HISTORY {
            s.history.pop_front();
        }
        s.history.push_back(t.clone());
        Some(t)
    }
}

/// The plugin.
#[derive(Debug, Default)]
pub struct ToastsPlugin;

impl Plugin for ToastsPlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("egui_pop.toasts")
    }

    fn title(&self) -> String {
        "Toasts".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let toasts = Toasts::default();
        ctx.service(&TOASTS, toasts.clone())?;
        let (a, b) = (toasts.clone(), toasts);
        ctx.collector(
            FnCollector::new(
                "toasts",
                "The toasts on screen and waiting, and the last ones gone with why; `dismiss` closes one.",
            )
            .snapshot(move |_| json!({ "shown": a.shown(), "history": a.history() }))
            .action("dismiss", "Closes a toast (`id`).", true, move |_, args| {
                let id = args.get("id").and_then(Json::as_u64).ok_or("`id` is missing")?;
                if b.dismiss(id) {
                    Ok(json!({ "dismissed": id }))
                } else {
                    Err(format!("no toast {id}"))
                }
            })
            .args(json!({ "id": 1 })),
        )?;
        ctx.block(
            zones::OVERLAYS,
            Block::add(BlockId::from_static("egui_pop.toasts"), 20, show),
        );
        Ok(())
    }
}

/// The area the toasts are drawn in.
pub(crate) fn area_id() -> Id {
    Id::new("egui_pop.toasts")
}

/// Draws the toasts in the bottom-right corner, newest at the bottom.
fn show(ui: &mut Ui, app: &AppHandle) {
    let Some(toasts) = app.service(&TOASTS) else {
        return;
    };
    let ctx = ui.ctx().clone();
    let now = ctx.input(|i| i.time);
    let fresh = std::mem::take(&mut lock(&toasts.0).fresh);
    for t in fresh {
        let politeness = if t.toast.level == Level::Error {
            Politeness::Assertive
        } else {
            Politeness::Polite
        };
        a11y::announce(&ctx, t.toast.text.clone(), politeness);
        app.publish(
            "app",
            None,
            "toast.shown",
            json!({ "id": t.id, "level": t.toast.level, "text": t.toast.text }),
        );
    }
    // Start the clocks of the toasts on screen; end the expired ones.
    let on_screen: Vec<Shown> = {
        let mut s = lock(&toasts.0);
        for t in s.queue.iter_mut().take(SHOWN) {
            if t.until.is_none() {
                t.until = t.toast.lifetime.map(|l| now + l);
            }
        }
        s.queue.iter().take(SHOWN).cloned().collect()
    };
    let mut expired = Vec::new();
    let mut next_deadline: Option<f64> = None;
    for t in &on_screen {
        if let Some(u) = t.until {
            if u <= now {
                expired.push(t.id);
            } else {
                next_deadline = Some(next_deadline.map_or(u, |d: f64| d.min(u)));
            }
        }
    }
    for id in expired {
        toasts.end(id, "expired");
    }
    if let Some(d) = next_deadline {
        ctx.request_repaint_after(std::time::Duration::from_secs_f64((d - now).max(0.0)));
    }
    let on_screen: Vec<Shown> = toasts.shown().into_iter().take(SHOWN).collect();
    if on_screen.is_empty() {
        return;
    }
    let t = Tokens::of(&ctx);
    let margin = t.space.l;
    let mut ended: Vec<(u64, &'static str)> = Vec::new();
    let mut hold: Vec<u64> = Vec::new();
    egui::Area::new(area_id())
        .order(Order::Foreground)
        .anchor(Align2::RIGHT_BOTTOM, egui::vec2(-margin, -margin))
        .interactable(true)
        .show(&ctx, |ui| {
            ui.set_max_width(t.toast.width);
            for shown in &on_screen {
                let frame = egui::Frame::new()
                    .fill(t.toast.background)
                    .stroke(egui::Stroke::new(1.0, t.toast.border))
                    .corner_radius(t.radius.widget)
                    .inner_margin(t.toast.padding)
                    .show(ui, |ui| {
                        ui.set_width(t.toast.width - 2.0 * t.toast.padding);
                        ui.horizontal(|ui| {
                            let (icon, tone) = shown.toast.level.icon();
                            let level = strings::tr(
                                ui.ctx(),
                                match shown.toast.level {
                                    Level::Info => &txt::TOAST_INFO,
                                    Level::Success => &txt::TOAST_SUCCESS,
                                    Level::Warning => &txt::TOAST_WARNING,
                                    Level::Error => &txt::TOAST_ERROR,
                                },
                            );
                            ui.add(icon.tone(tone).label(&level));
                            // The text wraps in what the close button and
                            // the action leave.
                            let spacing = ui.spacing().item_spacing.x;
                            let mut reserved = t.size.icon_inline + spacing;
                            if let Some(a) = &shown.toast.action {
                                let galley = egui::WidgetText::from(a.label.as_str()).into_galley(
                                    ui,
                                    Some(egui::TextWrapMode::Extend),
                                    f32::INFINITY,
                                    egui::TextStyle::Button,
                                );
                                reserved +=
                                    galley.size().x + 2.0 * ui.spacing().button_padding.x + spacing;
                            }
                            let width = (ui.available_width() - reserved).max(t.size.icon_inline);
                            ui.allocate_ui_with_layout(
                                egui::vec2(width, 0.0),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                    ui.set_max_width(width);
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(&shown.toast.text)
                                                .color(t.toast.text),
                                        )
                                        .wrap(),
                                    );
                                },
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let close = ui.add(
                                        Icon::Close
                                            .size(Size::Inline)
                                            .label(&strings::tr_args(
                                                ui.ctx(),
                                                &txt::TOAST_CLOSE,
                                                &[("text", &shown.toast.text)],
                                            ))
                                            .sense_click(),
                                    );
                                    if close.clicked() {
                                        ended.push((shown.id, "closed"));
                                    }
                                    if let Some(a) = &shown.toast.action
                                        && ui.small_button(&a.label).clicked()
                                    {
                                        app.run_as(a.command.clone(), a.args.clone(), Origin::User);
                                        ended.push((shown.id, "action"));
                                    }
                                },
                            );
                        });
                    });
                if frame.response.contains_pointer() {
                    hold.push(shown.id);
                }
                ui.add_space(t.space.s);
            }
        });
    // The pointer over a toast holds it.
    {
        let mut s = lock(&toasts.0);
        for t in s.queue.iter_mut().filter(|t| hold.contains(&t.id)) {
            if let (Some(until), Some(life)) = (t.until.as_mut(), t.toast.lifetime) {
                *until = until.max(now + life.min(2.0));
            }
        }
    }
    for (id, why) in ended {
        toasts.end(id, why);
    }
}
