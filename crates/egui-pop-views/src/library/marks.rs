//! An image's marks: a rating (0 to 5 stars), colour labels and a pick
//! or reject flag; the widgets editing them; the culling keys.
//!
//! The application owns the values: each widget takes the current one
//! and says what the user asked for ([`MarkResponse::changed`]); the
//! application applies it to its data (or not), and passes the new
//! value at the next pass.
//!
//! ```
//! use egui_pop_views::library::marks::{ColorLabels, FlagToggle, Flag, Labels, RatingStars};
//!
//! struct Photo { rating: u8, labels: Labels, flag: Flag }
//!
//! fn ui(ui: &mut egui::Ui, p: &mut Photo) {
//!     if let Some(r) = RatingStars::new(p.rating).show(ui).changed {
//!         p.rating = r;
//!     }
//!     if let Some(l) = ColorLabels::new(p.labels).show(ui).changed {
//!         p.labels = l;
//!     }
//!     if let Some(f) = FlagToggle::new(p.flag).show(ui).changed {
//!         p.flag = f;
//!     }
//! }
//! ```

use egui::{Key as EKey, Response, Sense, Ui, WidgetInfo, WidgetType, vec2};
use egui_pop_core::kit::{Kit, KitCx, Part, State};
use egui_pop_core::strings::{StrKey, tr, tr_args};
use egui_pop_core::theme::Tokens;
use serde::{Deserialize, Serialize};

use super::paint::{self, LibraryTokens};
use crate::strings as s;

/// The highest rating.
pub const MAX_RATING: u8 = 5;

/// Picked, rejected, or neither.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Flag {
    #[default]
    None,
    Pick,
    Reject,
}

impl Flag {
    pub const ALL: [Flag; 3] = [Flag::Pick, Flag::Reject, Flag::None];

    /// As in step arguments: `none`, `pick`, `reject`.
    pub fn name(self) -> &'static str {
        match self {
            Flag::None => "none",
            Flag::Pick => "pick",
            Flag::Reject => "reject",
        }
    }

    pub fn parse(text: &str) -> Option<Flag> {
        Flag::ALL.into_iter().find(|f| f.name() == text)
    }

    /// Its name as shown (translated).
    pub fn title(self, ctx: &egui::Context) -> String {
        tr(
            ctx,
            match self {
                Flag::None => &s::UNFLAGGED,
                Flag::Pick => &s::PICK,
                Flag::Reject => &s::REJECT,
            },
        )
    }
}

/// One of the five colour labels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorLabel {
    Red,
    Yellow,
    Green,
    Blue,
    Purple,
}

impl ColorLabel {
    pub const ALL: [ColorLabel; 5] = [
        ColorLabel::Red,
        ColorLabel::Yellow,
        ColorLabel::Green,
        ColorLabel::Blue,
        ColorLabel::Purple,
    ];

    /// As in step arguments: `red`, `yellow`, `green`, `blue`, `purple`.
    pub fn name(self) -> &'static str {
        match self {
            ColorLabel::Red => "red",
            ColorLabel::Yellow => "yellow",
            ColorLabel::Green => "green",
            ColorLabel::Blue => "blue",
            ColorLabel::Purple => "purple",
        }
    }

    pub fn parse(text: &str) -> Option<ColorLabel> {
        ColorLabel::ALL.into_iter().find(|l| l.name() == text)
    }

    pub(crate) fn index(self) -> usize {
        self as usize
    }

    fn key(self) -> &'static StrKey {
        match self {
            ColorLabel::Red => &s::RED,
            ColorLabel::Yellow => &s::YELLOW,
            ColorLabel::Green => &s::GREEN,
            ColorLabel::Blue => &s::BLUE,
            ColorLabel::Purple => &s::PURPLE,
        }
    }

    /// Its name as shown (translated): "Red".
    pub fn title(self, ctx: &egui::Context) -> String {
        tr(ctx, self.key())
    }
}

/// A set of colour labels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Labels(u8);

impl Labels {
    pub const NONE: Labels = Labels(0);

    pub fn has(self, label: ColorLabel) -> bool {
        self.0 & (1 << label.index()) != 0
    }

    /// With `label` on or off.
    #[must_use]
    pub fn with(self, label: ColorLabel, on: bool) -> Labels {
        let bit = 1 << label.index();
        Labels(if on { self.0 | bit } else { self.0 & !bit })
    }

