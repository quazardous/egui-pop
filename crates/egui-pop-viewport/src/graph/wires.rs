//! How wires are drawn: curves, straight segments, or orthogonal routes
//! around the nodes (by the `ortho-router` crate), the latter cached
//! by layout.

use egui::{Pos2, Rect, pos2};
use ortho_router::{Connector, Options, Port, Router, Side};

/// How a graph's wires are drawn ([`GraphEditor::wires`](super::GraphEditor::wires)).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum WireStyle {
    /// Curves out of each port (the default): they may cross nodes.
    #[default]
    Curves,
    /// Horizontal and vertical segments around the nodes, with few
    /// bends; wires through one channel are spread apart.
    Orthogonal,
    /// A straight segment from port to port.
    Straight,
}

impl WireStyle {
    /// Every style, in the order a menu lists them.
    pub const ALL: [WireStyle; 3] = [
        WireStyle::Curves,
        WireStyle::Orthogonal,
        WireStyle::Straight,
    ];

    /// Its name in snapshots and steps: `curves`, `orthogonal`,
    /// `straight`.
    pub fn name(self) -> &'static str {
        match self {
            WireStyle::Curves => "curves",
            WireStyle::Orthogonal => "orthogonal",
            WireStyle::Straight => "straight",
        }
    }

    /// The style of a name ([`name`](Self::name)).
    pub fn from_name(name: &str) -> Option<WireStyle> {
        WireStyle::ALL.into_iter().find(|s| s.name() == name)
    }
}

/// How far routes keep from nodes, in graph units.
const MARGIN: f32 = 14.0;
/// What a bend costs, as a length in graph units.
const BEND: f32 = 40.0;
/// How far apart wires sharing a channel are drawn.
const SPACING: f32 = 5.0;

/// Orthogonal routes, computed again only when the layout changes.
#[derive(Clone, Debug)]
pub(crate) struct RouteCache {
    router: Router,
    /// The layout the routes are for.
    key: Option<u64>,
    routes: Vec<Vec<Pos2>>,
    /// How many times routes were computed (for tests).
    pub(crate) computed: u64,
}

impl Default for RouteCache {
    fn default() -> Self {
        RouteCache {
            router: Router::new(Options {
                margin: MARGIN,
                bend_penalty: BEND,
                spacing: SPACING,
            }),
            key: None,
            routes: Vec::new(),
            computed: 0,
        }
    }
}

fn point(p: Pos2) -> ortho_router::Point {
    ortho_router::Point::new(p.x, p.y)
}

impl RouteCache {
    /// The routes of `wires` (an output's position, then an input's)
    /// around `boxes`: one polyline per wire, in order.
    pub(crate) fn routes(&mut self, boxes: &[Rect], wires: &[(Pos2, Pos2)]) -> &[Vec<Pos2>] {
        self.router.set_obstacles(
            boxes
                .iter()
                .map(|r| ortho_router::Rect::from_corners(point(r.min), point(r.max))),
        );
        let connectors: Vec<Connector> = wires
            .iter()
            .map(|&(a, b)| {
                Connector::new(
                    Port::new(point(a), Side::Right),
                    Port::new(point(b), Side::Left),
                )
            })
            .collect();
        let key = self.router.fingerprint(&connectors);
        if self.key != Some(key) {
            self.routes = self
                .router
                .route_all(&connectors)
                .into_iter()
                .map(|r| r.points.iter().map(|p| pos2(p.x, p.y)).collect())
                .collect();
            self.key = Some(key);
            self.computed += 1;
        }
        &self.routes
    }

    /// Forgets the routes (the style changed away from orthogonal).
    pub(crate) fn clear(&mut self) {
        self.key = None;
        self.routes.clear();
    }
}

/// A rough orthogonal wire from an output at `a` to an input at `b`,
/// for the wire being drawn: out, one elbow halfway, in.
pub(crate) fn elbow(a: Pos2, b: Pos2) -> [Pos2; 4] {
    let x = (a.x + b.x) / 2.0;
    [a, pos2(x, a.y), pos2(x, b.y), b]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_are_computed_again_only_when_the_layout_changes() {
        let mut cache = RouteCache::default();
        let boxes = [
            Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(100.0, 60.0)),
            Rect::from_min_size(pos2(300.0, 100.0), egui::vec2(100.0, 60.0)),
        ];
        let wires = [(pos2(100.0, 30.0), pos2(300.0, 130.0))];
        let first = cache.routes(&boxes, &wires).to_vec();
        assert_eq!(cache.computed, 1);
        assert_eq!(cache.routes(&boxes, &wires), first.as_slice());
        assert_eq!(cache.computed, 1, "the same layout: from the cache");
        let moved = [boxes[0], boxes[1].translate(egui::vec2(0.0, 10.0))];
        let wires = [(pos2(100.0, 30.0), pos2(300.0, 140.0))];
        let r = cache.routes(&moved, &wires)[0].clone();
        assert_eq!(cache.computed, 2);
        assert_eq!(r.first(), Some(&pos2(100.0, 30.0)));
        assert_eq!(r.last(), Some(&pos2(300.0, 140.0)));
        assert!(r.windows(2).all(|w| w[0].x == w[1].x || w[0].y == w[1].y));
    }

    #[test]
    fn styles_have_names() {
        for s in WireStyle::ALL {
            assert_eq!(WireStyle::from_name(s.name()), Some(s));
        }
        assert_eq!(WireStyle::default(), WireStyle::Curves);
    }
}
