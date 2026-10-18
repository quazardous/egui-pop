//! The `pill` handler: a box with round ends.

use egui::Vec2;

use crate::kit::{
    BoxStyle, ComponentKit, Corners, KitCx, Part, Primitive, paint_box, paint_label, paint_ring,
};

/// A box whose radius is at most half its height, rounded down to a
/// whole point (a large radius gives round ends at any height), with its
/// border, indicator and ring. The library's pills and tag chips.
pub(crate) struct Pill;

/// The box of a part with its radius capped at half the part's height.
fn capped(b: &BoxStyle, height: f32) -> BoxStyle {
    BoxStyle {
        radius: b.radius.min((height / 2.0).floor()),
        ..*b
    }
}

impl Primitive<ComponentKit> for Pill {
    fn measure(&self, data: &ComponentKit, label: Vec2) -> Vec2 {
        Vec2::new(
            data.width.max(label.x + 2.0 * data.padding_x),
            data.height.max(label.y + 2.0 * data.padding_y),
        )
    }

    fn paint(&self, cx: &KitCx<'_>, data: &ComponentKit, part: &Part<'_>) {
        let b = capped(data.states.get(part.state), part.rect.height());
        paint_box(cx, part.rect, &b, Corners::All, data.indicator_side);
        paint_ring(cx, part.rect, &b, Corners::All);
    }

    fn paint_label(&self, cx: &KitCx<'_>, data: &ComponentKit, part: &Part<'_>) {
        paint_label(cx, data, part);
    }
}
