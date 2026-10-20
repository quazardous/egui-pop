//! The palettes page: a toolbox, a colour palette and a navigator
//! floating over a canvas, opened by buttons (closed elsewhere in the
//! gallery), each draggable by its band, dockable on an area, rolled up,
//! pinned, hidden and shown by Tab.

use egui::{Color32, Sense, Ui, pos2, vec2};
use egui_pop::api::palettes;
use egui_pop::api::stories::Story;
use egui_pop::api::workbench::Change;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::core::widgets;
use egui_pop::model::ids::{PanelId, PluginId, StoryId};
use egui_pop::model::panels::{Area, PanelSpec};
use egui_pop::theme::Tokens;

use super::section;
use crate::shell::SHELL;

pub const PALETTES_PAGE: PluginId = PluginId::from_static("gallery.palettes");

pub const TOOLBOX: PanelId = PanelId::from_static("gallery.toolbox");
pub const SWATCHES: PanelId = PanelId::from_static("gallery.swatches");
pub const NAVIGATOR: PanelId = PanelId::from_static("gallery.navigator");

/// The context key holding the tool chosen in the toolbox.
const TOOL: &str = "galleryTool";
/// The context key holding the colour chosen in the colour palette.
const COLOUR: &str = "galleryColour";

const TOOLS: [&str; 6] = ["Move", "Brush", "Eraser", "Crop", "Picker", "Hand"];

/// The palettes page and its palettes.
#[derive(Debug, Default)]
pub struct PalettesPage;

impl Plugin for PalettesPage {
    fn id(&self) -> PluginId {
        PALETTES_PAGE
    }

    fn title(&self) -> String {
        "Palettes".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        // Closed: they open from the page only.
        ctx.panel(
            PanelSpec::new(TOOLBOX, "Toolbox", Area::StartSide)
                .palette()
                .min_size(150.0)
                .default_size(150.0)
                .icon("edit")
                .closed(),
            toolbox,
        )?
        .panel(
            PanelSpec::new(SWATCHES, "Colours", Area::EndSide)
                .palette()
                .min_size(180.0)
                .default_size(180.0)
                .icon("theme")
                .closed(),
            swatches,
        )?
        .panel(
            PanelSpec::new(NAVIGATOR, "Navigator", Area::EndSide)
                .palette()
                .min_size(200.0)
                .default_size(200.0)
                .icon("search")
                .closed(),
            navigator,
        )?;
        ctx.story(
            Story::new(StoryId::from_static("palettes"), "Palettes", show)
                .description("Toolboxes and palettes floating over the canvas, never modal: dragged by their band, snapped, docked on an area, rolled up, pinned, hidden by Tab.")
                .group("Framework", 104),
        )?;
        Ok(())
    }
}

fn show(ui: &mut Ui, app: &AppHandle) {
    ui.horizontal_wrapped(|ui| {
        if ui.button("Open the palettes").clicked() {
            // Side by side rather than cascaded: none hides another.
            for (p, pos) in [
                (TOOLBOX, [16.0, 120.0]),
                (SWATCHES, [16.0, 330.0]),
                (NAVIGATOR, [520.0, 120.0]),
            ] {
                app.set_panel_visible(&p, true);
                app.change_workbench(Change::MovePalette(p, pos));
            }
        }
        if ui.button("Close them").clicked() {
            for p in [TOOLBOX, SWATCHES, NAVIGATOR] {
                app.set_panel_visible(&p, false);
            }
        }
        if ui.button("Reset palettes").clicked() {
            palettes::reset(app);
        }
    });
    let mut fade = app.context().get(palettes::CONTEXT_STROKE) == "true";
    if widgets::checkbox(
        ui,
        &mut fade,
        "A stroke is in progress (palettes fade with the setting on)",
    )
    .changed()
    {
        palettes::set_stroke(app, fade);
    }
    ui.weak("Drag a palette by its band; drop it on a side bar or the bottom panel to dock it, float it again from its tab menu. Tab hides and shows them while no field has the keyboard.");
    section(ui, "Canvas");
    canvas(ui, app);
}

