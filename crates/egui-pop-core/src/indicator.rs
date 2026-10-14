//! Micro-indicators: the small marks that say a state in very little
//! space, beside or on another widget. Five kinds need nothing but the
//! theme, plus icons that follow a state:
//!
//! - a **dot** or an **LED** ([`Led`]: off, on, busy, warning, error),
//!   each state with a shape of its own (hollow, filled, pulsing, a
//!   triangle, a square);
//! - a **badge**: a count capped at `99+` (zero hidden unless asked),
//!   or a short text;
//! - an **activity pip** ([`Pip`]): a spinner, a ring, done, failed;
//! - a **flag** ([`Flag`]: modified, pinned, required, invalid, or a
//!   [`FlagShape`] of the application's): a corner triangle, a stripe,
//!   a diamond, a triangle, a square, an asterisk;
//! - a **swatch** ([`Swatch`]): a colour over egui's alpha checkerboard,
//!   no colour (a slash), or mixed (a dash);
//! - a **dynamic icon**: a built-in [`Icon`], or one of two by a state.
//!
//! Colours come from the shared [`Tone`], sizes from the `indicator.*`
//! tokens ([`IndicatorTokens`]). A state is never said by colour alone:
//! each has a shape, a glyph or a text, and a name.
//!
//! **Passive by default.** A passive mark takes no click and no focus.
//! On a widget ([`Indicator::on`], [`IndicatorExt::with_indicator`]) its
//! meaning joins that widget's accessible name ("Notifications, 3");
//! inline ([`Ui::add`]) it is read as an image with its meaning for a
//! name. Its tooltip says the meaning too.
//!
//! **Interactive** ([`Indicator::interactive`]): the button rules of
//! egui-pop, a target of at least `indicator.target` (24 points) around
//! the mark, the focus ring, Space and Enter, a tooltip, an AccessKit
//! node of its own (a button, or a check box for an icon that toggles)
//! whose description is the mark's meaning, a reason when disabled, and
//! a command ([`Indicator::command`]) that the application runs.
//!
//! **Anchoring.** Inline with `ui.add`, or on any rectangle with
//! [`Indicator::paint_at`]: a corner, the start or end edge (centred
//! vertically) or the centre ([`Anchor`]). Start and end follow the
//! [`Direction`] (left to right unless set). Badges and dots overhang a
//! corner by half their height and are drawn on a layer of their own
//! just above the host's, never clipped; the others sit inside.
//!
//! **Motion.** A busy LED pulses; any mark can call attention after a
//! change ([`Indicator::attention`]) for `indicator.pulse.count` pulses.
//! The pulse runs on the motion clock (a frozen clock gives the same
//! frame), and under reduced motion it is a still ring instead.
//!
//! **Agents.** Marks with an id ([`Indicator::id`]) are recorded each
//! pass: [`drawn`] lists them, and the debug bridge's `indicators`
//! collector shows them as JSON ([`Record`]), so an agent reads states
//! instead of pixels.
//!
//! ```
//! use egui_pop_core::icon::Icon;
//! use egui_pop_core::indicator::{Anchor, Indicator, IndicatorExt as _, Led, Pip, Tone};
//!
//! fn status(ui: &mut egui::Ui, unread: u64, rendering: bool) {
//!     // Passive, inline: read as "On".
//!     ui.add(Indicator::led(Led::On).id("node").tooltip("Node connected"));
//!     if rendering {
//!         ui.add(Indicator::pip(Pip::Busy).name("Rendering"));
//!     }
//!     // On a button's corner: the button reads "Notifications, 3".
//!     ui.add(Icon::Notification.label("Notifications").sense_click())
//!         .with_indicator(ui, Indicator::count(unread).tone(Tone::Danger), Anchor::TopEnd);
//!     // Interactive: a button of its own.
//!     if ui
//!         .add(Indicator::dot(Tone::Accent).name("Changed").interactive("Reset exposure"))
//!         .clicked()
//!     { /* reset */ }
//! }
//! ```

use std::sync::{Arc, Mutex};

use egui::{
    Align2, Color32, Context, CornerRadius, FontId, Id, LayerId, Painter, Pos2, Rect, Response,
    Sense, Shape, Stroke, StrokeKind, Ui, Vec2, Widget, WidgetInfo, WidgetType, pos2, vec2,
};
use egui_pop_model::ids::CommandId;
use serde::Serialize;

use crate::icon::Icon;
use crate::progress::{self, ProgressTokens};
use crate::state::{lock, pass, shared};
use crate::strings::{self, StrKey};
use crate::theme::Tokens;
pub use crate::tone::Tone;
use crate::{motion, scale};

/// A badge's cap by default: `99+` past it.
pub const CAP: u64 = 99;

/// The indicator tokens of the installed theme.
#[derive(Clone, Debug, PartialEq)]
pub struct IndicatorTokens {
    /// A dot's or an LED's diameter (`indicator.dot.size`).
    pub dot: f32,
    /// The ring of an LED that is off, or of a still pulse
    /// (`indicator.led.ring`).
    pub ring: f32,
    /// An LED that is off (`indicator.led.off`).
    pub off: Color32,
    /// The recording tone (`indicator.record`).
    pub record: Color32,
    /// A badge's height (`indicator.badge.height`).
    pub badge_height: f32,
    /// Either side of a badge's text (`indicator.badge.padding`).
    pub badge_padding: f32,
    /// A badge's text size (`indicator.badge.font`).
    pub badge_font: f32,
    /// A badge's text on every tone (`indicator.badge.text`).
    pub badge_text: Color32,
    /// The ring separating a mark from what it overhangs
    /// (`indicator.outline`).
    pub outline: Color32,
    /// A flag's size (`indicator.flag.size`).
    pub flag: f32,
    /// A swatch's side, corners and outline (`indicator.swatch.*`).
    pub swatch: f32,
    pub swatch_radius: f32,
    pub swatch_border: Color32,
    /// The smallest target of an interactive mark, never under 24
    /// points (`indicator.target`).
    pub target: f32,
    /// How far a mark on a corner sits in from it (`indicator.offset`).
    pub offset: f32,
    /// One pulse, in seconds (`indicator.pulse.period`).
    pub pulse_period: f32,
    /// Pulses of a mark calling attention (`indicator.pulse.count`).
    pub pulse_count: u32,
}

