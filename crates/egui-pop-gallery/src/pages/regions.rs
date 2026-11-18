//! The slot regions page: read-outs and tool chips in named regions
//! around and over an image-like surface (`egui_pop::fit::layout_regions`),
//! the same declaration at two sizes: in the smaller, lower-priority cards
//! fold.

use egui::{Color32, Mesh, Rect, Sense, Shape, Stroke, StrokeKind, Ui, pos2, vec2};
use egui_pop::core::kit::{Kit, KitCx, Part, State as KitState};
use egui_pop::core::theme::Tokens;
use egui_pop::fit::{Anchor, Card, PointRect, Region, RegionLayout, Surface, layout_regions};

/// The page keeps no state.
#[derive(Debug, Default)]
pub struct State;

/// The engine's unit, in points.
const UNIT: f32 = 8.0;

/// The regions: key, anchor, around the image or over it, priority.
const REGIONS: [(u64, Anchor, bool, i32); 7] = [
    (1, Anchor::TopStart, false, 2),
    (2, Anchor::TopEnd, false, 3),
    (3, Anchor::Start, false, 1),
    (4, Anchor::Bottom, false, 0),
    (5, Anchor::BottomEnd, false, 1),
    (6, Anchor::Top, true, 0),
    (7, Anchor::Bottom, true, 0),
];

/// The cards: key, region, label, size in points, priority.
const CARDS: [(u64, u64, &str, [f32; 2], i32); 13] = [
    (10, 1, "ISO 200", [56.0, 18.0], 2),
    (11, 1, "1/250 s", [56.0, 18.0], 1),
    (12, 1, "f/8", [36.0, 18.0], 0),
    (20, 2, "Histogram", [80.0, 40.0], 3),
    (30, 3, "C", [18.0, 18.0], 3),
    (31, 3, "B", [18.0, 18.0], 2),
    (32, 3, "M", [18.0, 18.0], 1),
    (33, 3, "H", [18.0, 18.0], 0),
    (40, 4, "Zoom 100 %", [80.0, 18.0], 0),
    (41, 4, "Before / after", [80.0, 18.0], -1),
    (50, 5, "3 stars", [52.0, 18.0], 1),
    (60, 6, "IMG_0042.CR3", [100.0, 18.0], 0),
    (70, 7, "12 of 120", [72.0, 18.0], 0),
];

/// The regions and cards, as the page declares them.
pub fn declaration() -> (Vec<Region>, Vec<Card>) {
    let regions = REGIONS
        .iter()
        .map(|&(key, anchor, around, priority)| {
            let r = Region::new(key, anchor).with_priority(priority);
            // The start column, the top start row and the bottom row one
            // card deep: what does not fit folds instead of wrapping.
            let r =
                if !around && matches!(anchor, Anchor::Start | Anchor::TopStart | Anchor::Bottom) {
                    r.with_depth(24.0)
                } else {
                    r
                };
            if around {
                r.around().with_gap(4.0)
            } else {
                r.with_gap(6.0)
            }
        })
        .collect();
    let cards = CARDS
        .iter()
        .map(|&(key, region, _, size, priority)| {
            Card::sized(key, region, size[0], size[1]).with_priority(priority)
        })
        .collect();
    (regions, cards)
}

/// The labels of the folded cards, in the cards' order.
pub fn folded(layout: &RegionLayout) -> Vec<&'static str> {
    layout
        .folded()
        .into_iter()
        .filter_map(|key| CARDS.iter().find(|c| c.0 == key).map(|c| c.2))
        .collect()
}

/// Lays the page's cards out around an image `image` within `bounds`,
/// both from the canvas's top left.
pub fn solve(bounds: [f32; 2], image: [f32; 4]) -> RegionLayout {
    let (regions, cards) = declaration();
    let surface = Surface::new(
        PointRect {
            x: image[0],
            y: image[1],
            w: image[2],
            h: image[3],
        },
        UNIT,
    )
    .within(PointRect {
        x: 0.0,
        y: 0.0,
        w: bounds[0],
        h: bounds[1],
    });
    layout_regions(&surface, &regions, &cards, None)
}

