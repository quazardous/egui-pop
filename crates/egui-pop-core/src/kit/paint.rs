//! What kit handlers paint with: boxes, indicators, focus rings and
//! labels, on whole physical pixels.

use egui::epaint::{CornerRadius, StrokeKind};
use egui::{Rect, Stroke, Vec2, pos2};

use super::{Align, BoxStyle, ComponentKit, KitCx, Part, Side};

/// Paints a box: its fill and border with `corners` rounded by its
/// radius, and its indicator on `side`. Strokes are whole physical
/// pixels.
pub fn paint_box(cx: &KitCx<'_>, rect: Rect, b: &BoxStyle, corners: Corners, side: Side) {
    let r = b.radius.round().clamp(0.0, 255.0) as u8;
    let radius = corners.radius(r);
    if b.fill.a() > 0 {
        cx.painter.rect_filled(rect, radius, b.fill);
    }
    if b.border_width > 0.0 && b.border_color.a() > 0 {
        let w = crate::scale::snap(b.border_width, cx.ppp);
        cx.painter.rect_stroke(
            rect,
            radius,
            Stroke::new(w, b.border_color),
            StrokeKind::Inside,
        );
    }
    paint_indicator(cx, rect, b, side);
}

pub fn paint_indicator(cx: &KitCx<'_>, rect: Rect, b: &BoxStyle, side: Side) {
    if b.indicator_size <= 0.0 || b.indicator_color.a() == 0 {
        return;
    }
    let s = crate::scale::snap_size(b.indicator_size, cx.ppp);
    let bar = match side {
        Side::Top => Rect::from_min_max(rect.min, pos2(rect.max.x, rect.min.y + s)),
        Side::Bottom => Rect::from_min_max(pos2(rect.min.x, rect.max.y - s), rect.max),
        Side::Left => Rect::from_min_max(rect.min, pos2(rect.min.x + s, rect.max.y)),
        Side::Right => Rect::from_min_max(pos2(rect.max.x - s, rect.min.y), rect.max),
    };
    cx.painter.rect_filled(bar, 0.0, b.indicator_color);
}

/// Paints a box's focus ring, if its state gives one: an outline
/// `ring_offset` outside the box, following its corners. Every handler
/// calls it last, so the ring is above the box and its label.
pub fn paint_ring(cx: &KitCx<'_>, rect: Rect, b: &BoxStyle, corners: Corners) {
    if b.ring_width <= 0.0 || b.ring_color.a() == 0 {
        return;
    }
    let w = crate::scale::snap(b.ring_width, cx.ppp);
    let r = (b.radius + b.ring_offset.max(0.0))
        .round()
        .clamp(0.0, 255.0) as u8;
    cx.painter.rect_stroke(
        rect.expand(b.ring_offset),
        corners.radius(r),
        Stroke::new(w, b.ring_color),
        StrokeKind::Outside,
    );
}

/// Which corners of a box are rounded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Corners {
    All,
    Top,
    None,
}

impl Corners {
    pub fn radius(self, r: u8) -> CornerRadius {
        match self {
            Corners::All => CornerRadius::same(r),
            Corners::Top => CornerRadius {
                nw: r,
                ne: r,
                sw: 0,
                se: 0,
            },
            Corners::None => CornerRadius::ZERO,
        }
    }
}

/// Where a label of `size` goes in a part: along it by the component's
/// `align`, inside its room less the padding, centred across; on whole
/// physical pixels is the painter's business.
pub fn label_rect(data: &ComponentKit, part: &Part<'_>, size: Vec2) -> Rect {
    let inner = part.room.shrink2(Vec2::new(data.padding_x, 0.0));
    let x = match data.align {
        Align::Start => inner.left(),
        Align::End => inner.right() - size.x,
        Align::Centre => (part.rect.center().x - size.x / 2.0)
            .clamp(inner.left(), (inner.right() - size.x).max(inner.left())),
    };
    let y = part.rect.center().y - size.y / 2.0;
    Rect::from_min_size(pos2(x, y), size)
}

/// Paints a part's label where [`label_rect`] puts it, in its state's
/// `text`, on whole physical pixels. What handlers do unless they draw
/// labels their own way.
pub fn paint_label(cx: &KitCx<'_>, data: &ComponentKit, part: &Part<'_>) {
    let Some(galley) = part.label else { return };
    let b = data.states.get(part.state);
    let at = label_rect(data, part, galley.size()).min;
    let pos = pos2(
        (at.x * cx.ppp).round() / cx.ppp,
        (at.y * cx.ppp).round() / cx.ppp,
    );
    cx.painter.galley(pos, galley.clone(), b.text);
}