impl IndicatorTokens {
    pub fn of(ctx: &Context) -> IndicatorTokens {
        let t = Tokens::of(ctx);
        let r = t.resolved();
        let d = |path: &str, fallback: f32| r.dimension(path).unwrap_or(fallback);
        let c = |path: &str, fallback: Color32| t.color(path).unwrap_or(fallback);
        IndicatorTokens {
            dot: d("indicator.dot.size", 8.0).max(4.0),
            ring: d("indicator.led.ring", 1.5).max(1.0),
            off: c("indicator.led.off", t.border.strong),
            record: c("indicator.record", t.status.danger),
            badge_height: d("indicator.badge.height", 16.0).max(10.0),
            badge_padding: d("indicator.badge.padding", t.space.s).max(0.0),
            badge_font: d("indicator.badge.font", t.font.small).max(6.0),
            badge_text: c("indicator.badge.text", t.surface.panel),
            outline: c("indicator.outline", t.surface.panel),
            flag: d("indicator.flag.size", 8.0).max(4.0),
            swatch: d("indicator.swatch.size", t.size.icon_inline).max(6.0),
            swatch_radius: d("indicator.swatch.radius", 2.0).max(0.0),
            swatch_border: c("indicator.swatch.border", t.border.strong),
            target: d("indicator.target", 24.0).max(24.0),
            offset: d("indicator.offset", t.space.xs).max(0.0),
            pulse_period: r
                .duration_ms("indicator.pulse.period")
                .unwrap_or(1200.0)
                .max(0.0)
                / 1000.0,
            pulse_count: d("indicator.pulse.count", 3.0).max(0.0) as u32,
        }
    }
}

/// A size step (never points): the tokens' sizes scaled.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Size {
    Small,
    #[default]
    Regular,
    Large,
}

impl Size {
    pub const ALL: [Size; 3] = [Size::Small, Size::Regular, Size::Large];

    pub fn factor(self) -> f32 {
        match self {
            Size::Small => 0.75,
            Size::Regular => 1.0,
            Size::Large => 1.5,
        }
    }
}

/// An LED's state; each has its own shape and default tone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Led {
    /// A hollow ring (`indicator.led.off`).
    Off,
    /// Filled (success).
    On,
    /// Filled and pulsing (accent); a still ring under reduced motion.
    Busy,
    /// A triangle (warning).
    Warning,
    /// A square (danger).
    Error,
}

impl Led {
    pub const ALL: [Led; 5] = [Led::Off, Led::On, Led::Busy, Led::Warning, Led::Error];

    pub fn tone(self) -> Tone {
        match self {
            Led::Off => Tone::Muted,
            Led::On => Tone::Success,
            Led::Busy => Tone::Accent,
            Led::Warning => Tone::Warning,
            Led::Error => Tone::Danger,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Led::Off => "off",
            Led::On => "on",
            Led::Busy => "busy",
            Led::Warning => "warning",
            Led::Error => "error",
        }
    }

    fn key(self) -> StrKey {
        match self {
            Led::Off => strings::INDICATOR_OFF,
            Led::On => strings::INDICATOR_ON,
            Led::Busy => strings::BUSY,
            Led::Warning => strings::INDICATOR_WARNING,
            Led::Error => strings::INDICATOR_ERROR,
        }
    }
}

/// An activity pip's state.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Pip {
    /// A turning arc (still under reduced motion).
    Busy,
    /// A ring filled from the top, 0 to 1.
    Progress(f32),
    /// A tick (success).
    Done,
    /// A cross in a circle (danger).
    Failed,
}

impl Pip {
    pub fn tone(self) -> Tone {
        match self {
            Pip::Busy | Pip::Progress(_) => Tone::Accent,
            Pip::Done => Tone::Success,
            Pip::Failed => Tone::Danger,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Pip::Busy => "busy",
            Pip::Progress(_) => "progress",
            Pip::Done => "done",
            Pip::Failed => "failed",
        }
    }
}

/// A flag's shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FlagShape {
    /// A triangle filling a corner (on a corner anchor, that corner).
    Corner,
    /// A thin bar (on a start or end anchor, the host's height).
    Stripe,
    Diamond,
    Triangle,
    Square,
    Asterisk,
}

impl FlagShape {
    pub const ALL: [FlagShape; 6] = [
        FlagShape::Corner,
        FlagShape::Stripe,
        FlagShape::Diamond,
        FlagShape::Triangle,
        FlagShape::Square,
        FlagShape::Asterisk,
    ];
}

/// The flags egui-pop names, each with its shape and tone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Flag {
    /// A corner triangle (accent).
    Modified,
    /// A diamond (info).
    Pinned,
    /// An asterisk (warning).
    Required,
    /// A triangle (danger).
    Invalid,
}

impl Flag {
    pub const ALL: [Flag; 4] = [Flag::Modified, Flag::Pinned, Flag::Required, Flag::Invalid];

    pub fn shape(self) -> FlagShape {
        match self {
            Flag::Modified => FlagShape::Corner,
            Flag::Pinned => FlagShape::Diamond,
            Flag::Required => FlagShape::Asterisk,
            Flag::Invalid => FlagShape::Triangle,
        }
    }

    pub fn tone(self) -> Tone {
        match self {
            Flag::Modified => Tone::Accent,
            Flag::Pinned => Tone::Info,
            Flag::Required => Tone::Warning,
            Flag::Invalid => Tone::Danger,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Flag::Modified => "modified",
            Flag::Pinned => "pinned",
            Flag::Required => "required",
            Flag::Invalid => "invalid",
        }
    }

    fn key(self) -> StrKey {
        match self {
            Flag::Modified => strings::INDICATOR_MODIFIED,
            Flag::Pinned => strings::INDICATOR_PINNED,
            Flag::Required => strings::INDICATOR_REQUIRED,
            Flag::Invalid => strings::INDICATOR_INVALID,
        }
    }
}

/// What a swatch shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Swatch {
    /// A colour; under alpha, egui's checkerboard shows through.
    Color(Color32),
    /// No colour: a slash.
    None,
    /// Several colours at once (a selection): a dash.
    Mixed,
}

/// Where a mark goes on its host.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Anchor {
    TopStart,
    TopEnd,
    BottomStart,
    BottomEnd,
    /// The start edge, centred vertically.
    Start,
    /// The end edge, centred vertically.
    End,
    Center,
}

impl Anchor {
    pub const ALL: [Anchor; 7] = [
        Anchor::TopStart,
        Anchor::TopEnd,
        Anchor::BottomStart,
        Anchor::BottomEnd,
        Anchor::Start,
        Anchor::End,
        Anchor::Center,
    ];

    pub fn is_corner(self) -> bool {
        matches!(
            self,
            Anchor::TopStart | Anchor::TopEnd | Anchor::BottomStart | Anchor::BottomEnd
        )
    }

    /// Which sides it touches: right (else left, `None` centred), bottom
    /// (else top, `None` centred).
    fn sides(self, direction: Direction) -> (Option<bool>, Option<bool>) {
        let rtl = direction == Direction::RightToLeft;
        match self {
            Anchor::TopStart => (Some(rtl), Some(false)),
            Anchor::TopEnd => (Some(!rtl), Some(false)),
            Anchor::BottomStart => (Some(rtl), Some(true)),
            Anchor::BottomEnd => (Some(!rtl), Some(true)),
            Anchor::Start => (Some(rtl), None),
            Anchor::End => (Some(!rtl), None),
            Anchor::Center => (None, None),
        }
    }