    #[must_use]
    pub fn toggled(self, label: ColorLabel) -> Labels {
        self.with(label, !self.has(label))
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The labels set, red first.
    pub fn iter(self) -> impl Iterator<Item = ColorLabel> {
        ColorLabel::ALL.into_iter().filter(move |l| self.has(*l))
    }
}

impl FromIterator<ColorLabel> for Labels {
    fn from_iter<T: IntoIterator<Item = ColorLabel>>(iter: T) -> Self {
        iter.into_iter().fold(Labels::NONE, |l, c| l.with(c, true))
    }
}

/// An image's marks, as the application gives them to the grid.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Marks {
    /// 0 (none) to [`MAX_RATING`].
    pub rating: u8,
    pub labels: Labels,
    pub flag: Flag,
}

/// What a culling key does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CullAction {
    /// Sets the rating (0 clears it).
    Rate(u8),
    /// Sets the flag.
    Flag(Flag),
    /// Toggles a colour label.
    Label(ColorLabel),
    /// Expands or collapses the stack under the cursor.
    ToggleStack,
    /// Larger or smaller thumbnails (the grid only).
    ZoomIn,
    ZoomOut,
}

/// The keys a focused grid or filmstrip reads for marks, and whether
/// marking moves to the next image (auto-advance).
///
/// Keys go through the key dispatcher first: a key the application
/// binds to a command never reaches the view.
#[derive(Clone, Debug, PartialEq)]
pub struct CullKeymap {
    pub bindings: Vec<(EKey, CullAction)>,
    /// After a rating, a flag or a label, the cursor moves to the next
    /// image (Lightroom's Caps Lock mode).
    pub auto_advance: bool,
}

impl Default for CullKeymap {
    fn default() -> Self {
        CullKeymap::lightroom()
    }
}

impl CullKeymap {
    /// The culling keys of Lightroom Classic, which Capture One's
    /// culling also uses for ratings: 0 to 5 rate, P picks, X rejects,
    /// U unflags, 6 to 9 toggle red, yellow, green and blue; S expands
    /// or collapses a stack, + and - change the thumbnails' size.
    pub fn lightroom() -> CullKeymap {
        use CullAction as A;
        CullKeymap {
            bindings: vec![
                (EKey::Num0, A::Rate(0)),
                (EKey::Num1, A::Rate(1)),
                (EKey::Num2, A::Rate(2)),
                (EKey::Num3, A::Rate(3)),
                (EKey::Num4, A::Rate(4)),
                (EKey::Num5, A::Rate(5)),
                (EKey::P, A::Flag(Flag::Pick)),
                (EKey::X, A::Flag(Flag::Reject)),
                (EKey::U, A::Flag(Flag::None)),
                (EKey::Num6, A::Label(ColorLabel::Red)),
                (EKey::Num7, A::Label(ColorLabel::Yellow)),
                (EKey::Num8, A::Label(ColorLabel::Green)),
                (EKey::Num9, A::Label(ColorLabel::Blue)),
                (EKey::S, A::ToggleStack),
                (EKey::Plus, A::ZoomIn),
                (EKey::Equals, A::ZoomIn),
                (EKey::Minus, A::ZoomOut),
            ],
            auto_advance: false,
        }
    }

    /// No culling keys (navigation and selection keys only).
    pub fn none() -> CullKeymap {
        CullKeymap {
            bindings: Vec::new(),
            auto_advance: false,
        }
    }

    #[must_use]
    pub fn auto_advance(mut self, on: bool) -> CullKeymap {
        self.auto_advance = on;
        self
    }

    /// Binds a key (replacing its binding).
    #[must_use]
    pub fn bind(mut self, key: EKey, action: CullAction) -> CullKeymap {
        self.bindings.retain(|(k, _)| *k != key);
        self.bindings.push((key, action));
        self
    }

    /// What `key` does.
    pub fn action(&self, key: EKey) -> Option<CullAction> {
        self.bindings
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, a)| *a)
    }
}

/// What a mark widget returns: its response and the value the user
/// asked for.
#[derive(Debug)]
pub struct MarkResponse<T> {
    pub response: Response,
    pub changed: Option<T>,
}

/// Five stars: a click on a star gives that rating (on the current one,
/// none); focused, 0 to 5 set it, arrows step it, Home and End go to
/// the ends. A slider for assistive technologies.
#[derive(Clone, Debug)]
#[must_use = "show it with `show`"]
pub struct RatingStars {
    value: u8,
    label: Option<String>,
    size: Option<f32>,
}

impl RatingStars {
    pub fn new(value: u8) -> RatingStars {
        RatingStars {
            value: value.min(MAX_RATING),
            label: None,
            size: None,
        }
    }