/// A stand-in image filling the rest of the page, with the tool and the
/// colour chosen.
fn canvas(ui: &mut Ui, app: &AppHandle) {
    let t = Tokens::of(ui.ctx());
    let context = app.context();
    let tool = context.get(TOOL);
    let colour = context.get(COLOUR);
    ui.label(format!(
        "Tool: {}   Colour: {}",
        if tool.is_empty() { "Move" } else { tool },
        if colour.is_empty() { "none" } else { colour }
    ));
    let size = ui.available_size().max(vec2(200.0, 160.0));
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, t.surface.canvas);
    image(&painter, rect.shrink(t.space.xl));
}

/// The stand-in image: a sky, hills and a sun.
fn image(painter: &egui::Painter, r: egui::Rect) {
    let sky = Color32::from_rgb(92, 128, 168);
    let hill = Color32::from_rgb(70, 104, 64);
    let far = Color32::from_rgb(110, 132, 96);
    let sun = Color32::from_rgb(236, 196, 104);
    painter.rect_filled(r, 0.0, sky);
    painter.circle_filled(
        pos2(r.left() + r.width() * 0.72, r.top() + r.height() * 0.3),
        r.height() * 0.1,
        sun,
    );
    let ground = |h: f32, color: Color32| {
        let points: Vec<egui::Pos2> = (0..=16)
            .map(|i| {
                let x = r.left() + r.width() * i as f32 / 16.0;
                let y = r.top() + r.height() * (h + 0.06 * ((i as f32) * 0.9).sin());
                pos2(x, y)
            })
            .chain([r.right_bottom(), r.left_bottom()])
            .collect();
        painter.add(egui::Shape::convex_polygon(
            points,
            color,
            egui::Stroke::NONE,
        ));
    };
    ground(0.55, far);
    ground(0.7, hill);
}

fn toolbox(ui: &mut Ui, app: &AppHandle) {
    let chosen = app.context().get(TOOL).to_string();
    let chosen = if chosen.is_empty() {
        "Move".into()
    } else {
        chosen
    };
    egui::Grid::new("gallery.toolbox")
        .num_columns(2)
        .show(ui, |ui| {
            for (i, tool) in TOOLS.iter().enumerate() {
                if widgets::selectable_label(ui, chosen == *tool, *tool).clicked() {
                    app.set_context(TOOL, tool);
                }
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });
}

const COLOURS: [(&str, [u8; 3]); 8] = [
    ("Black", [24, 24, 24]),
    ("White", [240, 240, 236]),
    ("Red", [200, 64, 56]),
    ("Orange", [228, 140, 52]),
    ("Yellow", [232, 204, 80]),
    ("Green", [84, 160, 88]),
    ("Blue", [64, 112, 196]),
    ("Violet", [140, 92, 180]),
];

fn swatches(ui: &mut Ui, app: &AppHandle) {
    let t = Tokens::of(ui.ctx());
    let chosen = app.context().get(COLOUR).to_string();
    ui.horizontal_wrapped(|ui| {
        for (name, [r, g, b]) in COLOURS {
            let side = t.size.control_height;
            let (rect, response) = ui.allocate_exact_size(vec2(side, side), Sense::click());
            ui.painter()
                .rect_filled(rect, t.radius.widget, Color32::from_rgb(r, g, b));
            let stroke = if chosen == name || response.has_focus() {
                egui::Stroke::new(2.0, t.focus_ring)
            } else {
                egui::Stroke::new(1.0, t.border.default)
            };
            ui.painter()
                .rect_stroke(rect, t.radius.widget, stroke, egui::StrokeKind::Inside);
            egui_pop::core::a11y::name(&response, egui::WidgetType::Button, name);
            if response.on_hover_text(name).clicked() {
                app.set_context(COLOUR, name);
            }
        }
    });
}

fn navigator(ui: &mut Ui, _app: &AppHandle) {
    let t = Tokens::of(ui.ctx());
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, width * 0.62), Sense::hover());
    let painter = ui.painter_at(rect);
    image(&painter, rect);
    // The part of the image the canvas shows.
    let view =
        egui::Rect::from_min_size(rect.min + rect.size() * vec2(0.3, 0.2), rect.size() * 0.45);
    painter.rect_stroke(
        view,
        0.0,
        egui::Stroke::new(2.0, t.accent.default),
        egui::StrokeKind::Inside,
    );
    ui.label("Zoom: 220 %");
}
