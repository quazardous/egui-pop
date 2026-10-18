//! The `plain` handler, every component's default.

use egui::Vec2;

use crate::kit::{
    BoxStyle, ComponentKit, Corners, KitCx, Part, Primitive, paint_indicator, paint_label,
    paint_ring,
};

/// A box with square corners: its fill, its border on the indicator
/// side only (the line under a bar) and its indicator. Bars and window
/// buttons.
pub(crate) struct Plain;

impl Primitive<ComponentKit> for Plain {
    fn measure(&self, data: &ComponentKit, label: Vec2) -> Vec2 {
        Vec2::new(
            data.width.max(label.x + 2.0 * data.padding_x),
            data.height.max(label.y),
        )
    }

    fn paint_label(&self, cx: &KitCx<'_>, data: &ComponentKit, part: &Part<'_>) {
        paint_label(cx, data, part);
    }

    fn paint(&self, cx: &KitCx<'_>, data: &ComponentKit, part: &Part<'_>) {
        let b = data.states.get(part.state);
        if b.fill.a() > 0 {
            cx.painter.rect_filled(part.rect, 0.0, b.fill);
        }
        if b.border_width > 0.0 && b.border_color.a() > 0 {
            let w = crate::scale::snap(b.border_width, cx.ppp);
            let line = BoxStyle {
                indicator_color: b.border_color,
                indicator_size: w,
                ..BoxStyle::default()
            };
            paint_indicator(cx, part.rect, &line, data.indicator_side);
        }
        paint_indicator(cx, part.rect, b, data.indicator_side);
        paint_ring(cx, part.rect, b, Corners::None);
    }
}
