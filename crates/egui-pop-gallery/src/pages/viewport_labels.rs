//! The labels-over-images page: the kit's `viewport-label` component
//! (`egui_pop::viewport::overlay::paint_label`) over a bright and a dark
//! image, with an axis gizmo in the `palette.axis.*` colours.

use egui::{Align2, Color32, Mesh, Pos2, Rect, Sense, Shape, Stroke, Ui, vec2};
use egui_pop::core::theme::Tokens;
use egui_pop::viewport::overlay::paint_label;

/// The page keeps no state.
#[derive(Debug, Default)]
pub struct State;

/// Two images, a bright sky and a night, each with the same labels.
pub fn show(ui: &mut Ui, _: &mut State) {
    ui.label("A label over an image is a translucent pill (the kit's viewport-label): the image shows through, and its text reads over the brightest and the darkest pixels alike. The axes take the theme's palette.axis colours.");
    ui.horizontal(|ui| {
        image(
            ui,
            "Bright image",
            [
                Color32::from_rgb(250, 250, 244),
                Color32::from_rgb(255, 236, 170),
            ],
        );
        image(
            ui,
            "Dark image",
            [Color32::from_rgb(8, 10, 24), Color32::from_rgb(40, 30, 52)],
        );
    });
}

/// One image: a vertical gradient from `colors[0]` (top) to `colors[1]`,
/// a crop box and a point with their labels, a readout and the axes.
fn image(ui: &mut Ui, name: &str, colors: [Color32; 2]) {
    let (rect, response) = ui.allocate_exact_size(vec2(300.0, 200.0), Sense::hover());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, name));
    let painter = ui.painter_at(rect);
    let mut mesh = Mesh::default();
    mesh.colored_vertex(rect.left_top(), colors[0]);
    mesh.colored_vertex(rect.right_top(), colors[0]);
    mesh.colored_vertex(rect.right_bottom(), colors[1]);
    mesh.colored_vertex(rect.left_bottom(), colors[1]);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(Shape::mesh(mesh));
    let t = Tokens::of(ui.ctx());
    let crop = Rect::from_min_size(rect.min + vec2(60.0, 50.0), vec2(150.0, 100.0));
    painter.rect_stroke(
        crop,
        0.0,
        Stroke::new(1.5, t.text.strong),
        egui::StrokeKind::Middle,
    );
    paint_label(
        &painter,
        crop.left_top() + vec2(8.0, -8.0),
        Align2::LEFT_BOTTOM,
        "Crop",
    );
    let point = crop.right_bottom() + vec2(30.0, -20.0);
    painter.circle_filled(point, 3.0, t.text.strong);
    paint_label(
        &painter,
        point + vec2(8.0, -8.0),
        Align2::LEFT_BOTTOM,
        "Point 2",
    );
    paint_label(
        &painter,
        rect.left_bottom() + vec2(8.0, -8.0),
        Align2::LEFT_BOTTOM,
        "1204, 873  L 0.912",
    );
    axes(&painter, rect.right_bottom() + vec2(-40.0, -16.0), &t);
}

/// An axis gizmo at `origin`: X right, Y up, Z towards the viewer.
fn axes(painter: &egui::Painter, origin: Pos2, t: &Tokens) {
    let axis = &t.palette.axis;
    for (dir, color) in [
        (vec2(24.0, 0.0), axis.x),
        (vec2(0.0, -24.0), axis.y),
        (vec2(-14.0, 12.0), axis.z),
    ] {
        painter.arrow(origin, dir, Stroke::new(2.0, color));
    }
}
