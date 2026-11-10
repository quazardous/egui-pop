//! Rating stars: zero to five stars, edited with the pointer (and the
//! keyboard when focusable) or only shown. One widget for every place a
//! rating appears: a thumbnail's overlay, a panel, a table's column, a
//! file's facts. Its behaviour follows the photo applications it is
//! modelled on (`docs/rating.md`):
//!
//! - **empty stars** are outlines, shown always or only while the item
//!   they rate (or the stars) is under the pointer ([`EmptyStars`]), so
//!   an unrated item still says it can be rated;
//! - **under the pointer** the stars up to the pointer light in a
//!   preview look (`hover`), the rest stay outlines; the current value
//!   past the preview stays visible as a ghost (`selected` in the
//!   `muted` tone);
//! - **a click** gives the star's value; on the star of the current
//!   value it clears the rating (its preview: every star a ghost). Each
//!   star's tooltip and accessible name say it: "Set 3 of 5", "Clear
//!   rating";
//! - **read-only**, the stars are drawn by `rating-star-display` and
//!   read as "3 of 5".
//!
//! The application owns the value: the widget takes the current one and
//! says what the user asked for ([`RatingResponse::changed`]).
//!
//! ```
//! use egui_pop_core::rating::{EmptyStars, RatingStars};
//!
//! fn ui(ui: &mut egui::Ui, rating: &mut u8) {
//!     if let Some(r) = RatingStars::new(*rating).show(ui).changed {
//!         *rating = r;
//!     }
//!     // In a table's cell: shown, not edited.
//!     let _ = RatingStars::new(*rating).read_only(true).show(ui);
//!     // Over a thumbnail: outlines only while the cell is hovered.
//!     let _ = RatingStars::new(*rating).empty(EmptyStars::OnHover);
//! }
//! ```

use egui::{Id, Key as EKey, Rect, Response, Role, Sense, Ui, WidgetInfo, vec2};

use crate::kit::{Kit, KitCx, Part, State, Tone};
use crate::strings::{self, tr};
use crate::theme::Tokens;

/// The highest rating.
pub const MAX_RATING: u8 = 5;

/// When empty stars (outlines) show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum EmptyStars {
    /// Always: a panel, a row or a cell with room for them.
    #[default]
    Always,
    /// While the item they rate is hovered ([`RatingStars::item_hovered`])
    /// or the pointer is on the stars: a thumbnail's overlay.
    OnHover,
}

/// What a star shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StarLook {
    /// Past the value (and the preview): an outline, the kit's `rest`.
    Empty,
    /// A star of the value: `selected`.
    Given,
    /// A star of the value under the pointer: `hover`.
    Preview,
    /// A star of the current value past the preview: `selected` in the
    /// `muted` tone.
    Ghost,
}

impl StarLook {
    /// The look of star `star` (1 to 5) for a value and, while the
    /// pointer is on the stars, the value a click would give.
    pub fn of(star: u8, value: u8, preview: Option<u8>) -> StarLook {
        match preview {
            Some(p) if star <= p => StarLook::Preview,
            Some(_) if star <= value => StarLook::Ghost,
            None if star <= value => StarLook::Given,
            _ => StarLook::Empty,
        }
    }

    /// The kit state it is painted in.
    pub fn state(self) -> State {
        match self {
            StarLook::Empty => State::REST,
            StarLook::Given => State::SELECTED,
            StarLook::Preview => State::HOVER,
            StarLook::Ghost => State::SELECTED.toned(Tone::Muted),
        }
    }
}

/// What a click on star `star` (1 to 5) gives: its value, or 0 on the
/// star of the current value (it clears the rating).
pub fn click_value(star: u8, value: u8) -> u8 {
    if star == value { 0 } else { star }
}

/// What [`RatingStars`] returns.
#[derive(Debug)]
pub struct RatingResponse {
    /// The stars' row as a whole.
    pub response: Response,
    /// The value the user asked for, if any (never the current one).
    pub changed: Option<u8>,
    /// The value a click would give, while the pointer is on the stars.
    pub preview: Option<u8>,
    /// Whether the pointer is on the stars.
    pub hovered: bool,
}

