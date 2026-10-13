//! Overlays in image coordinates, edited through one handle kit.
//!
//! The application owns its overlays (shapes, guides, a crop box) and
//! gives them to the viewport each frame; the viewport draws them and
//! turns drags on their handles into [`Signal::Handle`](crate::Signal)s
//! (begin, update, end, or cancel on Escape), which the application
//! applies to its own data ([`Overlay::apply`] does it for the standard
//! shapes). A `before_handle` hook may veto a move or snap it.
//!
//! The same handle kinds look and behave the same everywhere: a body
//! (move), points and corners, edges, a size, a rotation, a feather.
//! Their hit target is [`HIT_RADIUS`] points whatever their drawn size.

use egui::{Color32, Painter, Pos2, Stroke, StrokeKind, pos2, vec2};

use crate::tokens::ViewportTokens;
use crate::view::Mapping;

/// The radius of a handle's hit target, in points (24-point targets).
pub const HIT_RADIUS: f32 = 12.0;

/// The radius a handle is drawn with, in points.
pub const DRAWN_RADIUS: f32 = 5.0;

/// A shape's line, its dark outline under it, a handle's outline, in
/// points before snapping: each is drawn as whole physical pixels at the
/// window's scale (`scale::stroke`), so it never blurs at 125 % or 150 %.
pub const LINE_WIDTH: f32 = 1.5;
pub const UNDER_WIDTH: f32 = 3.0;
pub const OUTLINE_WIDTH: f32 = 1.5;

/// A width of `w` points, snapped to whole pixels where `painter` draws.
fn snapped(painter: &Painter, w: f32) -> f32 {
    egui_pop_core::scale::stroke(painter.ctx(), w)
}

/// What a handle adjusts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HandleKind {
    /// The whole shape (its body).
    Move,
    /// One point of a line or a path.
    Point,
    /// A corner of a box.
    Corner,
    /// An edge of a box.
    Edge,
    /// A size (a radius).
    Size,
    /// A rotation.
    Rotate,
    /// A feather (a soft edge).
    Feather,
}

impl HandleKind {
    pub fn name(self) -> &'static str {
        match self {
            HandleKind::Move => "move",
            HandleKind::Point => "point",
            HandleKind::Corner => "corner",
            HandleKind::Edge => "edge",
            HandleKind::Size => "size",
            HandleKind::Rotate => "rotate",
            HandleKind::Feather => "feather",
        }
    }
}

/// A handle: an index within its overlay, a kind, a place (image
/// pixels).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Handle {
    pub index: u32,
    pub kind: HandleKind,
    pub at: [f64; 2],
}

/// A shape in image pixels.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Point([f64; 2]),
    Line([f64; 2], [f64; 2]),
    /// `[x0, y0, x1, y1]`.
    Rect([f64; 4]),
    Circle {
        center: [f64; 2],
        radius: f64,
    },
    Path {
        points: Vec<[f64; 2]>,
        closed: bool,
    },
}

impl Shape {
    /// The standard handles of the shape.
    pub fn handles(&self) -> Vec<Handle> {
        let h = |index, kind, at| Handle { index, kind, at };
        match self {
            Shape::Point(p) => vec![h(0, HandleKind::Move, *p)],
            Shape::Line(a, b) => vec![
                h(0, HandleKind::Point, *a),
                h(1, HandleKind::Point, *b),
                h(2, HandleKind::Move, mid(*a, *b)),
            ],
            Shape::Rect(r) => {
                let [x0, y0, x1, y1] = *r;
                let (mx, my) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
                vec![
                    h(0, HandleKind::Corner, [x0, y0]),
                    h(1, HandleKind::Corner, [x1, y0]),
                    h(2, HandleKind::Corner, [x1, y1]),
                    h(3, HandleKind::Corner, [x0, y1]),
                    h(4, HandleKind::Edge, [mx, y0]),
                    h(5, HandleKind::Edge, [x1, my]),
                    h(6, HandleKind::Edge, [mx, y1]),
                    h(7, HandleKind::Edge, [x0, my]),
                    h(8, HandleKind::Move, [mx, my]),
                ]
            }
            Shape::Circle { center, radius } => vec![
                h(0, HandleKind::Move, *center),
                h(1, HandleKind::Size, [center[0] + radius, center[1]]),
            ],
            Shape::Path { points, .. } => {
                let mut v: Vec<Handle> = points
                    .iter()
                    .enumerate()
                    .map(|(i, p)| h(i as u32, HandleKind::Point, *p))
                    .collect();
                if !points.is_empty() {
                    let n = points.len() as f64;
                    let c = points
                        .iter()
                        .fold([0.0, 0.0], |a, p| [a[0] + p[0] / n, a[1] + p[1] / n]);
                    v.push(h(points.len() as u32, HandleKind::Move, c));
                }
                v
            }
        }
    }

