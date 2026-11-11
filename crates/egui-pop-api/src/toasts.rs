//! Toasts, as a plugin ([`ToastsPlugin`], in `DefaultPlugins`): short
//! notices in a corner that never take the focus, each with a level, an
//! optional action (a command, run through the host) and a close button.
//! Information and success go after a few seconds, warnings after
//! longer, errors stay until closed; a thin bar empties as the time
//! runs, and stops while the pointer is over the toast. Screen readers
//! hear them through the live region (errors at once).
//!
//! They show in a corner of the window, or the middle of its top or
//! bottom edge (the `appearance.toast_corner` setting, [`Corner`];
//! `AppBuilder::toast_corner` gives its default), the newest the nearest
//! to the edge. They slide in from the corner's side and out again in
//! `motion.toast.slide` (fade in the middle of an edge); under reduced
//! motion they appear and go at once, and the bar steps once a second.
//! They cast the shadow of what lies over the interface
//! (`toast.shadow-*`, aliases of `shadow.popup.*`).
//!
//! They keep off what an interface asks them to ([`keep_clear`]): the
//! Settings app's wizard footer, whose Finish sits in the default corner,
//! is never under a toast that would take its click.
//!
//! Each application window has its own toasts (ADR 0025): a toast
//! belongs to the window it was raised from (the application window being
//! drawn, or the one whose command raised it, else the active one;
//! [`Toasts::notify_in`] names one), and each window stacks its own, up
//! to [`SHOWN`] at a time. In the main window's group they show in the
//! window of it the user works in ([`AppHandle::active_window`]): a
//! detached panel's window while it has the focus, the main window again
//! when that one closes. A closed application window's toasts go to the
//! main window.
//!
//! The queue is data: [`Toasts::shown`] and [`Toasts::history`], the
//! debug bridge's `toasts` collector (with `dismiss`), and a
//! `toast.shown` event in the journal.
//!
//! Owned: egui-notify and egui-toast (both MIT) keep a queue of strings,
//! with no command actions and no state another plugin or an agent can
//! read; their stacking ideas are reused. From egui-notify (MIT,
//! <https://github.com/ItsEthra/egui-notify>) come the slide with its
//! appear, idle and disappear states and its eased value (`toast.rs`,
//! `lib.rs`), the time-left bar that stops under the pointer (`lib.rs`),
//! the choice of corner (`anchor.rs`), the newest nearest the edge (its
//! `reverse`, `lib.rs`) and a shadow under each toast (`lib.rs`); the
//! code is egui-pop's own.
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

use egui::{Align2, Id, Order, Rect, Ui, UiBuilder, Vec2, vec2};
use egui_pop_core::a11y::{self, Politeness};
use egui_pop_core::debug::FnCollector;
use egui_pop_core::icon::{Icon, Size, Tone};
use egui_pop_core::kit::{self, Kit};
use egui_pop_core::lock;
use egui_pop_core::motion;
use egui_pop_core::progress::ProgressTokens;
use egui_pop_core::strings;
use egui_pop_core::theme::Tokens;
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::{BlockId, CommandId, PluginId, SettingId};
use egui_pop_model::values::Descriptor;
use serde::Serialize;
use serde_json::{Value as Json, json};

use crate::app::AppHandle;
use crate::ctx::CtxError;
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

/// The setting: where the toasts show ([`Corner`]'s choices).
pub const SETTING_CORNER: SettingId = SettingId::from_static("appearance.toast_corner");

/// Where the toasts show: a corner of the window, or the middle of its
/// top or bottom edge. The newest is always the nearest to the edge:
/// at the top under a top corner, at the bottom under a bottom one.
// The corners as a choice, after egui-notify's `Anchor` (MIT, anchor.rs),
// with the middle of the top and bottom edges added.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Corner {
    TopLeft,
    TopCenter,
    TopRight,
    BottomLeft,
    BottomCenter,
    /// The default.
    #[default]
    BottomRight,
}