    /// Its accessible name ("Rating" by default).
    pub fn label(mut self, label: impl Into<String>) -> RatingStars {
        self.label = Some(label.into());
        self
    }

    /// One star's size in points (default: the inline icon size).
    pub fn size(mut self, size: f32) -> RatingStars {
        self.size = Some(size);
        self
    }

    pub fn show(self, ui: &mut Ui) -> MarkResponse<u8> {
        let t = Tokens::of(ui.ctx());
        let lt = LibraryTokens::of(ui.ctx());
        let star = self.size.unwrap_or(t.size.icon_inline.max(12.0) + 4.0);
        let (rect, response) =
            ui.allocate_exact_size(vec2(star * f32::from(MAX_RATING), star), Sense::click());
        let at = |x: f32| (((x - rect.left()) / star).floor() as i32).clamp(0, 4) as u8 + 1;
        let hover = response.hover_pos().map(|p| at(p.x));
        let mut changed = None;
        if response.clicked()
            && let Some(p) = response.interact_pointer_pos()
        {
            let r = at(p.x);
            changed = Some(if r == self.value { 0 } else { r });
        }
        if response.has_focus() {
            changed = changed.or_else(|| {
                ui.input(|i| {
                    let mut v = None;
                    for (k, r) in [
                        (EKey::Num0, 0),
                        (EKey::Num1, 1),
                        (EKey::Num2, 2),
                        (EKey::Num3, 3),
                        (EKey::Num4, 4),
                        (EKey::Num5, 5),
                    ] {
                        if i.key_pressed(k) {
                            v = Some(r);
                        }
                    }
                    if i.key_pressed(EKey::ArrowRight) || i.key_pressed(EKey::ArrowUp) {
                        v = Some((self.value + 1).min(MAX_RATING));
                    }
                    if i.key_pressed(EKey::ArrowLeft) || i.key_pressed(EKey::ArrowDown) {
                        v = Some(self.value.saturating_sub(1));
                    }
                    if i.key_pressed(EKey::Home) {
                        v = Some(0);
                    }
                    if i.key_pressed(EKey::End) {
                        v = Some(MAX_RATING);
                    }
                    v
                })
            });
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    response.id,
                    egui::EventFilter {
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        ..Default::default()
                    },
                )
            });
        }
        if changed == Some(self.value) {
            changed = None;
        }
        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            let shown = hover.unwrap_or(self.value);
            for i in 0..MAX_RATING {
                let c = rect.left_center() + vec2(star * (f32::from(i) + 0.5), 0.0);
                let fill = if i < shown {
                    Some(if hover.is_some() {
                        lt.star.gamma_multiply(0.8)
                    } else {
                        lt.star
                    })
                } else {
                    None
                };
                paint::star(painter, c, star * 0.45, fill, lt.star_empty);
            }
            if response.has_focus() {
                painter.rect_stroke(
                    rect.expand(1.0),
                    t.radius.widget,
                    egui::Stroke::new(1.0, t.focus_ring),
                    egui::StrokeKind::Outside,
                );
            }
        }
        let label = self.label.unwrap_or_else(|| tr(ui.ctx(), &s::RATING));
        let value = f64::from(self.value);
        let enabled = ui.is_enabled();
        response.widget_info(move || WidgetInfo::slider(enabled, value, &label));
        ui.ctx().accesskit_node_builder(response.id, |n| {
            n.set_min_numeric_value(0.0);
            n.set_max_numeric_value(f64::from(MAX_RATING));
            n.set_numeric_value_step(1.0);
        });
        MarkResponse { response, changed }
    }
}

/// A toggle button per colour label, each its shape in its colour.
#[derive(Clone, Debug)]
#[must_use = "show it with `show`"]
pub struct ColorLabels {
    value: Labels,
    size: Option<f32>,
}

impl ColorLabels {
    pub fn new(value: Labels) -> ColorLabels {
        ColorLabels { value, size: None }
    }

    /// One button's size in points.
    pub fn size(mut self, size: f32) -> ColorLabels {
        self.size = Some(size);
        self
    }