    /// The shape after handle `index` moved from `from` to `to` (image
    /// pixels), starting from this shape (the one at the gesture's
    /// start).
    pub fn moved(&self, index: u32, from: [f64; 2], to: [f64; 2]) -> Shape {
        let d = [to[0] - from[0], to[1] - from[1]];
        let shift = |p: [f64; 2]| [p[0] + d[0], p[1] + d[1]];
        match self {
            Shape::Point(p) => Shape::Point(shift(*p)),
            Shape::Line(a, b) => match index {
                0 => Shape::Line(to, *b),
                1 => Shape::Line(*a, to),
                _ => Shape::Line(shift(*a), shift(*b)),
            },
            Shape::Rect(r) => {
                let [mut x0, mut y0, mut x1, mut y1] = *r;
                match index {
                    0 => (x0, y0) = (to[0], to[1]),
                    1 => (x1, y0) = (to[0], to[1]),
                    2 => (x1, y1) = (to[0], to[1]),
                    3 => (x0, y1) = (to[0], to[1]),
                    4 => y0 = to[1],
                    5 => x1 = to[0],
                    6 => y1 = to[1],
                    7 => x0 = to[0],
                    _ => {
                        x0 += d[0];
                        x1 += d[0];
                        y0 += d[1];
                        y1 += d[1];
                    }
                }
                Shape::Rect([x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1)])
            }
            Shape::Circle { center, radius } => match index {
                1 => Shape::Circle {
                    center: *center,
                    radius: ((to[0] - center[0]).powi(2) + (to[1] - center[1]).powi(2)).sqrt(),
                },
                _ => Shape::Circle {
                    center: shift(*center),
                    radius: *radius,
                },
            },
            Shape::Path { points, closed } => {
                let mut points = points.clone();
                if let Some(p) = points.get_mut(index as usize) {
                    *p = to;
                } else {
                    for p in &mut points {
                        *p = shift(*p);
                    }
                }
                Shape::Path {
                    points,
                    closed: *closed,
                }
            }
        }
    }
}

fn mid(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]
}

/// An overlay: a shape with an id, its handles, and whether it is a
/// guide (drawn, never edited).
#[derive(Clone, Debug, PartialEq)]
pub struct Overlay {
    pub id: String,
    pub shape: Shape,
    pub handles: Vec<Handle>,
    pub label: Option<String>,
}

impl Overlay {
    /// An editable overlay with the shape's standard handles.
    pub fn new(id: impl Into<String>, shape: Shape) -> Overlay {
        Overlay {
            id: id.into(),
            handles: shape.handles(),
            shape,
            label: None,
        }
    }

    /// A guide: drawn, no handles.
    pub fn guide(id: impl Into<String>, shape: Shape) -> Overlay {
        Overlay {
            id: id.into(),
            shape,
            handles: Vec::new(),
            label: None,
        }
    }

    /// With a label drawn next to it.
    pub fn label(mut self, label: impl Into<String>) -> Overlay {
        self.label = Some(label.into());
        self
    }

    /// Applies a handle signal for this overlay to its shape: `start` is
    /// the shape when the gesture began (kept by the caller), so a
    /// cancel puts it back. Returns whether the shape changed.
    pub fn apply(&mut self, start: &Shape, signal: &crate::Signal) -> bool {
        let crate::Signal::Handle {
            overlay,
            handle,
            phase,
            from,
            at,
            ..
        } = signal
        else {
            return false;
        };
        if *overlay != self.id {
            return false;
        }
        let shape = match phase {
            crate::Phase::Cancel => start.clone(),
            crate::Phase::Begin => return false,
            _ => start.moved(*handle, *from, *at),
        };
        let changed = shape != self.shape;
        self.handles = shape.handles();
        self.shape = shape;
        changed
    }
}

/// How a handle is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandleState {
    Rest,
    Hover,
    Active,
}

/// Draws one handle of the kit at `pos` (points).
pub fn paint_handle(
    painter: &Painter,
    pos: Pos2,
    kind: HandleKind,
    state: HandleState,
    t: &ViewportTokens,
) {
    let fill = match state {
        HandleState::Rest => t.handle,
        HandleState::Hover | HandleState::Active => t.handle_hover,
    };
    let r = if state == HandleState::Rest {
        DRAWN_RADIUS
    } else {
        DRAWN_RADIUS + 1.5
    };
    let w = |x: f32| snapped(painter, x);
    let outline = Stroke::new(w(OUTLINE_WIDTH), t.handle_outline);
    match kind {
        HandleKind::Move => {
            painter.circle(pos, r, fill, outline);
            painter.circle_filled(pos, r * 0.35, t.handle_outline);
        }
        HandleKind::Corner | HandleKind::Edge => {
            let rect = egui::Rect::from_center_size(pos, vec2(r * 1.8, r * 1.8));
            painter.rect(rect, 0.0, fill, outline, StrokeKind::Middle);
        }
        HandleKind::Rotate => {
            painter.circle(pos, r, Color32::TRANSPARENT, Stroke::new(w(2.5), fill));
            painter.circle_stroke(pos, r + 1.5, outline);
        }
        HandleKind::Feather => {
            painter.circle(
                pos,
                r,
                Color32::TRANSPARENT,
                Stroke::new(w(LINE_WIDTH), fill),
            );
            painter.circle_stroke(pos, r * 0.5, Stroke::new(w(LINE_WIDTH), fill));
        }
        HandleKind::Point | HandleKind::Size => {
            painter.circle(pos, r, fill, outline);
        }
    }
}