    /// The rectangle of a mark of `size` on `host`. Inside, it sits
    /// `offset` in from the edges it touches; overhanging (corners only)
    /// it passes them by half its height, so a dot is centred on the
    /// corner and a badge on the top edge.
    pub fn place(
        self,
        host: Rect,
        size: Vec2,
        direction: Direction,
        overhang: bool,
        offset: f32,
    ) -> Rect {
        let (right, bottom) = self.sides(direction);
        let overhang = overhang && self.is_corner();
        let out = size.y / 2.0;
        let x = match right {
            Some(true) if overhang => host.right() + out - size.x,
            Some(true) => host.right() - offset - size.x,
            Some(false) if overhang => host.left() - out,
            Some(false) => host.left() + offset,
            None => host.center().x - size.x / 2.0,
        };
        let y = match bottom {
            Some(true) if overhang => host.bottom() + out - size.y,
            Some(true) => host.bottom() - offset - size.y,
            Some(false) if overhang => host.top() - out,
            Some(false) => host.top() + offset,
            None => host.center().y - size.y / 2.0,
        };
        Rect::from_min_size(pos2(x, y), size)
    }
}

/// The interface's direction: where start and end are. egui has none
/// of its own yet; an application in a right-to-left language sets it
/// with [`set_direction`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    #[default]
    LeftToRight,
    RightToLeft,
}

/// Sets where start and end are for the marks of this context.
pub fn set_direction(ctx: &Context, direction: Direction) {
    lock(&drawn_state(ctx)).direction = direction;
}

/// Where start and end are ([`set_direction`]).
pub fn direction(ctx: &Context) -> Direction {
    lock(&drawn_state(ctx)).direction
}

/// The text a badge shows for a count: `cap+` past its cap.
pub fn badge_text(count: u64, cap: u64) -> String {
    if count > cap {
        format!("{cap}+")
    } else {
        count.to_string()
    }
}

/// Adds a meaning to a widget's accessible name: "Masks" becomes
/// "Masks, 3 errors". A widget named by another node (`labelled_by`)
/// gets it as its description instead. Call it after the widget gave
/// its own name, each pass.
pub fn annotate(ctx: &Context, host: Id, meaning: &str) {
    ctx.accesskit_node_builder(host, |node| {
        let label = node.label().unwrap_or_default().to_string();
        if label.is_empty() && !node.labelled_by().is_empty() {
            let description = match node.description() {
                Some(d) if !d.is_empty() => format!("{d}, {meaning}"),
                _ => meaning.to_string(),
            };
            node.set_description(description);
        } else if label.is_empty() {
            node.set_label(meaning.to_string());
        } else {
            node.set_label(format!("{label}, {meaning}"));
        }
    });
}

#[derive(Clone, Debug, PartialEq)]
enum Kind {
    Dot(Option<Led>),
    Badge {
        count: Option<u64>,
        text: String,
        cap: u64,
        show_zero: bool,
    },
    Pip(Pip),
    Flag(FlagShape, Option<Flag>),
    Swatch(Swatch),
    Icon {
        icon: Icon,
        /// A toggle: whether it is on, and the icon when off.
        toggle: Option<(bool, Icon)>,
    },
}

/// A micro-indicator, built then added inline (`ui.add`) or painted on
/// a widget ([`Indicator::on`], [`Indicator::paint_at`]).
#[derive(Clone, Debug, PartialEq)]
#[must_use = "add it with `ui.add`, or paint it on a widget with `on` or `paint_at`"]
pub struct Indicator {
    kind: Kind,
    tone: Option<Tone>,
    key: Option<String>,
    name: Option<String>,
    tooltip: Option<String>,
    size: Size,
    action: Option<String>,
    command: Option<CommandId>,
    disabled: Option<String>,
    attention: Option<f64>,
    delayed: Option<(Id, f64, bool)>,
    overhang: Option<bool>,
}

impl Indicator {
    fn new(kind: Kind) -> Indicator {
        Indicator {
            kind,
            tone: None,
            key: None,
            name: None,
            tooltip: None,
            size: Size::Regular,
            action: None,
            command: None,
            disabled: None,
            attention: None,
            delayed: None,
            overhang: None,
        }
    }

    /// A plain dot in a tone. Give it a [`name`](Self::name): a dot
    /// alone says nothing but "something".
    pub fn dot(tone: Tone) -> Indicator {
        Indicator::new(Kind::Dot(None)).tone(tone)
    }

    /// An LED: its state's shape and tone, its state's name.
    pub fn led(state: Led) -> Indicator {
        Indicator::new(Kind::Dot(Some(state)))
    }

    /// A count badge: `99+` past its [`cap`](Self::cap), hidden at zero
    /// unless [`show_zero`](Self::show_zero). Its name is the real count.
    pub fn count(count: u64) -> Indicator {
        Indicator::new(Kind::Badge {
            count: Some(count),
            text: String::new(),
            cap: CAP,
            show_zero: false,
        })
    }

    /// A badge with a short text (`!`, `NEW`: three characters at most
    /// read well).
    pub fn text(text: impl Into<String>) -> Indicator {
        Indicator::new(Kind::Badge {
            count: None,
            text: text.into(),
            cap: CAP,
            show_zero: false,
        })
    }

    /// An activity pip.
    pub fn pip(state: Pip) -> Indicator {
        Indicator::new(Kind::Pip(state))
    }

    /// One of egui-pop's flags, with its shape, tone and name.
    pub fn flag(flag: Flag) -> Indicator {
        Indicator::new(Kind::Flag(flag.shape(), Some(flag)))
    }

    /// A flag of the application's: a shape, then a tone and a name.
    pub fn flag_shape(shape: FlagShape) -> Indicator {
        Indicator::new(Kind::Flag(shape, None))
    }

    /// A colour swatch.
    pub fn swatch(color: Color32) -> Indicator {
        Indicator::new(Kind::Swatch(Swatch::Color(color)))
    }

    /// A swatch saying "no colour", or "mixed".
    pub fn swatch_of(swatch: Swatch) -> Indicator {
        Indicator::new(Kind::Swatch(swatch))
    }

    /// A built-in icon as a mark.
    pub fn icon(icon: Icon) -> Indicator {
        Indicator::new(Kind::Icon { icon, toggle: None })
    }

    /// One of two icons by a state (eye and eye-off, lock and unlock).
    /// Made interactive, it is a check box.
    pub fn toggle(on: bool, when_on: Icon, when_off: Icon) -> Indicator {
        Indicator::new(Kind::Icon {
            icon: when_on,
            toggle: Some((on, when_off)),
        })
    }

