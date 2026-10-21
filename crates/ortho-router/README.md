# ortho-router

Orthogonal connector routing around rectangular obstacles: the wires of
a node graph or a diagram drawn with right angles, going around the
nodes instead of through them, with few bends and short lengths.

- Plain `f32` types, no dependencies, no `unsafe`, no GUI toolkit: it
  works with egui, iced, a web canvas or an SVG writer alike.
- Ports have a side: a wire leaves an output on a node's right edge
  going right and enters an input on a left edge going right.
- Deterministic: the same obstacles, options and connectors give the
  same routes, bit for bit.
- A batch call shares one grid between all the connectors of a graph.

It was written for the node graph editor of
[egui-pop](https://github.com/quazardous/egui-pop) and is published on
its own for any project that draws connectors.

## Example

```rust
use ortho_router::{Connector, Options, Port, Rect, Router, Side};

// Two nodes side by side and a third one in the way between them.
let a = Rect::from_min_size([0.0, 0.0], 100.0, 60.0);
let b = Rect::from_min_size([400.0, 0.0], 100.0, 60.0);
let wall = Rect::from_min_size([200.0, -40.0], 100.0, 140.0);

let mut router = Router::new(Options::default()).with_obstacles([a, b, wall]);
let wire = Connector::new(
    Port::new([100.0, 30.0], Side::Right), // an output on a's right edge
    Port::new([400.0, 30.0], Side::Left),  // an input on b's left edge
);
let route = router.route(&wire);
assert!(route.routed);
// Out of a, around the wall (above or below it), into b.
assert_eq!(route.points.first(), Some(&wire.from.at));
assert_eq!(route.points.last(), Some(&wire.to.at));
assert!(route.bends() >= 4);
for w in route.points.windows(2) {
    assert!(w[0].x == w[1].x || w[0].y == w[1].y, "only right angles");
}
```

Many connectors at once, the way a graph editor draws its wires:

```rust
use ortho_router::{Connector, Options, Port, Rect, Router, Side};

let nodes = [
    Rect::from_min_size([0.0, 0.0], 100.0, 60.0),
    Rect::from_min_size([0.0, 120.0], 100.0, 60.0),
    Rect::from_min_size([300.0, 60.0], 100.0, 60.0),
];
let wires = [
    Connector::new(Port::new([100.0, 30.0], Side::Right), Port::new([300.0, 80.0], Side::Left)),
    Connector::new(Port::new([100.0, 150.0], Side::Right), Port::new([300.0, 100.0], Side::Left)),
];
let mut router = Router::new(Options::default()).with_obstacles(nodes);
// A cache keyed by the layout: route again only when it changes.
let key = router.fingerprint(&wires);
let routes = router.route_all(&wires);
assert_eq!(routes.len(), 2);
assert_eq!(router.fingerprint(&wires), key);
```

## How it routes

Each obstacle is grown by the margin (`Options::margin`). The routing
grid is the Hanan grid of the grown obstacles: a vertical line through
each one's left and right edges, a horizontal line through its top and
bottom edges, and lines through each port's stub end (the point
`margin` out of the port, where a route may first turn). Grid edges
that cross a grown obstacle's interior are removed; edges along its
boundary stay, so routes run at the margin's distance from the nodes.
This is the sparse "orthogonal visibility graph" of Wybrow, Marriott and
Stuckey, *Orthogonal Connector Routing* (Graph Drawing 2009), built the
simple way: every line crosses every other one, which costs more
vertices than their sweep but is a few dozen lines and plenty for the
hundreds of nodes an interactive editor shows.

A route is then a cheapest path in that grid with A*, over states made
of a vertex and the direction the route arrived in, so that a turn
costs `Options::bend_penalty` on top of the length. The heuristic is
the Manhattan distance to the goal, a lower bound, so the route found
is the cheapest. Ties are broken by state index, which makes the output
deterministic. The route leaves its first port along the port's side
and arrives at the second one coming in through its side (a last turn
there costs a bend too).

`Router::route_all` then nudges: segments of different routes on one
line that overlap are spread `Options::spacing` apart across it,
centred on the line and within the margin, ordered so the spreading
adds as few crossings as it can. A segment moves as a whole, so the
route stays orthogonal; the segments at the ports stay where they are.

When no route exists (a port boxed in, a stub end inside another
obstacle), the router does not fail: it returns the fallback, a stub out
of each port joined by one elbow halfway, with `Route::routed` set to
`false`, so the caller can draw it differently.

## What it does not do (yet)

- Obstacles are rectangles; a port's stub is not checked against other
  obstacles, only its end.
- Routes are found one by one, then nudged: wires sharing a channel are
  spread apart, but a route does not make room for the others, and a
  channel with many wires squeezes them within the margin.
- No incremental update: each call rebuilds the grid. Cache routes
  under `Router::fingerprint` while the layout does not change.

## Prior art

Surveyed in October 2026 before writing this crate, for an
obstacle-avoiding orthogonal router to depend on or contribute to:

| Crate or project | Licence | Obstacles | Notes |
|---|---|---|---|
| [libavoid](https://www.adaptagrams.org/) (Adaptagrams, C++) | LGPL-2.1 | yes | The reference (Wybrow); orthogonal and polyline routing, nudging, incremental. C++ and LGPL. |
| [libavoid-rust](https://github.com/boxabirds/libavoid-rust) | LGPL-2.1 (a port) | yes | Not on crates.io; last change 2025-11. A port of an LGPL library, so LGPL itself. |
| [msagl-rust](https://github.com/ByteBard97/msagl-rust) | MIT | yes | A port of MSAGL's rectilinear edge router with edge separation: complete, but about 800 kB of source, four dependencies, `f64`, not on crates.io, quiet since 2026-03. |
| [clearpath](https://crates.io/crates/clearpath) 0.1 | MIT OR Apache-2.0 | yes | Path planning around polygons with smooth Bézier output; its rectilinear mode is a cell A* without a bend cost or port sides, and ports may not touch obstacles. `f64`, one release. |
| [neco-edge-routing](https://crates.io/crates/neco-edge-routing) 0.1 | MIT | no | Route shapes between two points (Bézier, orthogonal elbows, splines); no obstacles. |
| [elk-rs](https://github.com/openedges/elk-rs) (`elk-layout`) | EPL-2.0 OR GPL-3.0 | yes | A port of the Eclipse Layout Kernel, libavoid integration included; a whole layout engine, copyleft. |
| egui_mobius's `egui_grafica` | MIT | no | Its orthogonal style is one elbow halfway between the ports, which may cross nodes. |
| [arcivis-layout](https://github.com/christianjann/arcivis-layout) | MIT | yes | Layout and routing for architecture diagrams, not published, tied to its own graph model. |

None was a small, maintained, permissively licensed router with port
sides and `f32` coordinates, so this crate is written from the
published description of the algorithm. Code from LGPL and GPL projects
was not read or copied.

## License

MIT OR Apache-2.0, at your option.
