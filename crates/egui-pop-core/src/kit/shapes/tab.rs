//! The tab handlers egui-pop ships: `box` and `underline`.

use egui::epaint::StrokeKind;
use egui::{Rect, Stroke, Vec2, pos2};

use crate::kit::{
    ComponentKit, Corners, KitCx, Part, Primitive, paint_box, paint_indicator, paint_label,
    paint_ring,
};

/// A tab as a box joined to the panel under it: rounded top corners, a
/// border on three sides, the bottom left open.
pub(crate) struct BoxTab;

impl Primitive<ComponentKit> for BoxTab {
    fn measure(&self, data: &ComponentKit, label: Vec2) -> Vec2 {
        Vec2::new(label.x + 2.0 * data.padding_x, data.height.max(label.y))
    }

    fn paint(&self, cx: &KitCx<'_>, data: &ComponentKit, part: &Part<'_>) {
        let b = data.states.get(part.state);
        let rect = part.rect;
        let r = Corners::Top.radius(b.radius.round().clamp(0.0, 255.0) as u8);
        if b.fill.a() > 0 {
            cx.painter.rect_filled(rect, r, b.fill);
        }
        if b.border_width > 0.0 && b.border_color.a() > 0 {
            // The outline of a box one line taller, cut at the tab's
            // bottom: three sides drawn, the bottom left open. egui's
            // rectangle, not a path: nothing computed per frame.
            let w = crate::scale::snap(b.border_width, cx.ppp);
            let taller = Rect::from_min_max(rect.min, pos2(rect.max.x, rect.max.y + w));
            cx.painter
                .with_clip_rect(rect.intersect(cx.painter.clip_rect()))
                .rect_stroke(
                    taller,
                    r,
                    Stroke::new(w, b.border_color),
                    StrokeKind::Inside,
                );
        }
        paint_indicator(cx, rect, b, data.indicator_side);
        paint_ring(cx, rect, b, Corners::Top);
    }

    fn paint_label(&self, cx: &KitCx<'_>, data: &ComponentKit, part: &Part<'_>) {
        paint_label(cx, data, part);
    }
}

/// A tab as its label over an indicator bar (usually on the selected tab
/// only), its box rounded all around.
pub(crate) struct UnderlineTab;

impl Primitive<ComponentKit> for UnderlineTab {
    fn measure(&self, data: &ComponentKit, label: Vec2) -> Vec2 {
        Vec2::new(label.x + 2.0 * data.padding_x, data.height.max(label.y))
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