    /// An id, for agents and tests: recorded each pass ([`drawn`]).
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.key = Some(id.into());
        self
    }

    /// What the mark means, read with its host's name (passive) or as
    /// its description (interactive). LEDs, flags, pips, swatches and
    /// counts have one by default.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// The tooltip; by default the meaning (passive) or the action
    /// (interactive).
    pub fn tooltip(mut self, text: impl Into<String>) -> Self {
        self.tooltip = Some(text.into());
        self
    }

    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = Some(tone);
        self
    }

    pub fn size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }

    /// A badge's cap: `cap+` past it.
    pub fn cap(mut self, cap: u64) -> Self {
        if let Kind::Badge { cap: c, .. } = &mut self.kind {
            *c = cap;
        }
        self
    }

    /// Shows a count of zero (hidden by default).
    pub fn show_zero(mut self, show: bool) -> Self {
        if let Kind::Badge { show_zero, .. } = &mut self.kind {
            *show_zero = show;
        }
        self
    }

    /// Makes it a button whose accessible name is `action` ("Reset
    /// exposure", "Visible" for a toggle).
    pub fn interactive(mut self, action: impl Into<String>) -> Self {
        self.action = Some(action.into());
        self
    }

    /// The command an activation runs (an interactive mark): the
    /// application runs it before the next frame, so it is a step.
    pub fn command(mut self, command: CommandId) -> Self {
        self.command = Some(command);
        self
    }

    /// Disables an interactive mark, saying why on hover.
    pub fn disabled(mut self, reason: impl Into<String>) -> Self {
        self.disabled = Some(reason.into());
        self
    }

    /// Calls attention: pulses `indicator.pulse.count` times from
    /// `since` (motion clock seconds, [`motion::now`]), then stays
    /// still. A still ring under reduced motion, for as long.
    pub fn attention(mut self, since: f64) -> Self {
        self.attention = Some(since);
        self
    }

    /// A busy pip shown only after `motion.spinner.delay` of work, then
    /// at least `motion.spinner.min` ([`motion::Busy`]); `id` keeps its
    /// state, `since` is when the work started.
    pub fn delayed(mut self, id: Id, since: f64, busy: bool) -> Self {
        self.delayed = Some((id, since, busy));
        self
    }

    /// Whether it overhangs a corner (badges and dots do by default).
    pub fn overhang(mut self, overhang: bool) -> Self {
        self.overhang = Some(overhang);
        self
    }

    pub fn is_interactive(&self) -> bool {
        self.action.is_some()
    }

    /// Its tone: the one given, else its kind's.
    pub fn tone_of(&self) -> Tone {
        self.tone.unwrap_or(match &self.kind {
            Kind::Dot(Some(led)) => led.tone(),
            Kind::Dot(None) | Kind::Badge { .. } => Tone::Accent,
            Kind::Pip(p) => p.tone(),
            Kind::Flag(_, Some(f)) => f.tone(),
            Kind::Flag(_, None) => Tone::Accent,
            Kind::Swatch(_) | Kind::Icon { .. } => Tone::Neutral,
        })
    }

    /// The text a badge shows (none for other kinds).
    pub fn label(&self) -> Option<String> {
        match &self.kind {
            Kind::Badge {
                count: Some(n),
                cap,
                ..
            } => Some(badge_text(*n, *cap)),
            Kind::Badge { text, .. } => Some(text.clone()),
            _ => None,
        }
    }

    /// Whether it shows anything: a badge at zero does not, unless asked.
    fn has_content(&self) -> bool {
        match &self.kind {
            Kind::Badge {
                count: Some(0),
                show_zero,
                ..
            } => *show_zero,
            Kind::Badge { count: Some(_), .. } => true,
            Kind::Badge { text, .. } => !text.is_empty(),
            _ => true,
        }
    }

    fn shown(&self, ctx: &Context) -> bool {
        if !self.has_content() {
            return false;
        }
        match self.delayed {
            Some((id, since, busy)) => motion::Busy::new(id, since).visible(ctx, busy),
            None => true,
        }
    }

    /// What it means, in the current language: the name given, else its
    /// kind's (an LED's state, a flag, a pip, a swatch's colour, a
    /// count).
    pub fn meaning(&self, ctx: &Context) -> Option<String> {
        if let Some(n) = &self.name {
            return Some(n.clone());
        }
        match &self.kind {
            Kind::Dot(Some(led)) => Some(strings::tr(ctx, &led.key())),
            Kind::Dot(None) | Kind::Icon { .. } => None,
            Kind::Badge { count: Some(n), .. } => Some(n.to_string()),
            Kind::Badge { text, .. } => (!text.is_empty()).then(|| text.clone()),
            Kind::Pip(Pip::Busy) => Some(strings::tr(ctx, &strings::BUSY)),
            Kind::Pip(Pip::Progress(f)) => {
                let percent = format!("{:.0}", f.clamp(0.0, 1.0) * 100.0);
                Some(strings::tr_args(
                    ctx,
                    &strings::INDICATOR_PERCENT,
                    &[("percent", &percent)],
                ))
            }
            Kind::Pip(Pip::Done) => Some(strings::tr(ctx, &strings::INDICATOR_DONE)),
            Kind::Pip(Pip::Failed) => Some(strings::tr(ctx, &strings::INDICATOR_FAILED)),
            Kind::Flag(_, Some(f)) => Some(strings::tr(ctx, &f.key())),
            Kind::Flag(_, None) => None,
            Kind::Swatch(Swatch::Color(c)) => Some(hex(*c)),
            Kind::Swatch(Swatch::None) => Some(strings::tr(ctx, &strings::INDICATOR_NO_COLOR)),
            Kind::Swatch(Swatch::Mixed) => Some(strings::tr(ctx, &strings::INDICATOR_MIXED)),
        }
    }

    /// What agents and tests see of it (its rectangle empty: it is not
    /// drawn).
    pub fn describe(&self, ctx: &Context) -> Record {
        let (kind, state, value) = match &self.kind {
            Kind::Dot(None) => ("dot", "shown".to_string(), None),
            Kind::Dot(Some(led)) => ("led", led.name().to_string(), None),
            Kind::Badge { count: Some(n), .. } => ("badge", "count".to_string(), Some(*n as f64)),
            Kind::Badge { .. } => ("badge", "text".to_string(), None),
            Kind::Pip(p) => (
                "pip",
                p.name().to_string(),
                match p {
                    Pip::Progress(f) => Some(f64::from(f.clamp(0.0, 1.0))),
                    _ => None,
                },
            ),
            Kind::Flag(_, Some(f)) => ("flag", f.name().to_string(), None),
            Kind::Flag(shape, None) => (
                "flag",
                serde_json::to_value(shape)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_string))
                    .unwrap_or_default(),
                None,
            ),
            Kind::Swatch(Swatch::Color(_)) => ("swatch", "color".to_string(), None),
            Kind::Swatch(Swatch::None) => ("swatch", "none".to_string(), None),
            Kind::Swatch(Swatch::Mixed) => ("swatch", "mixed".to_string(), None),
            Kind::Icon {
                toggle: Some((on, _)),
                ..
            } => (
                "icon",
                if *on { "on" } else { "off" }.to_string(),
                Some(if *on { 1.0 } else { 0.0 }),
            ),
            Kind::Icon { icon, .. } => ("icon", icon.name().to_string(), None),
        };
        let text = match &self.kind {
            Kind::Swatch(Swatch::Color(c)) => Some(hex(*c)),
            _ => self.label(),
        };
        Record {
            id: self.key.clone().unwrap_or_default(),
            kind,
            state,
            value,
            text,
            tone: self.tone_of(),
            name: self.meaning(ctx),
            action: self.action.clone(),
            host: None,
            interactive: self.is_interactive(),
            enabled: self.disabled.is_none(),
            shown: self.has_content(),
            pulsing: false,
            command: self.command.as_ref().map(|c| c.as_str().to_string()),
            rect: [0.0; 4],
        }
    }

    /// The mark's own size, without its target.
    fn mark_size(&self, painter: &Painter, it: &IndicatorTokens) -> Vec2 {
        let f = self.size.factor();
        let t = Tokens::of(painter.ctx());
        let square = |side: f32| Vec2::splat((side * f).round());
        match &self.kind {
            Kind::Dot(_) => square(it.dot),
            Kind::Badge { .. } => {
                let h = (it.badge_height * f).round();
                let text = self.label().unwrap_or_default();
                let g = painter.layout_no_wrap(
                    text,
                    FontId::proportional(it.badge_font * f),
                    Color32::WHITE,
                );
                vec2((g.size().x + 2.0 * it.badge_padding * f).max(h).ceil(), h)
            }
            Kind::Pip(_) => square(ProgressTokens::of(painter.ctx()).ring),
            Kind::Flag(FlagShape::Stripe, _) => {
                vec2((3.0 * f).round(), (it.flag * 2.0 * f).round())
            }
            Kind::Flag(..) => square(it.flag),
            Kind::Swatch(_) => square(it.swatch),
            Kind::Icon { .. } => square(t.size.icon_inline),
        }
    }

    fn egui_id(&self, fallback: Id) -> Id {
        match &self.key {
            Some(k) => Id::new(("egui_pop_indicator", k.as_str())),
            None => fallback,
        }
    }

    fn pulse(&self, ctx: &Context, it: &IndicatorTokens) -> Pulse {
        let busy = matches!(self.kind, Kind::Dot(Some(Led::Busy)));
        let period = f64::from(it.pulse_period);
        let now = motion::now(ctx);
        let attending = self.attention.and_then(|since| {
            let until = since + period * f64::from(it.pulse_count);
            (since..until).contains(&now).then_some((since, until))
        });
        if !busy && attending.is_none() {
            return Pulse::None;
        }
        if let Some((_, until)) = attending {
            motion::wake_at(ctx, until);
        }
        if motion::reduced(ctx) || period <= 0.0 {
            return Pulse::Still;
        }
        motion::animate(ctx);
        let phase = match (busy, attending) {
            (false, Some((since, _))) => ((now - since) / period).rem_euclid(1.0) as f32,
            _ => motion::phase(ctx, period, 0.0),
        };
        Pulse::Moving(phase)
    }

    /// Adds it inline and returns its response (its target when
    /// interactive): the same as `ui.add`.
    pub fn show(self, ui: &mut Ui) -> Response {
        self.ui(ui)
    }

    /// Paints it on a widget's corner, edge or centre, and adds its
    /// meaning to that widget's accessible name when passive. Returns
    /// the mark's response (a button's when interactive).
    pub fn on(self, ui: &mut Ui, host: &Response, anchor: Anchor) -> Response {
        let ctx = ui.ctx().clone();
        let passive = !self.is_interactive();
        let meaning = self.meaning(&ctx);
        let host_name = host_label(&ctx, host.id);
        let shown = self.shown(&ctx);
        let r = self.place(ui, host.rect, anchor, host_name);
        if passive
            && shown
            && let Some(m) = meaning
        {
            annotate(&ctx, host.id, &m);
        }
        r
    }

    /// Paints it on a rectangle (no widget's name to add to). Returns
    /// the mark's response (a button's when interactive).
    pub fn paint_at(self, ui: &mut Ui, host: Rect, anchor: Anchor) -> Response {
        self.place(ui, host, anchor, None)
    }

    fn place(self, ui: &mut Ui, host: Rect, anchor: Anchor, host_name: Option<String>) -> Response {
        let ctx = ui.ctx().clone();
        let it = IndicatorTokens::of(&ctx);
        let dir = direction(&ctx);
        let mut size = self.mark_size(ui.painter(), &it);
        let shape = match &self.kind {
            Kind::Flag(shape, _) => Some(*shape),
            _ => None,
        };
        let mut offset = it.offset;
        if shape == Some(FlagShape::Stripe) && matches!(anchor, Anchor::Start | Anchor::End) {
            size.y = host.height();
            offset = 0.0;
        }
        if shape == Some(FlagShape::Corner) && anchor.is_corner() {
            offset = 0.0;
        }
        let overhang = self
            .overhang
            .unwrap_or(matches!(self.kind, Kind::Badge { .. } | Kind::Dot(_)))
            && anchor.is_corner();
        let mark = anchor.place(host, size, dir, overhang, offset);
        let fallback = ui
            .id()
            .with(("egui_pop_indicator", ui.next_auto_id(), anchor));
        let id = self.egui_id(fallback);
        let target = if self.is_interactive() {
            Rect::from_center_size(
                mark.center(),
                vec2(mark.width().max(it.target), mark.height().max(it.target)),
            )
        } else {
            mark
        };
        let response = self.interact(ui, target, id, self.shown(ui.ctx()));
        let painter = if overhang {
            overhang_painter(ui)
        } else {
            ui.painter().clone()
        };
        let corner = anchor.sides(dir);
        let visible = ui.is_rect_visible(host);
        self.finish(
            ui,
            &painter,
            response,
            mark,
            Placement {
                on_host: true,
                corner: (corner.0.unwrap_or(false), corner.1.unwrap_or(false)),
                visible,
                host: host_name,
            },
        )
    }

    /// Whether an interactive mark takes clicks: not disabled, nor in a
    /// disabled `Ui`.
    fn active(&self, ui: &Ui) -> bool {
        self.is_interactive() && self.disabled.is_none() && ui.is_enabled()
    }

    fn interact(&self, ui: &mut Ui, rect: Rect, id: Id, shown: bool) -> Response {
        let sense = if shown && self.active(ui) {
            Sense::click()
        } else {
            Sense::hover()
        };
        ui.interact(rect, id, sense)
    }

    fn finish(
        self,
        ui: &mut Ui,
        painter: &Painter,
        response: Response,
        mark: Rect,
        placement: Placement,
    ) -> Response {
        let ctx = ui.ctx().clone();
        let t = Tokens::of(&ctx);
        let it = IndicatorTokens::of(&ctx);
        let shown = self.shown(&ctx);
        let enabled = self.active(ui);
        let interactive = self.is_interactive();
        let pulse = if shown {
            self.pulse(&ctx, &it)
        } else {
            Pulse::None
        };
        if shown && placement.visible {
            if interactive {
                let target = response.rect;
                let radius = CornerRadius::same(t.radius.widget.round() as u8);
                if enabled && (response.hovered() || response.is_pointer_button_down_on()) {
                    painter.rect_filled(target, radius, t.surface.hover);
                }
                if response.has_focus() {
                    painter.rect_stroke(
                        target,
                        radius,
                        Stroke::new(scale::stroke(&ctx, 1.5), t.focus_ring),
                        StrokeKind::Inside,
                    );
                }
            }
            let mut color = self.color(&t, &it);
            if interactive && !enabled {
                color = t.text.muted;
            } else if interactive && response.hovered() && matches!(self.kind, Kind::Icon { .. }) {
                color = t.text.strong;
            }
            self.paint(
                painter,
                mark,
                &Look {
                    color,
                    on_host: placement.on_host,
                    corner: placement.corner,
                    pulse,
                },
                &it,
                &t,
            );
        }
        let meaning = self.meaning(&ctx);
        let mut response = response;
        if interactive && shown {
            let action = self.action.clone().unwrap_or_default();
            let ui_enabled = enabled;
            match &self.kind {
                Kind::Icon {
                    toggle: Some((on, _)),
                    ..
                } => {
                    let on = *on;
                    response.widget_info(|| {
                        WidgetInfo::selected(WidgetType::Checkbox, ui_enabled, on, &action)
                    });
                }
                _ => response
                    .widget_info(|| WidgetInfo::labeled(WidgetType::Button, ui_enabled, &action)),
            }
            if let Some(m) = &meaning {
                ctx.accesskit_node_builder(response.id, |node| node.set_description(m.clone()));
            }
            let tip = self.tooltip.clone().unwrap_or_else(|| action.clone());
            response = match &self.disabled {
                Some(reason) => response.on_hover_text(format!("{action}: {reason}")),
                None => response.on_hover_text(tip),
            };
            if enabled
                && response.clicked()
                && let Some(c) = &self.command
            {
                lock(&drawn_state(&ctx)).commands.push(c.clone());
                ctx.request_repaint();
            }
        } else if shown {
            if !placement.on_host
                && let Some(m) = &meaning
            {
                let enabled = ui.is_enabled();
                response.widget_info(|| WidgetInfo::labeled(WidgetType::Image, enabled, m));
            }
            if let Some(tip) = self.tooltip.clone().or_else(|| meaning.clone()) {
                response = response.on_hover_text(tip);
            }
        }
        if self.key.is_some() {
            let mut r = self.describe(&ctx);
            r.shown = shown;
            r.pulsing = !matches!(pulse, Pulse::None);
            r.host = placement.host;
            r.rect = [mark.min.x, mark.min.y, mark.width(), mark.height()];
            record(&ctx, r);
        }
        response
    }

    fn color(&self, t: &Tokens, it: &IndicatorTokens) -> Color32 {
        match (&self.kind, self.tone) {
            (Kind::Dot(Some(Led::Off)), None) => it.off,
            (_, _) => match self.tone_of() {
                Tone::Record => it.record,
                tone => tone.color(t),
            },
        }
    }

    fn paint(&self, p: &Painter, rect: Rect, look: &Look, it: &IndicatorTokens, t: &Tokens) {
        let color = look.color;
        let c = rect.center();
        let r = rect.width().min(rect.height()) / 2.0;
        match &self.kind {
            Kind::Dot(led) => {
                if look.on_host {
                    p.circle_filled(c, r + 1.0, it.outline);
                }
                match led {
                    Some(Led::Off) => {
                        p.circle_stroke(c, r - it.ring / 2.0, Stroke::new(it.ring, color));
                    }
                    Some(Led::Warning) => {
                        p.add(Shape::convex_polygon(
                            vec![
                                pos2(c.x, c.y - r),
                                pos2(c.x + r, c.y + r * 0.8),
                                pos2(c.x - r, c.y + r * 0.8),
                            ],
                            color,
                            Stroke::NONE,
                        ));
                    }
                    Some(Led::Error) => {
                        p.rect_filled(
                            Rect::from_center_size(c, Vec2::splat(r * 1.7)),
                            CornerRadius::same(1),
                            color,
                        );
                    }
                    _ => {
                        p.circle_filled(c, r, color);
                    }
                }
                paint_pulse_ring(p, c, r, look, it);
            }
            Kind::Badge { .. } => {
                let radius = CornerRadius::same((rect.height() / 2.0).round() as u8);
                if look.on_host {
                    p.rect_filled(
                        rect.expand(1.0),
                        CornerRadius::same((rect.height() / 2.0 + 1.0).round() as u8),
                        it.outline,
                    );
                }
                p.rect_filled(rect, radius, color);
                let font = FontId::proportional(it.badge_font * self.size.factor());
                p.text(
                    c,
                    Align2::CENTER_CENTER,
                    self.label().unwrap_or_default(),
                    font,
                    it.badge_text,
                );
                match look.pulse {
                    Pulse::Moving(phase) => {
                        p.rect_stroke(
                            rect.expand(1.0 + 4.0 * phase),
                            CornerRadius::same((rect.height() / 2.0 + 1.0 + 4.0 * phase) as u8),
                            Stroke::new(it.ring, color.gamma_multiply(1.0 - phase)),
                            StrokeKind::Outside,
                        );
                    }
                    Pulse::Still => {
                        p.rect_stroke(
                            rect.expand(2.0),
                            CornerRadius::same((rect.height() / 2.0 + 2.0) as u8),
                            Stroke::new(it.ring, color),
                            StrokeKind::Outside,
                        );
                    }
                    Pulse::None => {}
                }
            }
            Kind::Pip(pip) => match pip {
                Pip::Busy => progress::paint_spinner_on(p, rect, color),
                Pip::Progress(f) => progress::paint_ring_on(p, rect, Some(*f), color),
                Pip::Done => Icon::Check.paint(p, rect, color),
                Pip::Failed => Icon::Error.paint(p, rect, color),
            },
            Kind::Flag(shape, _) => {
                paint_flag(p, *shape, rect, look.corner, color);
                paint_pulse_ring(p, c, r, look, it);
            }
            Kind::Swatch(swatch) => {
                let radius = CornerRadius::same(it.swatch_radius.round() as u8);
                match swatch {
                    Swatch::Color(col) if col.is_opaque() => {
                        p.rect_filled(rect, radius, *col);
                    }
                    // The checkerboard under alpha is egui's.
                    Swatch::Color(col) => egui::widgets::color_picker::show_color_at(p, *col, rect),
                    Swatch::None => {
                        let inset = rect.shrink(rect.width() * 0.15);
                        p.line_segment(
                            [inset.left_bottom(), inset.right_top()],
                            Stroke::new((rect.width() / 10.0).max(1.0), t.text.muted),
                        );
                    }
                    Swatch::Mixed => {
                        let half = rect.width() * 0.25;
                        p.line_segment(
                            [c - vec2(half, 0.0), c + vec2(half, 0.0)],
                            Stroke::new((rect.width() / 8.0).max(1.5), t.text.default),
                        );
                    }
                }
                p.rect_stroke(
                    rect,
                    radius,
                    Stroke::new(1.0, it.swatch_border),
                    StrokeKind::Inside,
                );
                paint_pulse_ring(p, c, r, look, it);
            }
            Kind::Icon { icon, toggle } => {
                let icon = match toggle {
                    Some((false, off)) => *off,
                    _ => *icon,
                };
                icon.paint(p, rect, color);
            }
        }
    }
}

