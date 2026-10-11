//! Icons by role: every [`Icon`] with its name, from the icon font or
//! painted, in a size and a tone.

use egui::{Sense, Ui, vec2};
use egui_pop::core::icon::{self, Icon, Size, Tone};
use egui_pop::theme::Tokens;

use super::section;

/// The playground's options.
#[derive(Clone, Debug, Default)]
pub struct State {
    pub size: Size,
    pub tone: Tone,
    /// Show the painted fallbacks next to the font's glyphs.
    pub compare: bool,
}

const SIZES: [(Size, &str); 3] = [
    (Size::Inline, "Inline"),
    (Size::Toolbar, "Toolbar"),
    (Size::Panel, "Panel"),
];

const TONES: [(Tone, &str); 9] = [
    (Tone::Default, "Default"),
    (Tone::Muted, "Muted"),
    (Tone::Strong, "Strong"),
    (Tone::Accent, "Accent"),
    (Tone::OnAccent, "On accent"),
    (Tone::Danger, "Danger"),
    (Tone::Warning, "Warning"),
    (Tone::Success, "Success"),
    (Tone::Info, "Info"),
];

pub fn playground(ui: &mut Ui, state: &mut State) {
    let label = ui.label("Size");
    egui::ComboBox::from_id_salt("icons.size")
        .selected_text(SIZES.iter().find(|s| s.0 == state.size).map_or("", |s| s.1))
        .show_ui(ui, |ui| {
            for (s, name) in SIZES {
                ui.selectable_value(&mut state.size, s, name);
            }
        })
        .response
        .labelled_by(label.id);
    let label = ui.label("Tone");
    egui::ComboBox::from_id_salt("icons.tone")
        .selected_text(TONES.iter().find(|s| s.0 == state.tone).map_or("", |s| s.1))
        .show_ui(ui, |ui| {
            for (s, name) in TONES {
                ui.selectable_value(&mut state.tone, s, name);
            }
        })
        .response
        .labelled_by(label.id);
    ui.checkbox(&mut state.compare, "Show painted fallbacks");
}

pub fn show(ui: &mut Ui, state: &mut State) {
    let t = Tokens::of(ui.ctx());
    ui.label(if icon::font_ready(ui.ctx()) {
        "Drawn from the Phosphor icon font (feature `phosphor`); dot and diamond are always painted."
    } else {
        "Painted: the icon font is not installed (feature `phosphor` off)."
    });
    section(ui, "Every icon");
    let side = state.size.points(&t);
    let cell = vec2(side.max(t.size.icon_toolbar) * 6.5, side + t.space.m);
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Grid::new("icons.grid")
                .num_columns(4)
                .spacing([t.space.l, t.space.s])
                .show(ui, |ui| {
                    for (i, &icon) in Icon::ALL.iter().enumerate() {
                        ui.allocate_ui_with_layout(
                            cell,
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.add(icon.size(state.size).tone(state.tone).label(icon.name()));
                                if state.compare {
                                    let (rect, _) =
                                        ui.allocate_exact_size(vec2(side, side), Sense::hover());
                                    icon::paint_fallback(
                                        ui.painter(),
                                        icon,
                                        rect,
                                        state.tone.color(&t),
                                    );
                                }
                                ui.weak(icon.name());
                            },
                        );
                        if i % 4 == 3 {
                            ui.end_row();
                        }
                    }
                });
        });
}