    pub fn show(self, ui: &mut Ui) -> MarkResponse<Labels> {
        let t = Tokens::of(ui.ctx());
        let lt = LibraryTokens::of(ui.ctx());
        let side = self.size.unwrap_or(t.size.control_height);
        let mut changed = None;
        let inner = ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = t.space.xs;
            for label in ColorLabel::ALL {
                let on = self.value.has(label);
                let r = toggle(ui, side, on, |painter, rect| {
                    let color = lt.label(label);
                    let color = if on {
                        color
                    } else {
                        color.gamma_multiply(0.45)
                    };
                    paint::label(painter, label, rect.center(), rect.height() * 0.28, color);
                });
                let name = tr_args(
                    ui.ctx(),
                    &s::LABEL_TOGGLE,
                    &[("label", &label.title(ui.ctx()))],
                );
                r.widget_info(move || WidgetInfo::selected(WidgetType::Checkbox, true, on, &name));
                if r.clicked() {
                    changed = Some(self.value.toggled(label));
                }
            }
        });
        MarkResponse {
            response: inner.response,
            changed,
        }
    }
}

/// Two toggle buttons: pick and reject (a click on the one set unflags).
#[derive(Clone, Debug)]
#[must_use = "show it with `show`"]
pub struct FlagToggle {
    value: Flag,
    size: Option<f32>,
}

impl FlagToggle {
    pub fn new(value: Flag) -> FlagToggle {
        FlagToggle { value, size: None }
    }

    /// One button's size in points.
    pub fn size(mut self, size: f32) -> FlagToggle {
        self.size = Some(size);
        self
    }

    pub fn show(self, ui: &mut Ui) -> MarkResponse<Flag> {
        let t = Tokens::of(ui.ctx());
        let lt = LibraryTokens::of(ui.ctx());
        let side = self.size.unwrap_or(t.size.control_height);
        let mut changed = None;
        let inner = ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = t.space.xs;
            for flag in [Flag::Pick, Flag::Reject] {
                let on = self.value == flag;
                let r = toggle(ui, side, on, |painter, rect| {
                    let icon = rect.shrink(rect.width() * 0.22);
                    match flag {
                        Flag::Pick => {
                            paint::flag(painter, icon, if on { lt.pick } else { lt.star_empty })
                        }
                        _ => {
                            paint::cross(painter, icon, if on { lt.reject } else { lt.star_empty })
                        }
                    }
                });
                let name = flag.title(ui.ctx());
                r.widget_info(move || WidgetInfo::selected(WidgetType::Checkbox, true, on, &name));
                if r.clicked() {
                    changed = Some(if on { Flag::None } else { flag });
                }
            }
        });
        MarkResponse {
            response: inner.response,
            changed,
        }
    }
}

/// A square toggle button (the kit's `mark-toggle`), its mark drawn by
/// `paint`.
fn toggle(
    ui: &mut Ui,
    side: f32,
    on: bool,
    paint: impl FnOnce(&egui::Painter, egui::Rect),
) -> Response {
    let (rect, r) = ui.allocate_exact_size(vec2(side, side), Sense::click());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        // On wins over the pointer: its fill stays under it.
        let state = State::REST
            .with(State::SELECTED, on)
            .with(State::HOVER, r.hovered() && !on)
            .with(State::FOCUS, r.has_focus());
        Kit::of(ui.ctx()).mark_toggle.paint(
            &KitCx {
                painter,
                ppp: ui.ctx().pixels_per_point(),
            },
            &Part {
                rect,
                room: rect,
                state,
                label: None,
            },
        );
        paint(painter, rect);
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_toggle_and_names_round_trip() {
        let l = Labels::NONE
            .toggled(ColorLabel::Red)
            .toggled(ColorLabel::Blue);
        assert_eq!(
            l.iter().collect::<Vec<_>>(),
            [ColorLabel::Red, ColorLabel::Blue]
        );
        assert!(l.toggled(ColorLabel::Red).has(ColorLabel::Blue));
        assert!(!l.toggled(ColorLabel::Red).has(ColorLabel::Red));
        for c in ColorLabel::ALL {
            assert_eq!(ColorLabel::parse(c.name()), Some(c));
        }
        for f in Flag::ALL {
            assert_eq!(Flag::parse(f.name()), Some(f));
        }
    }

    #[test]
    fn the_lightroom_keys_rate_flag_and_label() {
        let k = CullKeymap::lightroom();
        assert_eq!(k.action(EKey::Num3), Some(CullAction::Rate(3)));
        assert_eq!(k.action(EKey::X), Some(CullAction::Flag(Flag::Reject)));
        assert_eq!(
            k.action(EKey::Num8),
            Some(CullAction::Label(ColorLabel::Green))
        );
        assert_eq!(k.action(EKey::Q), None);
        let k = k.bind(EKey::Num9, CullAction::Label(ColorLabel::Purple));
        assert_eq!(
            k.action(EKey::Num9),
            Some(CullAction::Label(ColorLabel::Purple))
        );
    }
}