/// Five stars (see the [module](self)).
#[derive(Clone, Debug)]
#[must_use = "show it with `show` or `show_at`"]
pub struct RatingStars {
    value: u8,
    label: Option<String>,
    size: Option<f32>,
    read_only: bool,
    empty: EmptyStars,
    item_hovered: bool,
    focusable: bool,
    id: Option<Id>,
}

impl RatingStars {
    pub fn new(value: u8) -> RatingStars {
        RatingStars {
            value: value.min(MAX_RATING),
            label: None,
            size: None,
            read_only: false,
            empty: EmptyStars::Always,
            item_hovered: false,
            focusable: true,
            id: None,
        }
    }

    /// Its accessible name ("Rating" by default); a read-only rating is
    /// named by its value ("3 of 5").
    pub fn label(mut self, label: impl Into<String>) -> RatingStars {
        self.label = Some(label.into());
        self
    }

    /// One star's size in points (default: the inline icon size, at
    /// least 12, plus 4).
    pub fn size(mut self, size: f32) -> RatingStars {
        self.size = Some(size);
        self
    }

    /// Shown, not edited: no preview, no click, no focus, drawn by the
    /// kit's `rating-star-display`. A disabled `Ui` gives the same.
    pub fn read_only(mut self, read_only: bool) -> RatingStars {
        self.read_only = read_only;
        self
    }

    /// When empty stars show ([`EmptyStars::Always`] by default).
    pub fn empty(mut self, empty: EmptyStars) -> RatingStars {
        self.empty = empty;
        self
    }

    /// Whether the item the stars rate is under the pointer (a
    /// thumbnail's cell), for [`EmptyStars::OnHover`].
    pub fn item_hovered(mut self, hovered: bool) -> RatingStars {
        self.item_hovered = hovered;
        self
    }

    /// Whether the stars take the keyboard focus (true by default): 0 to
    /// 5 set the rating, the arrows step it, Home and End go to the ends.
    /// A view with its own keys (a thumbnail grid) turns it off.
    pub fn focusable(mut self, focusable: bool) -> RatingStars {
        self.focusable = focusable;
        self
    }

    /// The id of the stars' row, for [`RatingStars::show_at`] (default:
    /// the `Ui`'s next automatic id).
    pub fn id(mut self, id: Id) -> RatingStars {
        self.id = Some(id);
        self
    }

    /// The size of five stars of `size` points.
    pub fn size_for(size: f32) -> egui::Vec2 {
        vec2(size * f32::from(MAX_RATING), size)
    }

    fn star_size(&self, ui: &Ui) -> f32 {
        self.size
            .unwrap_or_else(|| Tokens::of(ui.ctx()).size.icon_inline.max(12.0) + 4.0)
    }

    fn editable(&self, ui: &Ui) -> bool {
        !self.read_only && ui.is_enabled()
    }

    fn row_sense(&self, ui: &Ui) -> Sense {
        if self.editable(ui) && self.focusable {
            Sense::FOCUSABLE
        } else {
            Sense::hover()
        }
    }

    /// Lays the stars out in `ui`, one star's size high.
    pub fn show(self, ui: &mut Ui) -> RatingResponse {
        let size = Self::size_for(self.star_size(ui));
        let (rect, response) = ui.allocate_exact_size(size, self.row_sense(ui));
        self.run(ui, rect, response)
    }

    /// Puts the stars in `rect` (laid out by the caller: an overlay),
    /// five equal slots across it.
    pub fn show_at(self, ui: &mut Ui, rect: Rect) -> RatingResponse {
        let id = self.id.unwrap_or_else(|| ui.next_auto_id());
        let response = ui.interact(rect, id, self.row_sense(ui));
        self.run(ui, rect, response)
    }