impl Corner {
    /// Every corner, in the setting's order.
    pub const ALL: [Corner; 6] = [
        Corner::TopLeft,
        Corner::TopCenter,
        Corner::TopRight,
        Corner::BottomLeft,
        Corner::BottomCenter,
        Corner::BottomRight,
    ];

    /// The setting's choices, in order.
    pub const CHOICES: [&'static str; 6] = [
        "top-left",
        "top-center",
        "top-right",
        "bottom-left",
        "bottom-center",
        "bottom-right",
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Corner::TopLeft => "top-left",
            Corner::TopCenter => "top-center",
            Corner::TopRight => "top-right",
            Corner::BottomLeft => "bottom-left",
            Corner::BottomCenter => "bottom-center",
            Corner::BottomRight => "bottom-right",
        }
    }

    pub fn parse(s: &str) -> Option<Corner> {
        Corner::ALL.into_iter().find(|c| c.as_str() == s)
    }

    /// Whether it is on the top edge (the newest toast on top).
    pub fn top(self) -> bool {
        matches!(self, Corner::TopLeft | Corner::TopCenter | Corner::TopRight)
    }

    /// The side a toast slides in from: -1 the left, 1 the right, 0 the
    /// edge itself (the middle of an edge: it fades as it slides).
    // egui-notify's `Anchor::anim_side` (MIT, anchor.rs).
    fn side(self) -> f32 {
        match self {
            Corner::TopLeft | Corner::BottomLeft => -1.0,
            Corner::TopRight | Corner::BottomRight => 1.0,
            Corner::TopCenter | Corner::BottomCenter => 0.0,
        }
    }

    fn align(self) -> Align2 {
        match self {
            Corner::TopLeft => Align2::LEFT_TOP,
            Corner::TopCenter => Align2::CENTER_TOP,
            Corner::TopRight => Align2::RIGHT_TOP,
            Corner::BottomLeft => Align2::LEFT_BOTTOM,
            Corner::BottomCenter => Align2::CENTER_BOTTOM,
            Corner::BottomRight => Align2::RIGHT_BOTTOM,
        }
    }

    /// The area's offset from the window's edges, `margin` away.
    fn offset(self, margin: f32) -> Vec2 {
        let x = self.side() * -margin;
        let y = if self.top() { margin } else { -margin };
        vec2(x, y)
    }
}

/// Where the toasts show, as the setting says now.
pub fn corner(app: &AppHandle) -> Corner {
    lock(&app.store())
        .get(&format!("settings/{SETTING_CORNER}"))
        .and_then(|e| e.value.as_choice().and_then(Corner::parse))
        .unwrap_or_default()
}

/// Declares the setting, under Appearance, `default` its default.
pub(crate) fn register(ctx: &mut Ctx<'_>, default: Corner) -> Result<(), CtxError> {
    ctx.setting(
        SETTING_CORNER,
        Descriptor::choice("Toast corner", &Corner::CHOICES, default.as_str()).description(
            "Where toasts show: a corner of the window, or the middle of its top or bottom edge. The newest is the nearest to the edge.",
        ),
    )?;
    Ok(())
}

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
    /// A job whose progress it carries (`jobs`): its bar, Cancel while
    /// it goes on, its first action once ended. It stays while the job
    /// goes on; closing it does not cancel the job.
    pub job: Option<String>,
}

