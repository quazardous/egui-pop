//! The `box` handler of cells and rows: a free-standing box.

use egui::Vec2;

use crate::kit::{
    ComponentKit, Corners, KitCx, Part, Primitive, paint_box, paint_label, paint_ring,
};

/// A box on its own: its fill (at the component's `opacity`), its border
/// all around, every corner rounded by its radius, its indicator and its
/// ring. Rows of the data views, a table's header cells, buttons.
pub(crate) struct BoxCell;

impl Primitive<ComponentKit> for BoxCell {
    fn measure(&self, data: &ComponentKit, label: Vec2) -> Vec2 {
        Vec2::new(
            data.width.max(label.x + 2.0 * data.padding_x),
            data.height.max(label.y),
        )
    }

    fn paint(&self, cx: &KitCx<'_>, data: &ComponentKit, part: &Part<'_>) {
        let mut b = *data.states.get(part.state);
        if data.opacity < 1.0 {
            b.fill = b.fill.gamma_multiply(data.opacity);
        }
        paint_box(cx, part.rect, &b, Corners::All, data.indicator_side);
        paint_ring(cx, part.rect, &b, Corners::All);
    }

    fn paint_label(&self, cx: &KitCx<'_>, data: &ComponentKit, part: &Part<'_>) {
        paint_label(cx, data, part);
    }
}