/// The wide canvas and the narrow one: bounds, then the image.
pub const WIDE: ([f32; 2], [f32; 4]) = ([340.0, 270.0], [30.0, 34.0, 280.0, 202.0]);
pub const NARROW: ([f32; 2], [f32; 4]) = ([240.0, 270.0], [15.0, 34.0, 210.0, 202.0]);

/// The same regions and cards at two sizes.
pub fn show(ui: &mut Ui, _: &mut State) {
    ui.label("Cards in named regions around and over an image: corners, edges, the centre and an overlay, each with an anchor, a gap and a priority (egui_pop::fit::layout_regions). Regions take their room in priority order; each packs its cards from its anchor outwards, and folds its lowest-priority cards when room runs out. The outlines are each region's block.");
    ui.horizontal_top(|ui| {
        canvas(ui, "Regions, wide", WIDE);
        canvas(ui, "Regions, narrow", NARROW);
    });
}

fn canvas(ui: &mut Ui, name: &str, (bounds, image): ([f32; 2], [f32; 4])) {
    ui.vertical(|ui| {
        let (rect, response) = ui.allocate_exact_size(vec2(bounds[0], bounds[1]), Sense::hover());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, name));
        let layout = solve(bounds, image);
        let t = Tokens::of(ui.ctx());
        let painter = ui.painter_at(rect);
        let at = |r: PointRect| Rect::from_min_size(rect.min + vec2(r.x, r.y), vec2(r.w, r.h));
        painter.rect_filled(rect, t.radius.widget, t.surface.sunken);
        let img = Rect::from_min_size(
            rect.min + vec2(image[0], image[1]),
            vec2(image[2], image[3]),
        );
        gradient(&painter, img);
        for region in &layout.regions {
            if let Some(b) = region.block {
                painter.rect_stroke(
                    at(b).expand(2.0),
                    t.radius.widget,
                    Stroke::new(1.0, t.border.strong),
                    StrokeKind::Middle,
                );
            }
        }
        let kit = Kit::of(ui.ctx());
        let part = &kit.viewport_label;
        let size = if part.data.font_size > 0.0 {
            part.data.font_size
        } else {
            t.font.small
        };
        let cx = KitCx {
            painter: &painter,
            ppp: ui.ctx().pixels_per_point(),
        };
        for card in &layout.cards {
            let (Some(r), Some(label)) = (card.rect, CARDS.iter().find(|c| c.0 == card.key)) else {
                continue;
            };
            let galley = painter.layout_no_wrap(
                label.2.to_owned(),
                egui::FontId::proportional(size),
                Color32::PLACEHOLDER,
            );
            let r = at(r);
            part.paint_part(
                &cx,
                &Part {
                    rect: r,
                    room: r,
                    state: KitState::REST,
                    label: Some(&galley),
                },
            );
        }
        let folded = folded(&layout);
        if folded.is_empty() {
            ui.weak(format!("{name}: every card placed"));
        } else {
            ui.weak(format!("{name}, folded: {}", folded.join(", ")));
        }
    });
}

/// A pretend picture: a sky over a darker ground.
fn gradient(painter: &egui::Painter, r: Rect) {
    let top = Color32::from_rgb(92, 136, 186);
    let mid = Color32::from_rgb(214, 196, 160);
    let low = Color32::from_rgb(62, 74, 52);
    let mut mesh = Mesh::default();
    let y = r.top() + r.height() * 0.6;
    for (a, b, ca, cb) in [(r.top(), y, top, mid), (y, r.bottom(), mid, low)] {
        let i = mesh.vertices.len() as u32;
        mesh.colored_vertex(pos2(r.left(), a), ca);
        mesh.colored_vertex(pos2(r.right(), a), ca);
        mesh.colored_vertex(pos2(r.right(), b), cb);
        mesh.colored_vertex(pos2(r.left(), b), cb);
        mesh.add_triangle(i, i + 1, i + 2);
        mesh.add_triangle(i, i + 2, i + 3);
    }
    painter.add(Shape::mesh(mesh));
}
