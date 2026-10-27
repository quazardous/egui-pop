//! The `section` handler: a collapsible section's title band.

use egui::{Stroke, Vec2};

use crate::kit::{
    ComponentKit, Corners, KitCx, Part, Primitive, paint_indicator, paint_label, paint_ring,
};

/// A band across a panel: its fill (square corners unless the kit gives
/// a radius), its border as a line along its top edge and one along its
/// bottom edge (two bands stacked share one line), its indicator and
/// its ring. A section's title band.
pub(crate) struct SectionBand;

impl Primitive<ComponentKit> for SectionBand {
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
            let r = b.radius.clamp(0.0, 255.0) as u8;
            cx.painter.rect_filled(rect, Corners::All.radius(r), b.fill);
        }
        if b.border_width > 0.0 && b.border_color.a() > 0 {
            let line = Stroke::new(crate::scale::snap(b.border_width, cx.ppp), b.border_color);
            cx.painter.hline(rect.x_range(), rect.top(), line);
            cx.painter.hline(rect.x_range(), rect.bottom(), line);
        }
        paint_indicator(cx, rect, b, data.indicator_side);
        paint_ring(cx, rect, b, Corners::All);
    }

    fn paint_label(&self, cx: &KitCx<'_>, data: &ComponentKit, part: &Part<'_>) {
        paint_label(cx, data, part);
    }
}