impl Widget for Indicator {
    fn ui(self, ui: &mut Ui) -> Response {
        let ctx = ui.ctx().clone();
        let it = IndicatorTokens::of(&ctx);
        let shown = self.shown(&ctx);
        let mark = if shown {
            self.mark_size(ui.painter(), &it)
        } else {
            Vec2::ZERO
        };
        let outer = if self.is_interactive() && shown {
            vec2(mark.x.max(it.target), mark.y.max(it.target))
        } else {
            mark
        };
        // Nothing to show (a badge at zero, a pip not yet due): no room
        // taken, no node.
        let (auto, rect) = if shown {
            ui.allocate_space(outer)
        } else {
            (
                ui.next_auto_id(),
                Rect::from_min_size(ui.cursor().min, Vec2::ZERO),
            )
        };
        let id = self.egui_id(auto);
        let response = self.interact(ui, rect, id, shown);
        let visible = ui.is_rect_visible(rect);
        let painter = ui.painter().clone();
        self.finish(
            ui,
            &painter,
            response,
            Rect::from_center_size(rect.center(), mark),
            Placement {
                on_host: false,
                corner: (direction(&ctx) == Direction::RightToLeft, false),
                visible,
                host: None,
            },
        )
    }
}

/// Indicators on a widget's response.
pub trait IndicatorExt {
    /// Paints an indicator on this widget ([`Indicator::on`]) and
    /// returns the widget's response.
    fn with_indicator(self, ui: &mut Ui, indicator: Indicator, anchor: Anchor) -> Response;
}

