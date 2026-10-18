//! The `traffic-light` handler for the window buttons (macOS flavour):
//! a circle in the state's `fill`, outlined in its `border-color`. The
//! title bar draws the glyph over it in the state's `text`; a kit whose
//! resting `text` is transparent shows glyphs only on hover.

use egui::{Stroke, Vec2};

use crate::kit::{ComponentKit, KitCx, Part, Primitive, paint_label};

/// A circle centred in its part: `height` across when the kit gives one,
/// else half the part's shorter side.
pub(crate) struct TrafficLight;

/// The circle's diameter in a part of `size`.
fn diameter(data: &ComponentKit, size: Vec2) -> f32 {
    let room = size.x.min(size.y);
    if data.height > 0.0 {
        data.height.min(room)
    } else {
        room / 2.0
    }
}

impl Primitive<ComponentKit> for TrafficLight {
    fn measure(&self, data: &ComponentKit, label: Vec2) -> Vec2 {
        Vec2::new(
            data.width.max(data.height).max(label.x),
            data.height.max(label.y),
        )
    }

    fn paint(&self, cx: &KitCx<'_>, data: &ComponentKit, part: &Part<'_>) {
        let b = data.states.get(part.state);
        let r = diameter(data, part.rect.size()) / 2.0;
        let c = part.rect.center();
        if b.fill.a() > 0 {
            cx.painter.circle_filled(c, r, b.fill);
        }
        if b.border_width > 0.0 && b.border_color.a() > 0 {
            let w = crate::scale::snap(b.border_width, cx.ppp);
            // Inside the circle, as a box's border is inside the box.
            cx.painter
                .circle_stroke(c, r - w / 2.0, Stroke::new(w, b.border_color));
        }
        if b.ring_width > 0.0 && b.ring_color.a() > 0 {
            let w = crate::scale::snap(b.ring_width, cx.ppp);
            cx.painter
                .circle_stroke(c, r + b.ring_offset + w / 2.0, Stroke::new(w, b.ring_color));
        }
    }

    fn paint_label(&self, cx: &KitCx<'_>, data: &ComponentKit, part: &Part<'_>) {
        paint_label(cx, data, part);
    }
}
