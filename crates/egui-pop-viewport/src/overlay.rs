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
//! (move), points and corners, edges, a size, a rotation, a feather, a
//! Bézier control. Their hit target is [`HIT_RADIUS`] points whatever
//! their drawn size. A click on a handle, or on an overlay's outline away
//! from its handles (to add a point there), is a signal too.
//!
//! Each overlay has a [`Style`]: an emphasis (normal, selected, faint)
//! and a dashed line; shapes are points, lines, boxes, circles, ellipses
//! and paths.

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
    /// A Bézier control point: the end of a curve's tangent at a point.
    Control,
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
            HandleKind::Control => "control",
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
    /// An ellipse: its radii along its own axes, turned by `rotation`
    /// (radians, clockwise on screen).
    Ellipse {
        center: [f64; 2],
        radii: [f64; 2],
        rotation: f64,
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
            Shape::Ellipse {
                center,
                radii,
                rotation,
            } => {
                let at = |x: f64, y: f64| along(*center, *rotation, x, y);
                vec![
                    h(0, HandleKind::Move, *center),
                    h(1, HandleKind::Size, at(radii[0], 0.0)),
                    h(2, HandleKind::Size, at(0.0, radii[1])),
                    h(3, HandleKind::Rotate, at(0.0, -radii[1] * ROTATE_OUT)),
                ]
            }
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
            Shape::Ellipse {
                center,
                radii,
                rotation,
            } => {
                let v = [to[0] - center[0], to[1] - center[1]];
                let (s, c) = rotation.sin_cos();
                // The pointer in the ellipse's own axes.
                let (x, y) = (v[0] * c + v[1] * s, -v[0] * s + v[1] * c);
                let (mut radii, mut rotation, mut center) = (*radii, *rotation, *center);
                match index {
                    1 => radii[0] = x.abs().max(MIN_RADIUS),
                    2 => radii[1] = y.abs().max(MIN_RADIUS),
                    3 => rotation = v[0].atan2(-v[1]),
                    _ => center = shift(center),
                }
                Shape::Ellipse {
                    center,
                    radii,
                    rotation,
                }
            }
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

/// How far out an ellipse's rotation handle is, past its second radius.
const ROTATE_OUT: f64 = 1.3;

/// The smallest radius a size handle gives an ellipse (image pixels).
const MIN_RADIUS: f64 = 0.5;

/// The point `(x, y)` of a frame centred at `center`, turned by
/// `rotation` (image pixels).
fn along(center: [f64; 2], rotation: f64, x: f64, y: f64) -> [f64; 2] {
    let (s, c) = rotation.sin_cos();
    [center[0] + x * c - y * s, center[1] + x * s + y * c]
}

/// The points of an ellipse's outline (image pixels).
fn ellipse_points(center: [f64; 2], radii: [f64; 2], rotation: f64, n: usize) -> Vec<[f64; 2]> {
    (0..n)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / n as f64;
            along(center, rotation, radii[0] * a.cos(), radii[1] * a.sin())
        })
        .collect()
}

/// How an overlay stands out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Emphasis {
    /// Drawn as usual.
    #[default]
    Normal,
    /// The one being edited: a heavier line in the handle colour.
    Selected,
    /// In the background (a shape not being edited): half-transparent,
    /// handles included.
    Faint,
}

/// How an overlay is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub emphasis: Emphasis,
    /// A dashed line (a shape that is not there yet, a feathered edge).
    pub dashed: bool,
}

/// An overlay: a shape with an id, its handles, and whether it is a
/// guide (drawn, never edited).
#[derive(Clone, Debug, PartialEq)]
pub struct Overlay {
    pub id: String,
    pub shape: Shape,
    pub handles: Vec<Handle>,
    pub label: Option<String>,
    pub style: Style,
}

impl Overlay {
    /// An editable overlay with the shape's standard handles.
    pub fn new(id: impl Into<String>, shape: Shape) -> Overlay {
        Overlay {
            id: id.into(),
            handles: shape.handles(),
            shape,
            label: None,
            style: Style::default(),
        }
    }

    /// A guide: drawn, no handles.
    pub fn guide(id: impl Into<String>, shape: Shape) -> Overlay {
        Overlay {
            id: id.into(),
            shape,
            handles: Vec::new(),
            label: None,
            style: Style::default(),
        }
    }

    /// With handles of its own instead of the shape's (a path with its
    /// Bézier controls); the application applies their moves itself.
    pub fn handles(mut self, handles: Vec<Handle>) -> Overlay {
        self.handles = handles;
        self
    }

