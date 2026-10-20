//! The layout by role page: the `UiPop` helpers (text roles, spacing by
//! intent, cards and buttons by tone), each with the tokens it reads.

use egui::Ui;
use egui_pop::core::ui::{Space, TextRole, Tone, UiPop};
use egui_pop::theme::Tokens;

/// The tones shown, each by its name.
const TONES: [Tone; 8] = [
    Tone::Neutral,
    Tone::Accent,
    Tone::Info,
    Tone::Success,
    Tone::Warning,
    Tone::Danger,
    Tone::Muted,
    Tone::Record,
];

/// What the buttons did.
#[derive(Debug, Default)]
pub struct State {
    last: Option<Tone>,
}

pub fn show(ui: &mut Ui, state: &mut State) {
    let t = Tokens::of(ui.ctx());
    ui.muted("Layout code says what a piece of text is and why there is space; the helpers draw from the theme's text roles, spacing by intent and kit.");
    ui.space_role(Space::Section);
    ui.section_title("Text roles");
    ui.space_role(Space::Inline);
    egui::Grid::new("roles.text")
        .num_columns(2)
        .spacing([t.space.group, t.space.inline])
        .show(ui, |ui| {
            for role in TextRole::ALL {
                ui.monospace(format!("text-role.{}", role.name()));
                ui.text_role(role, sample(role));
                ui.end_row();
            }
        });

    ui.space_role(Space::Section);
    ui.section_title("Spacing by intent");
    ui.space_role(Space::Inline);
    egui::Grid::new("roles.space")
        .num_columns(3)
        .spacing([t.space.group, t.space.inline])
        .show(ui, |ui| {
            for space in Space::ALL {
                let points = space.points(&t);
                ui.monospace(space.token());
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(points, t.space.group), egui::Sense::hover());
                ui.painter().rect_filled(rect, 0.0, t.accent.default);
                ui.meta(format!("{points} pt"));
                ui.end_row();
            }
        });

    ui.space_role(Space::Section);
    ui.section_title("Cards by tone");
    ui.space_role(Space::Inline);
    for row in TONES.chunks(4) {
        ui.horizontal(|ui| {
            for &tone in row {
                ui.card(tone, |ui| {
                    ui.vertical(|ui| {
                        ui.set_width(120.0);
                        ui.label(format!("Card: {}", tone.name()));
                        ui.meta("Content in a card");
                    });
                });
            }
        });
        ui.space_role(Space::Inline);
    }

    ui.space_role(Space::Section);
    ui.section_title("Buttons by tone");
    ui.space_role(Space::Inline);
    ui.horizontal_wrapped(|ui| {
        for tone in TONES {
            if ui.button_tone(tone, tone.name()).clicked() {
                state.last = Some(tone);
            }
        }
    });
    ui.space_role(Space::Group);
    match state.last {
        Some(tone) => ui.meta(format!("Last clicked: {}", tone.name())),
        None => ui.meta("Nothing clicked yet."),
    };
}

fn sample(role: TextRole) -> &'static str {
    match role {
        TextRole::Title => "Export the selection",
        TextRole::Section => "Output",
        TextRole::Body => "Writes a copy of each image next to the original.",
        TextRole::Muted => "The original is never changed.",
        TextRole::Meta => "24 MB, 12 October, 18:42",
    }
}
