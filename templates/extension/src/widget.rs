//! The tally widget: a minus button, the count, a plus button.

use egui_pop::api::AppHandle;
use egui_pop::core::a11y;
use egui_pop::core::icon::Icon;
use egui_pop::egui::{self, Align2, Response, Role, Sense, Stroke, StrokeKind, Ui};

use crate::strings;
use crate::tokens::TallyTokens;
use crate::{TALLIES, change};

/// A tally with a name, drawn from [`TallyTokens`]. Each click runs
/// [`crate::STEP_CHANGE`]; the count shown is the plugin's
/// ([`crate::Tallies`]).
#[derive(Clone, Debug)]
pub struct Tally<'a> {
    name: &'a str,
    label: &'a str,
}

impl<'a> Tally<'a> {
    /// `name` is the tally's stable id (commands and steps use it);
    /// `label` is what people read.
    pub fn new(name: &'a str, label: &'a str) -> Self {
        Self { name, label }
    }

    /// Draws it; the response is the count's.
    pub fn show(self, ui: &mut Ui, app: &AppHandle) -> Response {
        let Some(tallies) = app.service(&TALLIES) else {
            // No silent no-op (ADR 0003): say why nothing is drawn.
            return ui.label(strings::tr(ui.ctx(), &strings::MISSING, &[]));
        };
        let count = tallies.get(self.name);
        let t = TallyTokens::of(ui.ctx());
        let ctx = ui.ctx().clone();
        let count_text = count.to_string();
        let args = [("name", self.label), ("count", count_text.as_str())];
        ui.push_id(("tally", self.name), |ui| {
            ui.horizontal(|ui| {
                let less = strings::tr(&ctx, &strings::DECREASE, &args);
                if ui.add(Icon::Minus.label(&less).sense_click()).clicked() {
                    let _ = change(app, self.name, count - 1, "decrease");
                }

                let text = strings::tr(&ctx, &strings::VALUE, &args);
                let galley = ui.painter().layout_no_wrap(
                    text.clone(),
                    egui::TextStyle::Body.resolve(ui.style()),
                    t.text,
                );
                let size = egui::vec2(galley.size().x + 2.0 * t.padding, t.height);
                let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
                if ui.is_rect_visible(rect) {
                    let p = ui.painter();
                    p.rect_filled(rect, t.radius, t.background);
                    p.rect_stroke(
                        rect,
                        t.radius,
                        Stroke::new(1.0, t.border),
                        StrokeKind::Inside,
                    );
                    if count > 0 {
                        let bar = egui::Rect::from_min_max(
                            egui::pos2(rect.left() + t.padding, rect.bottom() - 3.0),
                            egui::pos2(rect.right() - t.padding, rect.bottom() - 1.0),
                        );
                        p.rect_filled(bar, 1.0, t.accent);
                    }
                    p.galley(
                        Align2::CENTER_CENTER
                            .align_size_within_rect(galley.size(), rect)
                            .min,
                        galley,
                        t.text,
                    );
                }
                a11y::name(&response, Role::Label, &text);

                let more = strings::tr(&ctx, &strings::INCREASE, &args);
                if ui.add(Icon::Plus.label(&more).sense_click()).clicked() {
                    let _ = change(app, self.name, count + 1, "increase");
                }
                response
            })
            .inner
        })
        .inner
    }
}