/// Draws an overlay's shape (and its label) through a mapping.
pub fn paint_shape(painter: &Painter, m: &Mapping, o: &Overlay, t: &ViewportTokens) {
    let s = |p: [f64; 2]| m.image_to_screen(p);
    let under = Stroke::new(snapped(painter, UNDER_WIDTH), t.handle_outline);
    let over = Stroke::new(snapped(painter, LINE_WIDTH), t.guide);
    let line = |pts: Vec<Pos2>, closed: bool| {
        for stroke in [under, over] {
            if closed {
                painter.add(egui::Shape::closed_line(pts.clone(), stroke));
            } else {
                painter.add(egui::Shape::line(pts.clone(), stroke));
            }
        }
    };
    match &o.shape {
        Shape::Point(p) => {
            let c = s(*p);
            line(vec![c + vec2(-6.0, 0.0), c + vec2(6.0, 0.0)], false);
            line(vec![c + vec2(0.0, -6.0), c + vec2(0.0, 6.0)], false);
        }
        Shape::Line(a, b) => line(vec![s(*a), s(*b)], false),
        Shape::Rect(r) => line(
            vec![
                s([r[0], r[1]]),
                s([r[2], r[1]]),
                s([r[2], r[3]]),
                s([r[0], r[3]]),
            ],
            true,
        ),
        Shape::Circle { center, radius } => {
            let c = s(*center);
            let rr = (*radius * m.zoom / f64::from(m.ppp)) as f32;
            painter.circle_stroke(c, rr, under);
            painter.circle_stroke(c, rr, over);
        }
        Shape::Path { points, closed } => line(points.iter().map(|p| s(*p)).collect(), *closed),
    }
    if let Some(label) = &o.label {
        let at = o
            .handles
            .first()
            .map_or_else(|| first_point(&o.shape), |h| h.at);
        let p = s(at) + vec2(8.0, -8.0);
        painter.text(
            pos2(p.x, p.y),
            egui::Align2::LEFT_BOTTOM,
            label,
            egui::FontId::proportional(12.0),
            t.guide,
        );
    }
}

fn first_point(shape: &Shape) -> [f64; 2] {
    match shape {
        Shape::Point(p) | Shape::Line(p, _) => *p,
        Shape::Rect(r) => [r[0], r[1]],
        Shape::Circle { center, .. } => *center,
        Shape::Path { points, .. } => points.first().copied().unwrap_or([0.0, 0.0]),
    }
}

/// The handle under `pos` (points), topmost overlay first.
pub fn hit(overlays: &[Overlay], m: &Mapping, pos: Pos2) -> Option<(usize, Handle)> {
    let mut best: Option<(usize, Handle, f32)> = None;
    for (i, o) in overlays.iter().enumerate().rev() {
        for h in &o.handles {
            let d = m.image_to_screen(h.at).distance(pos);
            if d <= HIT_RADIUS && best.as_ref().is_none_or(|b| d < b.2) {
                best = Some((i, *h, d));
            }
        }
        if best.is_some() {
            break;
        }
    }
    best.map(|(i, h, _)| (i, h))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_box_follows_its_corner_edge_and_body() {
        let r = Shape::Rect([10.0, 10.0, 50.0, 30.0]);
        assert_eq!(
            r.moved(2, [50.0, 30.0], [60.0, 40.0]),
            Shape::Rect([10.0, 10.0, 60.0, 40.0])
        );
        assert_eq!(
            r.moved(7, [10.0, 20.0], [5.0, 99.0]),
            Shape::Rect([5.0, 10.0, 50.0, 30.0])
        );
        assert_eq!(
            r.moved(8, [30.0, 20.0], [31.0, 22.0]),
            Shape::Rect([11.0, 12.0, 51.0, 32.0])
        );
        // Dragged past the opposite corner: still a box.
        assert_eq!(
            r.moved(0, [10.0, 10.0], [70.0, 50.0]),
            Shape::Rect([50.0, 30.0, 70.0, 50.0])
        );
    }

    #[test]
    fn a_circle_sizes_from_its_size_handle() {
        let c = Shape::Circle {
            center: [0.0, 0.0],
            radius: 5.0,
        };
        assert_eq!(
            c.moved(1, [5.0, 0.0], [0.0, 12.0]),
            Shape::Circle {
                center: [0.0, 0.0],
                radius: 12.0
            }
        );
    }
}
