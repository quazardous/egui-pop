//! The scale page: the window's scales, the interface zoom in steps,
//! crisp lines at fractional scales, and the scale-changed signal.

use egui::{Sense, Stroke, Ui, pos2, vec2};
use egui_pop::api::stories::Story;
use egui_pop::api::zoom;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::core::scale::{self, Scale};
use egui_pop::core::widgets;
use egui_pop::model::ids::{PluginId, StoryId};
use egui_pop::theme::Tokens;

use super::section;
use crate::shell::SHELL;

pub const SCALE_PAGE: PluginId = PluginId::from_static("gallery.scale");

/// The scale page.
#[derive(Debug, Default)]
pub struct ScalePage;

impl Plugin for ScalePage {
    fn id(&self) -> PluginId {
        SCALE_PAGE
    }

    fn title(&self) -> String {
        "Scale".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.story(
            Story::new(StoryId::from_static("scale"), "Scale and zoom", show)
                .description("The window's native scale, the interface zoom in fixed steps (Ctrl+Alt+= and Ctrl+Alt+-), lines that stay one pixel wide, and the signal sent when a window's scale changes.")
                .group("Framework", 106),
        )?;
        Ok(())
    }
}

fn show(ui: &mut Ui, app: &AppHandle) {
    let t = Tokens::of(ui.ctx());
    let s = Scale::of(ui.ctx());
    egui::Grid::new("gallery.scale.grid")
        .num_columns(2)
        .spacing([t.space.l, t.space.xs])
        .show(ui, |ui| {
            ui.label("Native scale");
            ui.monospace(format!("{:.2}", s.native));
            ui.end_row();
            ui.label("Interface zoom");
            ui.monospace(zoom::label(s.zoom));
            ui.end_row();
            ui.label("Pixels per point");
            ui.monospace(format!("{:.2}", s.pixels_per_point));
            ui.end_row();
            ui.label("One pixel, in points");
            ui.monospace(format!("{:.3}", scale::hairline(ui.ctx())));
            ui.end_row();
        });

    section(ui, "Interface zoom");
    ui.horizontal_wrapped(|ui| {
        let now = zoom::current(app);
        for step in scale::ZOOM_STEPS {
            if widgets::selectable_label(ui, (step - now).abs() < 0.001, zoom::label(step))
                .clicked()
            {
                zoom::set(app, step);
            }
        }
    });
    ui.weak("Ctrl+Alt+= and Ctrl+Alt+- move it a step, Ctrl+Alt+0 resets it; egui's own Ctrl+= and Ctrl+- are left to the application's content.");

    section(ui, "Lines");
    let (rect, _) = ui.allocate_exact_size(vec2(360.0, 56.0), Sense::hover());
    let painter = ui.painter_at(rect);
    let ink = t.text.default;
    for (i, (label, w)) in [
        ("1 pt as is", 1.0),
        ("1 pt snapped", scale::stroke(ui.ctx(), 1.0)),
    ]
    .into_iter()
    .enumerate()
    {
        let x0 = rect.left() + 8.0 + i as f32 * 180.0;
        for k in 0..8 {
            let x = x0 + 4.0 + k as f32 * 3.3;
            painter.line_segment(
                [pos2(x, rect.top() + 4.0), pos2(x, rect.top() + 32.0)],
                Stroke::new(w, ink),
            );
        }
        painter.text(
            pos2(x0, rect.top() + 36.0),
            egui::Align2::LEFT_TOP,
            label,
            egui::FontId::proportional(t.font.small),
            t.text.muted,
        );
    }

    section(ui, "Scale changes");
    let changes: Vec<String> = app
        .events_since(app.last_event().saturating_sub(64))
        .into_iter()
        .filter(|e| e.name == "scale.changed")
        .rev()
        .take(4)
        .map(|e| format!("#{} {}", e.seq, e.data))
        .collect();
    if changes.is_empty() {
        ui.weak("None yet: change the zoom, or move the window to another screen.");
    }
    for c in changes {
        ui.monospace(c);
    }
}
