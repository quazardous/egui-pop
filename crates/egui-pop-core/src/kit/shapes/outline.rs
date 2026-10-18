//! The `outline` handler: a marker drawn by its border only.

use egui::epaint::{CornerRadius, StrokeKind};
use egui::{Stroke, Vec2};

use crate::kit::{ComponentKit, KitCx, Part, Primitive};

/// The border of a part, nothing else: inside its rectangle, with its
/// radius, or along it when the part has no height (a line between two
/// rows, centred on it). A tree's drop marker.
pub(crate) struct Outline;

impl Primitive<ComponentKit> for Outline {
    fn measure(&self, data: &ComponentKit, label: Vec2) -> Vec2 {
        Vec2::new(data.width.max(label.x), data.height.max(label.y))
    }

    fn paint(&self, cx: &KitCx<'_>, data: &ComponentKit, part: &Part<'_>) {
        let b = data.states.get(part.state);
        if b.border_width <= 0.0 || b.border_color.a() == 0 {
            return;
        }
        let stroke = Stroke::new(crate::scale::snap(b.border_width, cx.ppp), b.border_color);
        let rect = part.rect;
        if rect.height() <= 0.0 {
            cx.painter.hline(rect.x_range(), rect.top(), stroke);
        } else {
            let r = b.radius.round().clamp(0.0, 255.0) as u8;
            cx.painter
                .rect_stroke(rect, CornerRadius::same(r), stroke, StrokeKind::Inside);
        }
    }

    fn paint_label(&self, _: &KitCx<'_>, _: &ComponentKit, _: &Part<'_>) {}
}
