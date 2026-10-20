//! The `scoped` handler: a part egui draws itself (its combo box's box,
//! a selected button). The kit reaches it through egui's visuals, set in
//! a scope from the component's boxes when the kit is compiled
//! ([`Kit::scope`](crate::kit::Kit::scope)); the handler paints no box.

use egui::Vec2;

use crate::kit::{ComponentKit, KitCx, Part, Primitive, paint_label};

pub(crate) struct Scoped;

impl Primitive<ComponentKit> for Scoped {
    fn measure(&self, data: &ComponentKit, label: Vec2) -> Vec2 {
        Vec2::new(
            data.width.max(label.x + 2.0 * data.padding_x),
            data.height.max(label.y),
        )
    }

    fn paint(&self, _cx: &KitCx<'_>, _data: &ComponentKit, _part: &Part<'_>) {}

    fn paint_label(&self, cx: &KitCx<'_>, data: &ComponentKit, part: &Part<'_>) {
        paint_label(cx, data, part);
    }
}
