//! The progress handlers egui-pop ships: `track` (a bar's or a meter's
//! track), `span` (a stretch of a track: a fill, a section) and `tick` (a
//! meter's threshold).

use egui::epaint::CornerRadius;
use egui::{Stroke, Vec2, pos2};

use crate::kit::{
    ComponentKit, Corners, KitCx, Part, Primitive, paint_box, paint_label, paint_ring,
};

/// A box rounded on every corner: a radius of at least half its height
/// gives round ends (the painter keeps a radius within half the box).
pub(crate) struct Track;

impl Primitive<ComponentKit> for Track {
    fn measure(&self, data: &ComponentKit, label: Vec2) -> Vec2 {
        Vec2::new(
            data.width.max(label.x + 2.0 * data.padding_x),
            data.height.max(label.y),
        )
    }

    fn paint(&self, cx: &KitCx<'_>, data: &ComponentKit, part: &Part<'_>) {
        let b = data.states.get(part.state);
        paint_box(cx, part.rect, b, Corners::All, data.indicator_side);
        paint_ring(cx, part.rect, b, Corners::All);
    }

    fn paint_label(&self, cx: &KitCx<'_>, data: &ComponentKit, part: &Part<'_>) {
        paint_label(cx, data, part);
    }
}

/// A stretch of a track (`Part::room` is the track): its ends are rounded
/// where they meet the track's, square inside it. A part whose `room` is
/// its own rectangle is rounded at both ends (a travelling segment).
pub(crate) struct Span;

impl Primitive<ComponentKit> for Span {
    fn measure(&self, data: &ComponentKit, label: Vec2) -> Vec2 {
        Vec2::new(
            data.width.max(label.x + 2.0 * data.padding_x),
            data.height.max(label.y),
        )
    }

    fn paint(&self, cx: &KitCx<'_>, data: &ComponentKit, part: &Part<'_>) {
        let b = data.states.get(part.state);
        if b.fill.a() == 0 {
            return;
        }
        let r = b.radius.round().clamp(0.0, 255.0) as u8;
        let start = part.rect.left() <= part.room.left();
        let end = part.rect.right() >= part.room.right() - 0.5;
        let (l, rr) = (if start { r } else { 0 }, if end { r } else { 0 });
        let corners = CornerRadius {
            nw: l,
            sw: l,
            ne: rr,
            se: rr,
        };
        cx.painter.rect_filled(part.rect, corners, b.fill);
    }

    fn paint_label(&self, cx: &KitCx<'_>, data: &ComponentKit, part: &Part<'_>) {
        paint_label(cx, data, part);
    }
}

/// A line across its part, top to bottom on its centre, the component's
/// `width` thick, in its `fill`.
pub(crate) struct Tick;

impl Primitive<ComponentKit> for Tick {
    fn measure(&self, data: &ComponentKit, label: Vec2) -> Vec2 {
        Vec2::new(data.width, data.height.max(label.y))
    }

    fn paint(&self, cx: &KitCx<'_>, data: &ComponentKit, part: &Part<'_>) {
        let b = data.states.get(part.state);
        if data.width <= 0.0 || b.fill.a() == 0 {
            return;
        }
        let r = part.rect;
        // Across the bar: a line along the rect's longer side (a
        // vertical meter's ticks are horizontal).
        let ends = if r.width() > r.height() {
            [pos2(r.left(), r.center().y), pos2(r.right(), r.center().y)]
        } else {
            [pos2(r.center().x, r.top()), pos2(r.center().x, r.bottom())]
        };
        cx.painter
            .line_segment(ends, Stroke::new(data.width, b.fill));
    }

    fn paint_label(&self, cx: &KitCx<'_>, data: &ComponentKit, part: &Part<'_>) {
        paint_label(cx, data, part);
    }
}