    /// Drawn with this style.
    pub fn style(mut self, style: Style) -> Overlay {
        self.style = style;
        self
    }

    /// The one being edited.
    pub fn selected(mut self) -> Overlay {
        self.style.emphasis = Emphasis::Selected;
        self
    }

    /// In the background.
    pub fn faint(mut self) -> Overlay {
        self.style.emphasis = Emphasis::Faint;
        self
    }

    /// With a dashed line.
    pub fn dashed(mut self) -> Overlay {
        self.style.dashed = true;
        self
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
    paint_handle_styled(painter, pos, kind, state, Emphasis::Normal, t);
}

/// Draws one handle of the kit at `pos` (points), faint when its overlay
/// is.
pub fn paint_handle_styled(
    painter: &Painter,
    pos: Pos2,
    kind: HandleKind,
    state: HandleState,
    emphasis: Emphasis,
    t: &ViewportTokens,
) {
    let alpha = if emphasis == Emphasis::Faint {
        FAINT
    } else {
        1.0
    };
    let fill = match state {
        HandleState::Rest => t.handle,
        HandleState::Hover | HandleState::Active => t.handle_hover,
    }
    .gamma_multiply(alpha);
    let r = if state == HandleState::Rest {
        DRAWN_RADIUS
    } else {
        DRAWN_RADIUS + 1.5
    };
    let w = |x: f32| snapped(painter, x);
    let outline = Stroke::new(w(OUTLINE_WIDTH), t.handle_outline.gamma_multiply(alpha));
    match kind {
        HandleKind::Move => {
            painter.circle(pos, r, fill, outline);
            painter.circle_filled(pos, r * 0.35, outline.color);
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
        HandleKind::Control => {
            // A small diamond: a tangent's end, not a point of the shape.
            let d = r * 0.8;
            let pts = vec![
                pos + vec2(0.0, -d),
                pos + vec2(d, 0.0),
                pos + vec2(0.0, d),
                pos + vec2(-d, 0.0),
            ];
            painter.add(egui::Shape::convex_polygon(pts, fill, outline));
        }
        HandleKind::Point | HandleKind::Size => {
            painter.circle(pos, r, fill, outline);
        }
    }
}

/// How transparent a faint overlay is drawn.
const FAINT: f32 = 0.45;

/// A dash and the gap after it, in points, along a dashed line.
const DASH: [f32; 2] = [6.0, 4.0];

/// The points of a shape's outline on screen, and whether it closes;
/// `None` for a point (drawn as a cross).
fn outline(shape: &Shape, m: &Mapping) -> Option<(Vec<Pos2>, bool)> {
    let s = |p: [f64; 2]| m.image_to_screen(p);
    Some(match shape {
        Shape::Point(_) => return None,
        Shape::Line(a, b) => (vec![s(*a), s(*b)], false),
        Shape::Rect(r) => (
            vec![
                s([r[0], r[1]]),
                s([r[2], r[1]]),
                s([r[2], r[3]]),
                s([r[0], r[3]]),
            ],
            true,
        ),
        Shape::Circle { center, radius } => (
            ellipse_points(*center, [*radius, *radius], 0.0, CURVE_POINTS)
                .into_iter()
                .map(s)
                .collect(),
            true,
        ),
        Shape::Ellipse {
            center,
            radii,
            rotation,
        } => (
            ellipse_points(*center, *radii, *rotation, CURVE_POINTS)
                .into_iter()
                .map(s)
                .collect(),
            true,
        ),
        Shape::Path { points, closed } => (points.iter().map(|p| s(*p)).collect(), *closed),
    })
}

/// How many points draw a circle or an ellipse.
const CURVE_POINTS: usize = 96;

/// Draws a polyline, plain or dashed.
fn polyline(painter: &Painter, mut pts: Vec<Pos2>, closed: bool, stroke: Stroke, dashed: bool) {
    if dashed {
        if closed && let Some(first) = pts.first().copied() {
            pts.push(first);
        }
        painter.extend(egui::Shape::dashed_line(&pts, stroke, DASH[0], DASH[1]));
    } else if closed {
        painter.add(egui::Shape::closed_line(pts, stroke));
    } else {
        painter.add(egui::Shape::line(pts, stroke));
    }
}

/// Draws an overlay's shape (and its label) through a mapping, in its
/// style.
pub fn paint_shape(painter: &Painter, m: &Mapping, o: &Overlay, t: &ViewportTokens) {
    let s = |p: [f64; 2]| m.image_to_screen(p);
    let w = |x: f32| snapped(painter, x);
    let (line_color, line_width, alpha) = match o.style.emphasis {
        Emphasis::Normal => (t.guide, LINE_WIDTH, 1.0),
        Emphasis::Selected => (t.handle, LINE_WIDTH + 0.5, 1.0),
        Emphasis::Faint => (t.guide, LINE_WIDTH, FAINT),
    };
    let under = Stroke::new(
        w(UNDER_WIDTH + line_width - LINE_WIDTH),
        t.handle_outline.gamma_multiply(alpha),
    );
    let over = Stroke::new(w(line_width), line_color.gamma_multiply(alpha));
    let dashed = o.style.dashed;
    match outline(&o.shape, m) {
        Some((pts, closed)) => {
            for stroke in [under, over] {
                polyline(painter, pts.clone(), closed, stroke, dashed);
            }
        }
        None => {
            let c = s(first_point(&o.shape));
            for stroke in [under, over] {
                for (a, b) in [
                    (vec2(-6.0, 0.0), vec2(6.0, 0.0)),
                    (vec2(0.0, -6.0), vec2(0.0, 6.0)),
                ] {
                    polyline(painter, vec![c + a, c + b], false, stroke, false);
                }
            }
        }
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
            line_color.gamma_multiply(alpha),
        );
    }
}

fn first_point(shape: &Shape) -> [f64; 2] {
    match shape {
        Shape::Point(p) | Shape::Line(p, _) => *p,
        Shape::Rect(r) => [r[0], r[1]],
        Shape::Circle { center, .. } | Shape::Ellipse { center, .. } => *center,
        Shape::Path { points, .. } => points.first().copied().unwrap_or([0.0, 0.0]),
    }
}

/// How close to an outline a click lands on it, in points.
pub const EDGE_RADIUS: f32 = 6.0;

/// The outline under `pos` (points), topmost overlay first, guides left
/// out: the overlay's index, the segment (between points `i` and `i + 1`
/// of a path or a line, the box's top, right, bottom and left edges `0`
/// to `3`, `0` for a circle or an ellipse) and the nearest point of it
/// in image pixels.
pub fn hit_edge(overlays: &[Overlay], m: &Mapping, pos: Pos2) -> Option<(usize, u32, [f64; 2])> {
    for (i, o) in overlays.iter().enumerate().rev() {
        if o.handles.is_empty() {
            continue;
        }
        let Some((pts, closed)) = outline(&o.shape, m) else {
            continue;
        };
        let curve = matches!(o.shape, Shape::Circle { .. } | Shape::Ellipse { .. });
        let n = pts.len();
        let count = if closed { n } else { n.saturating_sub(1) };
        let mut best: Option<(u32, Pos2, f32)> = None;
        for k in 0..count {
            let (a, b) = (pts[k], pts[(k + 1) % n]);
            let ab = b - a;
            let t = if ab.length_sq() > 0.0 {
                ((pos - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let near = a + ab * t;
            let d = near.distance(pos);
            if d <= EDGE_RADIUS && best.is_none_or(|b| d < b.2) {
                let segment = if curve { 0 } else { k as u32 };
                best = Some((segment, near, d));
            }
        }
        if let Some((segment, near, _)) = best {
            return Some((i, segment, m.screen_to_image(near)));
        }
    }
    None
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
    fn an_ellipse_sizes_along_its_axes_and_turns() {
        let e = Shape::Ellipse {
            center: [0.0, 0.0],
            radii: [10.0, 5.0],
            rotation: 0.0,
        };
        let h = e.handles();
        assert_eq!(h.len(), 4);
        assert_eq!(h[1].at, [10.0, 0.0]);
        assert_eq!(h[3].kind, HandleKind::Rotate);
        // The first radius follows the pointer along its own axis only.
        assert_eq!(
            e.moved(1, [10.0, 0.0], [20.0, 7.0]),
            Shape::Ellipse {
                center: [0.0, 0.0],
                radii: [20.0, 5.0],
                rotation: 0.0
            }
        );
        // The rotation handle, dragged to the right: a quarter turn.
        let Shape::Ellipse { rotation, .. } = e.moved(3, h[3].at, [8.0, 0.0]) else {
            panic!()
        };
        assert!((rotation - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
        let turned = Shape::Ellipse {
            center: [0.0, 0.0],
            radii: [10.0, 5.0],
            rotation,
        };
        let h = turned.handles();
        assert!((h[1].at[0]).abs() < 1e-9 && (h[1].at[1] - 10.0).abs() < 1e-9);
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
