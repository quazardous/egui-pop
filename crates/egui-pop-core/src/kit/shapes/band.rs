//! The `band` handler: a header band at the top of a frame.

use egui::{Stroke, Vec2};

use crate::kit::{
    ComponentKit, Corners, KitCx, Part, Primitive, paint_indicator, paint_label, paint_ring,
};

/// A band across the top of a frame's content, reaching its edges: its
/// fill with the top corners of the frame (its `radius`) inside the
/// frame's one point outline, its border the line along its bottom
/// edge, its indicator and its ring. A movable dialog's and a palette's
/// header.
pub(crate) struct Band;

impl Primitive<ComponentKit> for Band {
    fn measure(&self, data: &ComponentKit, label: Vec2) -> Vec2 {
        Vec2::new(
            data.width.max(label.x + 2.0 * data.padding_x),
            data.height.max(label.y + 2.0 * data.padding_y),
        )
    }

    fn paint(&self, cx: &KitCx<'_>, data: &ComponentKit, part: &Part<'_>) {
        let b = data.states.get(part.state);
        let rect = part.rect;
        if b.fill.a() > 0 {
            let r = (b.radius - 1.0).clamp(0.0, 255.0) as u8;
            cx.painter.rect_filled(rect, Corners::Top.radius(r), b.fill);
        }
        if b.border_width > 0.0 && b.border_color.a() > 0 {
            let w = crate::scale::snap(b.border_width, cx.ppp);
            cx.painter.hline(
                rect.x_range(),
                rect.bottom(),
                Stroke::new(w, b.border_color),
            );
        }
        paint_indicator(cx, rect, b, data.indicator_side);
        paint_ring(cx, rect, b, Corners::Top);
    }

    fn paint_label(&self, cx: &KitCx<'_>, data: &ComponentKit, part: &Part<'_>) {
        paint_label(cx, data, part);
    }
}