impl Toast {
    pub fn new(level: Level, text: &str) -> Self {
        Self {
            level,
            text: text.into(),
            action: None,
            lifetime: level.lifetime(),
            job: None,
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

    /// Carries a job's progress (see [`Toast::job`]'s field).
    pub fn job(mut self, id: &str) -> Self {
        self.job = Some(id.into());
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

/// Where a toast is in its slide.
// egui-notify's `ToastState` (MIT, toast.rs): appear, idle, disappear.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    /// Sliding in (or waiting for its turn, not drawn yet).
    #[default]
    Appear,
    /// In place.
    Idle,
    /// Sliding out: it went already ([`Toasts::leaving`]).
    Disappear,
}

/// A toast in the queue.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Shown {
    pub id: u64,
    #[serde(flatten)]
    pub toast: Toast,
    /// When it goes, by the motion clock's seconds (set when first
    /// drawn; moved on while the pointer holds it).
    pub until: Option<f64>,
    /// Why it went: `expired`, `closed`, `action`, `dismissed`.
    pub ended: Option<&'static str>,
    /// When it was first drawn, by the motion clock.
    pub since: Option<f64>,
    /// Where it is in its slide.
    pub phase: Phase,
    /// Its time left, 0 to 1, as its bar shows it; none without a bar
    /// (no lifetime, or a job going on).
    pub left: Option<f32>,
    /// Since when the pointer holds it: its time stops meanwhile.
    pub held: Option<f64>,
    /// The application window it shows in (`main` or a window's id),
    /// once it was first drawn.
    pub window: Option<String>,
    /// Its window group (`AppHandle::window_group`).
    #[serde(skip)]
    group: Option<egui::ViewportId>,
}

/// A toast sliding out, and since when (stamped when next drawn).
#[derive(Debug)]
struct Leaving {
    shown: Shown,
    at: Option<f64>,
}

#[derive(Debug, Default)]
struct State {
    next: u64,
    queue: VecDeque<Shown>,
    history: VecDeque<Shown>,
    /// Toasts to announce and publish on the next frame.
    fresh: Vec<Shown>,
    /// Toasts gone, still sliding out.
    leaving: Vec<Leaving>,
}

/// The queue of toasts.
#[derive(Clone, Debug, Default)]
pub struct Toasts(Arc<Mutex<State>>);

impl Toasts {
    /// Shows a toast (it waits while [`SHOWN`] are on screen); returns
    /// its id.
    pub fn notify(&self, toast: Toast) -> u64 {
        self.notify_to(crate::windows::current_group(), toast)
    }

    /// Shows a toast in a window group (`AppHandle::window_group`: an
    /// application window's viewport, or `ViewportId::ROOT` for the main
    /// window); returns its id.
    pub fn notify_in(&self, group: egui::ViewportId, toast: Toast) -> u64 {
        self.notify_to(Some(group), toast)
    }

    fn notify_to(&self, group: Option<egui::ViewportId>, toast: Toast) -> u64 {
        let mut s = lock(&self.0);
        s.next += 1;
        let shown = Shown {
            id: s.next,
            toast,
            until: None,
            ended: None,
            since: None,
            phase: Phase::Appear,
            left: None,
            held: None,
            window: None,
            group,
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

    /// The toasts gone that still slide out, oldest first (drawn
    /// without their buttons, out of the accessibility tree).
    pub fn leaving(&self) -> Vec<Shown> {
        lock(&self.0)
            .leaving
            .iter()
            .map(|l| l.shown.clone())
            .collect()
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
        t.held = None;
        if t.since.is_some() {
            t.phase = Phase::Disappear;
            s.leaving.push(Leaving {
                shown: t.clone(),
                at: None,
            });
        }
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
                "The toasts on screen and waiting, those sliding out, and the last ones gone with why; `dismiss` closes one.",
            )
            .snapshot(move |_| {
                json!({ "shown": a.shown(), "leaving": a.leaving(), "history": a.history() })
            })
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
        // Drawn in every window, shown in the active one.
        ctx.block(
            zones::WINDOW_OVERLAYS,
            Block::add(BlockId::from_static("egui_pop.toasts"), 20, show),
        );
        Ok(())
    }
}

/// Rects the toasts keep off: the window's, the pass each was asked in,
/// the rect.
type Clear = Vec<(egui::ViewportId, u64, Rect)>;

fn clear_id() -> Id {
    area_id().with("keep_clear")
}

/// Keeps the toasts off `rect` (in the window's points) for this frame:
/// a row of buttons a toast would cover and take the clicks of, as a
/// wizard's footer. Toasts whose column meets it move past it: above it
/// under a bottom corner, below it under a top one. Called each frame
/// the rect is shown; it lapses the frame after it is not.
pub fn keep_clear(ctx: &egui::Context, rect: Rect) {
    let viewport = ctx.viewport_id();
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| {
        let kept = d.get_temp_mut_or_default::<Clear>(clear_id());
        kept.retain(|(v, p, _)| *v != viewport || *p + 1 >= pass);
        kept.push((viewport, pass, rect));
    });
}

/// How far the toasts move from the corner's edge to stay off the rects
/// asked this frame or the last one ([`keep_clear`]): those their column
/// (`width` wide, `margin` from the edge) meets.
fn clearance(ctx: &egui::Context, corner: Corner, width: f32, margin: f32) -> f32 {
    let viewport = ctx.viewport_id();
    let pass = ctx.cumulative_pass_nr();
    let screen = ctx.content_rect();
    let (left, right) = match corner.side() {
        s if s < 0.0 => (screen.left() + margin, screen.left() + margin + width),
        s if s > 0.0 => (screen.right() - margin - width, screen.right() - margin),
        _ => (
            screen.center().x - width / 2.0,
            screen.center().x + width / 2.0,
        ),
    };
    ctx.data_mut(|d| {
        let kept = d.get_temp_mut_or_default::<Clear>(clear_id());
        kept.retain(|(v, p, _)| *v != viewport || *p + 1 >= pass);
        kept.iter()
            .filter(|(v, _, r)| *v == viewport && r.left() < right && r.right() > left)
            .map(|(_, _, r)| {
                if corner.top() {
                    r.bottom() - screen.top()
                } else {
                    screen.bottom() - r.top()
                }
            })
            .fold(0.0_f32, f32::max)
    })
}

/// The area the toasts are drawn in.
pub(crate) fn area_id() -> Id {
    Id::unique("egui_pop.toasts")
}

/// Where a toast's size is kept, for its slide and its picture going.
fn size_id(id: u64) -> Id {
    area_id().with(("size", id))
}

/// How far in its slide a toast is, eased: 0 out of sight, 1 in place.
/// `p` is the time gone in the slide, 0 to 1, coming in; going out, the
/// same curve is run backwards.
// egui-notify's eased value (MIT, lib.rs: `ease_in_cubic`, the cubic
// ease-out), here egui's own `emath::easing::cubic_out`.
fn eased(p: f64) -> f32 {
    egui::emath::easing::cubic_out(p.clamp(0.0, 1.0) as f32)
}

/// A toast to draw: on screen, or sliding out.
enum Item {
    Live(Shown),
    Leaving(Shown, f64),
}

impl Item {
    fn shown(&self) -> &Shown {
        match self {
            Item::Live(s) | Item::Leaving(s, _) => s,
        }
    }
}

/// Draws the toasts in the chosen corner of the active window, the
/// newest the nearest to the edge.
fn show(ui: &mut Ui, app: &AppHandle) {
    let Some(toasts) = app.service(&TOASTS) else {
        return;
    };
    let ctx = ui.ctx().clone();
    let here = ctx.viewport_id();
    let group = app.window_group(here);
    // Each toast's group: the one it was raised in while that window is
    // open (the main window's otherwise); the active one's when unknown.
    {
        let active = app.window_group(app.active_window());
        let mut s = lock(&toasts.0);
        let State {
            queue,
            fresh,
            leaving,
            ..
        } = &mut *s;
        for t in queue
            .iter_mut()
            .chain(fresh.iter_mut())
            .chain(leaving.iter_mut().map(|l| &mut l.shown))
        {
            let g = app.window_group(t.group.unwrap_or(active));
            if t.group != Some(g) || t.window.is_none() {
                t.group = Some(g);
                t.window = Some(app.window_of_group(g).to_string());
            }
        }
    }
    if app.shown_in(group) != here {
        return;
    }
    let mine = |t: &Shown| t.group == Some(group);
    let now = motion::now(&ctx);
    let fresh: Vec<Shown> = {
        let mut s = lock(&toasts.0);
        let (here_now, later) = std::mem::take(&mut s.fresh).into_iter().partition(mine);
        s.fresh = later;
        here_now
    };
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
    let t = Tokens::of(&ctx);
    // No slide under reduced motion: they appear and go at once.
    let slide = if motion::reduced(&ctx) {
        0.0
    } else {
        f64::from(t.motion.toast_slide)
    };
    let corner = corner(app);
    // Start the clocks of the toasts on screen (a job's once the job
    // ended); end the expired ones. A toast the pointer holds does not
    // expire.
    let jobs = app.service(&crate::jobs::JOBS);
    let job_goes_on = |toast: &Toast| {
        toast
            .job
            .as_ref()
            .and_then(|id| jobs.as_ref()?.job(id))
            .is_some_and(|j| j.active())
    };
    let on_screen: Vec<Shown> = {
        let mut s = lock(&toasts.0);
        for t in s.queue.iter_mut().filter(|t| mine(t)).take(SHOWN) {
            if job_goes_on(&t.toast) {
                t.until = None;
            } else if t.until.is_none() {
                t.until = t.toast.lifetime.map(|l| now + l);
            }
            t.since.get_or_insert(now);
        }
        s.queue
            .iter()
            .filter(|t| mine(t))
            .take(SHOWN)
            .cloned()
            .collect()
    };
    let mut expired = Vec::new();
    let mut next_deadline: Option<f64> = None;
    for t in on_screen.iter().filter(|t| t.held.is_none()) {
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
        motion::wake_at(&ctx, d);
    }
    // Those sliding out, stamped when first drawn so; the size kept for
    // them goes with them.
    let mut gone = Vec::new();
    let leaving: Vec<(Shown, f64)> = {
        let mut s = lock(&toasts.0);
        for l in s.leaving.iter_mut().filter(|l| mine(&l.shown)) {
            l.at.get_or_insert(now);
        }
        s.leaving.retain(|l| {
            if !mine(&l.shown) {
                return true;
            }
            let stays = slide > 0.0 && l.at.is_some_and(|at| now - at < slide);
            if !stays {
                gone.push(l.shown.id);
            }
            stays
        });
        s.leaving
            .iter()
            .filter(|l| mine(&l.shown))
            .filter_map(|l| Some((l.shown.clone(), l.at?)))
            .collect()
    };
    if !gone.is_empty() {
        ctx.data_mut(|d| {
            for id in gone {
                d.remove::<Vec2>(size_id(id));
            }
        });
    }
    let mut items: Vec<Item> = toasts
        .shown()
        .into_iter()
        .filter(|t| mine(t))
        .take(SHOWN)
        .map(Item::Live)
        .chain(leaving.into_iter().map(|(s, at)| Item::Leaving(s, at)))
        .collect();
    if items.is_empty() {
        return;
    }
    // Oldest first from the top: the newest at the bottom under a bottom
    // corner, on top under a top one, the nearest to the edge either way.
    // (egui-notify's `reverse`, MIT, lib.rs, follows the corner here.)
    items.sort_by_key(|i| i.shown().id);
    if corner.top() {
        items.reverse();
    }
    let margin = t.space.l;
    let mut ended: Vec<(u64, &'static str)> = Vec::new();
    let mut hold: Vec<u64> = Vec::new();
    let mut drawn: Vec<(u64, Phase, Option<f32>)> = Vec::new();
    let mut moving = false;
    let mut running = false;
    // Off the rects asked clear: past the farthest one, then the margin.
    let clear = clearance(&ctx, corner, Kit::of(&ctx).toast.data.width, margin);
    let offset = corner.offset(margin) + vec2(0.0, if corner.top() { clear } else { -clear });
    // Over the lying-over panels, under menus and dialogs (ADR 0026).
    egui_pop_core::layers::rank(
        &ctx,
        egui::LayerId::new(Order::Foreground, area_id()),
        egui_pop_core::layers::Rank::Overlay,
    );
    egui::Area::new(area_id())
        .order(Order::Foreground)
        .anchor(corner.align(), offset)
        .interactable(true)
        .show(&ctx, |ui| {
            let width = Kit::of(ui.ctx()).toast.data.width;
            ui.set_max_width(width);
            for item in &items {
                let shown = item.shown();
                // How far in its slide (eased), and its phase.
                let (e, phase) = match item {
                    Item::Live(s) => {
                        let p = match s.since {
                            Some(since) if slide > 0.0 => (now - since) / slide,
                            _ => 1.0,
                        };
                        let phase = if p < 1.0 { Phase::Appear } else { Phase::Idle };
                        (eased(p), phase)
                    }
                    Item::Leaving(_, at) => (eased(1.0 - (now - at) / slide), Phase::Disappear),
                };
                moving |= phase != Phase::Idle;
                // Out of sight by its whole width and the margin, from the
                // corner's side (egui-notify's offset, MIT, lib.rs); in the
                // middle of an edge, by its height from the edge, fading.
                let size_id = size_id(shown.id);
                let away = 1.0 - e;
                let shift = if corner.side() == 0.0 {
                    let height = ctx
                        .data(|d| d.get_temp::<Vec2>(size_id))
                        .map_or(0.0, |s| s.y);
                    let y = away * (height + margin);
                    vec2(0.0, if corner.top() { -y } else { y })
                } else {
                    vec2(corner.side() * away * (width + margin), 0.0)
                };
                let at = ui.cursor().min;
                let room =
                    Rect::from_min_size(at + shift, vec2(width, ui.available_height().max(0.0)));
                // A child the stack does not grow by: the slot it keeps
                // is the one in place.
                let mut child = ui.new_child(
                    UiBuilder::new()
                        .id_salt(("toast", shown.id))
                        .max_rect(room)
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );
                if corner.side() == 0.0 {
                    child.multiply_opacity(e);
                }
                match item {
                    Item::Live(s) => {
                        let left = match (s.until, s.toast.lifetime) {
                            (Some(until), Some(life)) if life > 0.0 => Some(
                                (((until - s.held.unwrap_or(now)) / life) as f32).clamp(0.0, 1.0),
                            ),
                            _ => None,
                        };
                        let hovered = live(&mut child, app, s, left, size_id, &mut ended);
                        if hovered {
                            hold.push(s.id);
                        }
                        running |= left.is_some() && s.held.is_none();
                        drawn.push((s.id, phase, left));
                    }
                    Item::Leaving(s, _) => going(&mut child, s, size_id),
                }
                let height = child.min_rect().height();
                ui.allocate_space(vec2(width, height));
                ui.add_space(t.space.s);
            }
        });
    if moving {
        motion::animate(&ctx);
    }
    // A bar empties smoothly; under reduced motion it steps once a second.
    if running {
        if motion::reduced(&ctx) {
            motion::wake_at(&ctx, now + 1.0);
        } else {
            motion::animate(&ctx);
        }
    }
    // The pointer over a toast holds it: its time stops, and goes on
    // where it stopped once the pointer leaves (egui-notify's, MIT,
    // lib.rs).
    {
        let mut s = lock(&toasts.0);
        for t in s.queue.iter_mut().filter(|t| mine(t)) {
            if let Some((_, phase, left)) = drawn.iter().find(|d| d.0 == t.id) {
                t.phase = *phase;
                t.left = *left;
            }
            match (hold.contains(&t.id), t.held) {
                (true, None) => t.held = Some(now),
                (false, Some(since)) => {
                    if let Some(until) = t.until.as_mut() {
                        *until += now - since;
                    }
                    t.held = None;
                }
                _ => {}
            }
        }
    }
    for (id, why) in ended {
        toasts.end(id, why);
    }
}

/// Draws a toast on screen; whether the pointer is over it.
fn live(
    ui: &mut Ui,
    app: &AppHandle,
    shown: &Shown,
    left: Option<f32>,
    size_id: Id,
    ended: &mut Vec<(u64, &'static str)>,
) -> bool {
    let t = Tokens::of(ui.ctx());
    let style = Kit::of(ui.ctx());
    let toast = &style.toast;
    let text = toast.style(kit::State::REST).text;
    let (icon, tone) = shown.toast.level.icon();
    let frame = toast.show(ui, kit::State::REST, toast.data.shadow, |ui| {
        ui.set_width(toast.data.width - 2.0 * toast.data.padding_x);
        ui.horizontal(|ui| {
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
            // The text wraps in what the close button and the action
            // leave.
            let spacing = ui.spacing().item_spacing.x;
            let mut reserved = t.size.icon_inline + spacing;
            if let Some(a) = &shown.toast.action {
                let galley = egui::WidgetText::from(a.label.as_str()).into_galley(
                    ui,
                    Some(egui::TextWrapMode::Extend),
                    f32::INFINITY,
                    egui::TextStyle::Button,
                );
                reserved += galley.size().x + 2.0 * ui.spacing().button_padding.x + spacing;
            }
            let width = (ui.available_width() - reserved).max(t.size.icon_inline);
            ui.allocate_ui_with_layout(
                egui::vec2(width, 0.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_max_width(width);
                    crate::links::Inline::new(&shown.toast.text)
                        .color(text)
                        .show(ui, app);
                },
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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
            });
        });
        if let Some(id) = &shown.toast.job {
            crate::jobs_centre::toast_part(ui, app, id);
        }
        // The time left, emptying towards the start (egui-notify's
        // progress bar, MIT, lib.rs), in the level's colour, a thin
        // line's thickness.
        if let Some(left) = left {
            ui.add_space(t.space.xs);
            let line = ProgressTokens::of(ui.ctx()).line;
            let (_, rect) = ui.allocate_space(vec2(ui.available_width(), line));
            let fill = Rect::from_min_size(rect.min, vec2(rect.width() * left, line));
            ui.painter().rect_filled(fill, line / 2.0, tone.color(&t));
        }
        let size = ui.min_rect().size();
        ui.ctx().data_mut(|d| d.insert_temp(size_id, size));
    });
    frame.response.contains_pointer()
}

/// Draws a toast sliding out: its frame, icon and text, painted only
/// (it went: nothing to press, nothing for a screen reader).
fn going(ui: &mut Ui, shown: &Shown, size_id: Id) {
    let t = Tokens::of(ui.ctx());
    let style = Kit::of(ui.ctx());
    let toast = &style.toast;
    let text = toast.style(kit::State::REST).text;
    let (icon, tone) = shown.toast.level.icon();
    let inner = ui
        .ctx()
        .data(|d| d.get_temp::<Vec2>(size_id))
        .unwrap_or(vec2(
            toast.data.width - 2.0 * toast.data.padding_x,
            t.size.icon_inline,
        ));
    toast.show(ui, kit::State::REST, toast.data.shadow, |ui| {
        let (_, rect) = ui.allocate_space(inner);
        let side = t.size.icon_inline;
        icon.paint(
            ui.painter(),
            Rect::from_min_size(rect.min, Vec2::splat(side)),
            tone.color(&t),
        );
        let gap = ui.spacing().item_spacing.x;
        let wrap = (rect.width() - 2.0 * (side + gap)).max(side);
        let galley = egui::WidgetText::from(shown.toast.text.as_str()).into_galley(
            ui,
            Some(egui::TextWrapMode::Wrap),
            wrap,
            egui::TextStyle::Body,
        );
        ui.painter()
            .galley(rect.min + vec2(side + gap, 0.0), galley, text);
    });
}
