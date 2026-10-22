//! egui's own widgets restyled by the kit in a scope (`UiPop::scoped`):
//! egui still draws them, and a component's states become egui's widget
//! visuals for the widgets added in the scope; and the scroll bars,
//! whose width, margin, placement and visibility every scroll area takes
//! from the theme, their colours from a scope (`UiPop::scroll_area`).

use egui::Ui;
use egui_pop::core::ui::{Stock, Tone, UiPop};
use egui_pop::core::widgets;

use super::section;

/// The fields' tones, each with what it says.
const FIELDS: [(Tone, &str); 5] = [
    (Tone::Neutral, "Plain"),
    (Tone::Accent, "Changed"),
    (Tone::Warning, "Required"),
    (Tone::Danger, "Refused"),
    (Tone::Info, "Being checked"),
];

const ZOOMS: [&str; 3] = ["Fit", "100 %", "200 %"];
const MODES: [&str; 3] = ["Fast", "Balanced", "Exact"];

/// The widgets' values.
#[derive(Debug)]
pub struct State {
    zoom: usize,
    mode: usize,
    texts: [String; FIELDS.len()],
    last: Option<Tone>,
}

impl Default for State {
    fn default() -> Self {
        State {
            zoom: 0,
            mode: 1,
            texts: FIELDS.map(|(_, name)| name.to_lowercase()),
            last: None,
        }
    }
}

pub fn show(ui: &mut Ui, state: &mut State) {
    ui.muted("egui draws these widgets; a scope gives them the kit's states for one component, so a kit restyles them with data only.");

    section(ui, "Buttons by tone (button)");
    ui.horizontal_wrapped(|ui| {
        for tone in [Tone::Neutral, Tone::Accent, Tone::Danger, Tone::Muted] {
            if ui.button_tone(tone, tone.name()).clicked() {
                state.last = Some(tone);
            }
        }
        ui.add_enabled_ui(false, |ui| {
            ui.scoped(Stock::Button, Tone::Neutral, |ui| ui.button("Disabled"));
        });
    });
    match state.last {
        Some(tone) => ui.meta(format!("Last clicked: {}", tone.name())),
        None => ui.meta("Nothing clicked yet."),
    };

    section(ui, "On or off (toggle-button)");
    ui.scoped(Stock::ToggleButton, Tone::Neutral, |ui| {
        ui.horizontal_wrapped(|ui| {
            for (i, name) in ZOOMS.iter().enumerate() {
                if ui
                    .add(egui::Button::new(*name).selected(state.zoom == i))
                    .clicked()
                {
                    state.zoom = i;
                }
            }
        });
    });

    section(ui, "Combo box (combo-box)");
    let combo = widgets::combo_box_labelled("scoped.mode", "Mode").selected_text(MODES[state.mode]);
    ui.combo_box(combo, |ui| {
        for (i, name) in MODES.iter().enumerate() {
            widgets::selectable_value(ui, &mut state.mode, i, *name);
        }
    });

    section(ui, "Text fields by tone (field)");
    egui::Grid::new("scoped.fields")
        .num_columns(2)
        .show(ui, |ui| {
            for ((tone, name), text) in FIELDS.iter().zip(&mut state.texts) {
                let label = ui.label(*name);
                ui.scoped(Stock::Field, *tone, |ui| {
                    ui.text_edit_singleline(text).labelled_by(label.id);
                });
                ui.end_row();
            }
        });

    section(ui, "Scroll bars (scroll-bar)");
    ui.muted("Every scroll area takes the bar's width, margin, placement and visibility from the theme; this one also its thumb's colours and corners and its track.");
    let area = egui::ScrollArea::vertical()
        .id_salt("scoped.scroll")
        .max_height(96.0)
        .auto_shrink([false, true]);
    ui.scroll_area(area, |ui| {
        for row in 1..=24 {
            ui.label(format!("Row {row}"));
        }
    });
}