impl IndicatorExt for Response {
    fn with_indicator(self, ui: &mut Ui, indicator: Indicator, anchor: Anchor) -> Response {
        indicator.on(ui, &self, anchor);
        self
    }
}

/// What agents and tests see of a drawn indicator.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Record {
    /// Its id ([`Indicator::id`]).
    pub id: String,
    /// `dot`, `led`, `badge`, `pip`, `flag`, `swatch` or `icon`.
    pub kind: &'static str,
    /// The LED's, pip's or flag's state, `count` or `text` for a badge,
    /// `color`, `none` or `mixed` for a swatch, `on` or `off` for a
    /// toggle, the icon's name, `shown` for a dot.
    pub state: String,
    /// A badge's count, a pip's fraction, a toggle's 1 or 0.
    pub value: Option<f64>,
    /// What it shows: a badge's text (`99+`), a swatch's colour.
    pub text: Option<String>,
    pub tone: Tone,
    /// Its meaning, as read by screen readers.
    pub name: Option<String>,
    /// An interactive one's action.
    pub action: Option<String>,
    /// The accessible name of the widget it sits on, before its meaning
    /// joined it.
    pub host: Option<String>,
    pub interactive: bool,
    pub enabled: bool,
    /// Whether it shows (a badge at zero, a delayed pip do not).
    pub shown: bool,
    pub pulsing: bool,
    pub command: Option<String>,
    /// Its mark's rectangle in points: x, y, width, height.
    pub rect: [f32; 4],
}

