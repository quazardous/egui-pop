//! The `star` handler: a five-pointed star, the rating stars' mark.

use std::f32::consts::PI;

use egui::{Pos2, Shape, Stroke, Vec2, vec2};

use crate::kit::{ComponentKit, KitCx, Part, Primitive};

/// A five-pointed star centred in the part, its points on a circle of
/// 45 % of the part's smaller side: filled in `fill` (unless
/// transparent), outlined in `border-color` (unless `border-width` is 0).
/// An empty star is its outline alone, a given one its fill.
pub(crate) struct Star;

impl Primitive<ComponentKit> for Star {
    fn measure(&self, data: &ComponentKit, label: Vec2) -> Vec2 {
        Vec2::new(data.width.max(label.x), data.height.max(label.y))
    }

    fn paint(&self, cx: &KitCx<'_>, data: &ComponentKit, part: &Part<'_>) {
        let b = data.states.get(part.state);
        let radius = part.rect.width().min(part.rect.height()) * 0.45;
        if radius <= 0.0 {
            return;
        }
        let p = points(part.rect.center(), radius);
        if b.fill.a() > 0 {
            // Convex pieces: the pentagon inside and a triangle per point.
            let inner: Vec<Pos2> = p.iter().skip(1).step_by(2).copied().collect();
            cx.painter
                .add(Shape::convex_polygon(inner, b.fill, Stroke::NONE));
            for k in (0..10).step_by(2) {
                let tip = vec![p[(k + 9) % 10], p[k], p[k + 1]];
                cx.painter
                    .add(Shape::convex_polygon(tip, b.fill, Stroke::NONE));
            }
        }
        if b.border_width > 0.0 && b.border_color.a() > 0 {
            let w = crate::scale::snap(b.border_width, cx.ppp);
            cx.painter
                .add(Shape::closed_line(p, Stroke::new(w, b.border_color)));
        }
    }

    fn paint_label(&self, _: &KitCx<'_>, _: &ComponentKit, _: &Part<'_>) {}
}

/// The ten points of a five-pointed star, outer first, from the top,
/// clockwise.
fn points(center: Pos2, outer: f32) -> Vec<Pos2> {
    let inner = outer * 0.42;
    (0..10usize)
        .map(|k| {
            let a = -PI / 2.0 + k as f32 * PI / 5.0;
            let r = if k.is_multiple_of(2) { outer } else { inner };
            center + vec2(a.cos(), a.sin()) * r
        })
        .collect()
}