    fn run(self, ui: &mut Ui, rect: Rect, response: Response) -> RatingResponse {
        let editable = self.editable(ui);
        let focusable = editable && self.focusable;
        let slot = rect.width() / f32::from(MAX_RATING);
        let star_rect = |i: u8| {
            Rect::from_min_size(
                rect.min + vec2(slot * f32::from(i), 0.0),
                vec2(slot, rect.height()),
            )
        };
        let hovered = ui.rect_contains_pointer(rect);
        let mut changed = None;
        let mut preview = None;
        if editable {
            // The star under the pointer, gaps and edges included: the
            // preview never flickers between two stars.
            if hovered && let Some(p) = ui.ctx().pointer_hover_pos() {
                let i = (((p.x - rect.left()) / slot).floor() as i32).clamp(0, 4) as u8;
                preview = Some(click_value(i + 1, self.value));
            }
            for i in 0..MAX_RATING {
                let star = i + 1;
                let gives = click_value(star, self.value);
                let r = ui.interact(star_rect(i), response.id.with(star), Sense::CLICK);
                let name = if gives == 0 {
                    tr(ui.ctx(), &strings::RATING_CLEAR)
                } else {
                    strings::tr_with(ui.ctx(), &strings::RATING_SET, &[("stars", star.into())])
                };
                let tip = name.clone();
                r.widget_info(move || WidgetInfo::labeled(Role::Button, true, &name));
                let r = r.on_hover_text(tip);
                if r.clicked() {
                    changed = Some(gives);
                    if focusable {
                        r.ctx.memory_mut(|m| m.request_focus(response.id));
                    }
                }
            }
        }
        if focusable {
            crate::focus::takes_keys(ui.ctx(), response.id);
        }
        if focusable && response.has_focus() {
            changed = changed.or_else(|| self.keys(ui));
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
            let kit = Kit::of(ui.ctx());
            let component = if editable {
                &kit.rating_star
            } else {
                &kit.rating_star_display
            };
            let painter = ui.painter();
            let cx = KitCx {
                painter,
                ppp: ui.ctx().pixels_per_point(),
            };
            let empty_shown = match self.empty {
                EmptyStars::Always => true,
                EmptyStars::OnHover => self.item_hovered || hovered,
            };
            for i in 0..MAX_RATING {
                let look = StarLook::of(i + 1, self.value, preview);
                if look == StarLook::Empty && !empty_shown {
                    continue;
                }
                let r = star_rect(i);
                component.paint(
                    &cx,
                    &Part {
                        rect: r,
                        room: r,
                        state: look.state(),
                        label: None,
                    },
                );
            }
            if focusable && response.has_focus() {
                let t = Tokens::of(ui.ctx());
                crate::focus::own_ring(ui.ctx(), response.id);
                painter.rect_stroke(
                    rect.expand(1.0),
                    t.radius.widget,
                    egui::Stroke::new(1.0, t.focus_ring),
                    egui::StrokeKind::Outside,
                );
            }
        }
        let value = self.value;
        if focusable {
            let label = self.label.unwrap_or_else(|| tr(ui.ctx(), &strings::RATING));
            response.widget_info(move || WidgetInfo::slider(true, f64::from(value), &label));
            ui.ctx().accesskit_node_builder(response.id, |n| {
                n.set_min_numeric_value(0.0);
                n.set_max_numeric_value(f64::from(MAX_RATING));
                n.set_numeric_value_step(1.0);
            });
        } else if !editable {
            let said =
                strings::tr_with(ui.ctx(), &strings::RATING_VALUE, &[("stars", value.into())]);
            response.widget_info(move || WidgetInfo::labeled(Role::Label, true, &said));
        }
        RatingResponse {
            response,
            changed,
            preview,
            hovered,
        }
    }

    /// The value the keys pressed this pass ask for.
    fn keys(&self, ui: &Ui) -> Option<u8> {
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lower_preview_keeps_the_current_value_as_a_ghost() {
        let looks: Vec<StarLook> = (1..=5).map(|s| StarLook::of(s, 3, Some(1))).collect();
        assert_eq!(
            looks,
            [
                StarLook::Preview,
                StarLook::Ghost,
                StarLook::Ghost,
                StarLook::Empty,
                StarLook::Empty
            ]
        );
    }

    #[test]
    fn the_current_star_clears_and_its_preview_is_all_ghosts() {
        assert_eq!(click_value(3, 3), 0);
        assert_eq!(click_value(4, 3), 4);
        assert_eq!(click_value(1, 0), 1);
        let looks: Vec<StarLook> = (1..=5).map(|s| StarLook::of(s, 3, Some(0))).collect();
        assert_eq!(&looks[..3], [StarLook::Ghost; 3]);
        assert_eq!(&looks[3..], [StarLook::Empty; 2]);
        assert_eq!(StarLook::of(2, 3, None), StarLook::Given);
        assert_eq!(StarLook::of(5, 3, Some(5)), StarLook::Preview);
    }
}