/// The indicators with an id drawn in the last complete pass.
pub fn drawn(ctx: &Context) -> Vec<Record> {
    let st = drawn_state(ctx);
    let mut s = lock(&st);
    rotate(ctx, &mut s);
    s.last.clone()
}

/// One drawn indicator by id.
pub fn find(ctx: &Context, id: &str) -> Option<Record> {
    drawn(ctx).into_iter().find(|r| r.id == id)
}

/// The commands interactive indicators asked for since the last call;
/// the application runs them (an egui-pop `App` does, each frame).
pub fn take_commands(ctx: &Context) -> Vec<CommandId> {
    std::mem::take(&mut lock(&drawn_state(ctx)).commands)
}

#[derive(Default)]
struct Drawn {
    pass: u64,
    building: Vec<Record>,
    last: Vec<Record>,
    commands: Vec<CommandId>,
    direction: Direction,
}

fn drawn_state(ctx: &Context) -> Arc<Mutex<Drawn>> {
    shared::<Drawn>(ctx, "indicators")
}

fn rotate(ctx: &Context, s: &mut Drawn) {
    let now = pass(ctx);
    if s.pass != now {
        s.pass = now;
        s.last = std::mem::take(&mut s.building);
    }
}

fn record(ctx: &Context, r: Record) {
    let st = drawn_state(ctx);
    let mut s = lock(&st);
    rotate(ctx, &mut s);
    s.building.retain(|x| x.id != r.id);
    s.building.push(r);
}

fn host_label(ctx: &Context, id: Id) -> Option<String> {
    ctx.accesskit_node_builder(id, |node| node.label().map(str::to_string))
        .flatten()
        .filter(|l| !l.is_empty())
}

