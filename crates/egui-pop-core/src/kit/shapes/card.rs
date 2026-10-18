//! The `card` handler: a box rounded all around, the frames' default.

use std::sync::{Arc, LazyLock};

use egui::epaint::StrokeKind;
use egui::{Stroke, Vec2};

use crate::kit::{
    ComponentKit, Corners, KitCx, Part, Primitive, paint_indicator, paint_label, paint_ring,
};

/// A box with all its corners rounded by the radius, its border all
/// around (one rectangle, fill and border), its indicator and its focus
/// ring: dialogs, menus, cards, the frame of an editor. Around content
/// egui lays out, [`crate::kit::Component::show`] draws it as egui's own
/// frame, which is the same box.
pub(crate) struct Card;

/// The one `card` handler, so a kit knows it ([`crate::kit::Component::show`]
/// lets egui draw it).
pub(crate) static CARD: LazyLock<Arc<dyn Primitive<ComponentKit>>> =
    LazyLock::new(|| Arc::new(Card));

impl Primitive<ComponentKit> for Card {
    fn measure(&self, data: &ComponentKit, label: Vec2) -> Vec2 {
        Vec2::new(
            data.width.max(label.x + 2.0 * data.padding_x),
            data.height.max(label.y + 2.0 * data.padding_y),
        )
    }

    fn paint(&self, cx: &KitCx<'_>, data: &ComponentKit, part: &Part<'_>) {
        let b = data.states.get(part.state);
        let r = Corners::All.radius(b.radius.round().clamp(0.0, 255.0) as u8);
        let stroke = if b.border_width > 0.0 && b.border_color.a() > 0 {
            Stroke::new(crate::scale::snap(b.border_width, cx.ppp), b.border_color)
        } else {
            Stroke::NONE
        };
        if b.fill.a() > 0 || stroke != Stroke::NONE {
            cx.painter
                .rect(part.rect, r, b.fill, stroke, StrokeKind::Inside);
        }
        paint_indicator(cx, part.rect, b, data.indicator_side);
        paint_ring(cx, part.rect, b, Corners::All);
    }

    fn paint_label(&self, cx: &KitCx<'_>, data: &ComponentKit, part: &Part<'_>) {
        paint_label(cx, data, part);
    }
}