/// A painter on a layer of its own just above the host's, unclipped,
/// with the host layer's transform (a pan and zoom).
fn overhang_painter(ui: &Ui) -> Painter {
    let ctx = ui.ctx();
    let parent = ui.layer_id();
    let layer = LayerId::new(parent.order, parent.id.with("egui_pop_indicator"));
    ctx.set_sublayer(parent, layer);
    if let Some(transform) = ctx.layer_transform_to_global(parent) {
        ctx.set_transform_layer(layer, transform);
    }
    Painter::new(ctx.clone(), layer, Rect::EVERYTHING)
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Pulse {
    None,
    /// 0 to 1 through one pulse.
    Moving(f32),
    /// Reduced motion: a still ring.
    Still,
}

struct Look {
    color: Color32,
    on_host: bool,
    /// The corner it sits in: right, bottom.
    corner: (bool, bool),
    pulse: Pulse,
}

struct Placement {
    on_host: bool,
    corner: (bool, bool),
    visible: bool,
    host: Option<String>,
}

fn paint_pulse_ring(p: &Painter, c: Pos2, r: f32, look: &Look, it: &IndicatorTokens) {
    match look.pulse {
        Pulse::Moving(phase) => {
            p.circle_stroke(
                c,
                r + it.ring + r * 1.2 * phase,
                Stroke::new(it.ring, look.color.gamma_multiply(1.0 - phase)),
            );
        }
        Pulse::Still => {
            p.circle_stroke(c, r + it.ring * 1.5 + 1.0, Stroke::new(it.ring, look.color));
        }
        Pulse::None => {}
    }
}

fn paint_flag(p: &Painter, shape: FlagShape, rect: Rect, corner: (bool, bool), color: Color32) {
    let c = rect.center();
    let s = rect.width().min(rect.height());
    let h = s / 2.0;
    match shape {
        FlagShape::Corner => {
            let (right, bottom) = corner;
            let x = if right { rect.right() } else { rect.left() };
            let y = if bottom { rect.bottom() } else { rect.top() };
            let dx = if right { -rect.width() } else { rect.width() };
            let dy = if bottom {
                -rect.height()
            } else {
                rect.height()
            };
            p.add(Shape::convex_polygon(
                vec![pos2(x, y), pos2(x + dx, y), pos2(x, y + dy)],
                color,
                Stroke::NONE,
            ));
        }
        FlagShape::Stripe => {
            p.rect_filled(rect, CornerRadius::same(1), color);
        }
        FlagShape::Diamond => {
            p.add(Shape::convex_polygon(
                vec![
                    pos2(c.x, c.y - h),
                    pos2(c.x + h, c.y),
                    pos2(c.x, c.y + h),
                    pos2(c.x - h, c.y),
                ],
                color,
                Stroke::NONE,
            ));
        }
        FlagShape::Triangle => {
            p.add(Shape::convex_polygon(
                vec![
                    pos2(c.x, c.y - h),
                    pos2(c.x + h, c.y + h * 0.8),
                    pos2(c.x - h, c.y + h * 0.8),
                ],
                color,
                Stroke::NONE,
            ));
        }
        FlagShape::Square => {
            p.rect_filled(
                Rect::from_center_size(c, Vec2::splat(s * 0.8)),
                CornerRadius::same(1),
                color,
            );
        }
        FlagShape::Asterisk => {
            let stroke = Stroke::new((s / 5.0).max(1.0), color);
            for k in 0..3 {
                let a = std::f32::consts::FRAC_PI_2 + k as f32 * std::f32::consts::PI / 3.0;
                let d = vec2(a.cos(), a.sin()) * h;
                p.line_segment([c - d, c + d], stroke);
            }
        }
    }
}

fn hex(c: Color32) -> String {
    if c.a() == 255 {
        format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
    } else {
        let [r, g, b, a] = c.to_srgba_unmultiplied();
        format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_badge_caps_its_count_and_hides_zero() {
        assert_eq!(badge_text(0, CAP), "0");
        assert_eq!(badge_text(9, CAP), "9");
        assert_eq!(badge_text(99, CAP), "99");
        assert_eq!(badge_text(100, CAP), "99+");
        assert_eq!(badge_text(1000, 999), "999+");
        assert!(!Indicator::count(0).has_content());
        assert!(Indicator::count(0).show_zero(true).has_content());
        assert_eq!(Indicator::count(250).label().as_deref(), Some("99+"));
        assert_eq!(
            Indicator::count(250).cap(999).label().as_deref(),
            Some("250")
        );
        assert!(!Indicator::text("").has_content());
        assert!(Indicator::text("NEW").has_content());
    }

    #[test]
    fn a_capped_badge_still_says_its_real_count() {
        let ctx = Context::default();
        let r = Indicator::count(1234).id("unread").describe(&ctx);
        assert_eq!(r.text.as_deref(), Some("99+"));
        assert_eq!(r.name.as_deref(), Some("1234"));
        assert_eq!(r.value, Some(1234.0));
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(json["kind"], "badge");
        assert_eq!(json["state"], "count");
        assert_eq!(json["tone"], "accent");
        assert_eq!(json["interactive"], false);
    }

    #[test]
    fn anchors_follow_the_direction() {
        let host = Rect::from_min_size(pos2(100.0, 100.0), vec2(40.0, 20.0));
        let size = vec2(8.0, 8.0);
        let ltr = Anchor::TopEnd.place(host, size, Direction::LeftToRight, false, 2.0);
        assert_eq!(ltr.right_top(), pos2(138.0, 102.0));
        let rtl = Anchor::TopEnd.place(host, size, Direction::RightToLeft, false, 2.0);
        assert_eq!(rtl.left_top(), pos2(102.0, 102.0));
        let start = Anchor::Start.place(host, size, Direction::LeftToRight, false, 0.0);
        assert_eq!(start.left_center(), pos2(100.0, 110.0));
        let end = Anchor::End.place(host, size, Direction::RightToLeft, false, 0.0);
        assert_eq!(end.left_center(), pos2(100.0, 110.0));
        let center = Anchor::Center.place(host, size, Direction::LeftToRight, true, 2.0);
        assert_eq!(center.center(), host.center());
    }

    #[test]
    fn an_overhanging_mark_passes_the_corner_by_half_its_height() {
        let host = Rect::from_min_size(pos2(0.0, 0.0), vec2(24.0, 24.0));
        // A dot is centred on the corner.
        let dot = Anchor::TopEnd.place(host, vec2(8.0, 8.0), Direction::LeftToRight, true, 2.0);
        assert_eq!(dot.center(), pos2(24.0, 0.0));
        // A wider badge has its end cap on the corner, its middle on the edge.
        let badge =
            Anchor::BottomStart.place(host, vec2(28.0, 16.0), Direction::LeftToRight, true, 2.0);
        assert_eq!(badge.left(), -8.0);
        assert_eq!(badge.center().y, 24.0);
        // Edges never overhang.
        let end = Anchor::End.place(host, vec2(8.0, 8.0), Direction::LeftToRight, true, 2.0);
        assert!(host.contains_rect(end));
    }

    #[test]
    fn defaults_name_every_state_without_colour() {
        let ctx = Context::default();
        for led in Led::ALL {
            assert!(Indicator::led(led).meaning(&ctx).is_some(), "{led:?}");
        }
        for flag in Flag::ALL {
            assert!(Indicator::flag(flag).meaning(&ctx).is_some(), "{flag:?}");
        }
        for pip in [Pip::Busy, Pip::Progress(0.42), Pip::Done, Pip::Failed] {
            assert!(Indicator::pip(pip).meaning(&ctx).is_some(), "{pip:?}");
        }
        assert_eq!(
            Indicator::pip(Pip::Progress(0.42)).meaning(&ctx).as_deref(),
            Some("42%")
        );
        assert_eq!(
            Indicator::swatch(Color32::from_rgb(255, 0, 16))
                .meaning(&ctx)
                .as_deref(),
            Some("#ff0010")
        );
        // A plain dot has no meaning until it is given one.
        assert_eq!(Indicator::dot(Tone::Accent).meaning(&ctx), None);
        assert_eq!(
            Indicator::dot(Tone::Accent)
                .name("Changed")
                .meaning(&ctx)
                .as_deref(),
            Some("Changed")
        );
    }

    #[test]
    fn the_pulse_is_still_under_reduced_motion_and_ends() {
        let ctx = Context::default();
        crate::theme::install(&ctx, &crate::theme::ThemeSpec::new("pop-dark"));
        let clock = motion::freeze(&ctx, 10.0);
        let it = IndicatorTokens::of(&ctx);
        let busy = Indicator::led(Led::Busy);
        let Pulse::Moving(a) = busy.pulse(&ctx, &it) else {
            panic!("a busy LED pulses");
        };
        assert_eq!(
            busy.pulse(&ctx, &it),
            Pulse::Moving(a),
            "a frozen clock, a frozen pulse"
        );
        let called = Indicator::count(3).attention(10.0);
        assert!(matches!(called.pulse(&ctx, &it), Pulse::Moving(_)));
        clock.set(10.0 + f64::from(it.pulse_period) * f64::from(it.pulse_count) + 0.01);
        assert_eq!(called.pulse(&ctx, &it), Pulse::None, "attention ends");
        assert_eq!(Indicator::led(Led::On).pulse(&ctx, &it), Pulse::None);
        crate::theme::install(
            &ctx,
            &crate::theme::ThemeSpec::new("pop-dark").reduced_motion(true),
        );
        assert_eq!(busy.pulse(&ctx, &it), Pulse::Still);
    }

    #[test]
    fn the_interactive_target_is_never_under_24_points() {
        let ctx = Context::default();
        crate::theme::install(&ctx, &crate::theme::ThemeSpec::new("pop-dark"));
        assert!(IndicatorTokens::of(&ctx).target >= 24.0);
    }
}
