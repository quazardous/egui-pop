//! The layout by role page: the `UiPop` helpers (text roles, spacing by
//! intent, code, text in a tone and dividers, cards and buttons by tone,
//! rows and grids, fixed sizes), each with the
//! tokens it reads.

use egui::Ui;
use egui_pop::core::ui::{Align, Grid, Length, Row, Space, TextRole, Tone, Track, UiPop};
use egui_pop::core::widgets::SelectableRow;
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

/// What the buttons did, and the rows' field.
#[derive(Debug)]
pub struct State {
    last: Option<Tone>,
    name: String,
    /// The fixed-size rows' choice.
    shown: usize,
}

impl Default for State {
    fn default() -> Self {
        Self {
            last: None,
            name: "IMG_0001.raw".to_owned(),
            shown: 0,
        }
    }
}

pub fn show(ui: &mut Ui, state: &mut State) {
    let t = Tokens::of(ui.ctx());
    ui.muted("Layout code says what a piece of text is and why there is space; the helpers draw from the theme's text roles, spacing by intent and kit.");
    rows_and_grids(ui, state);
    fixed_sizes(ui, state);
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
            // Inline emphasis: not a `TextRole`, its own helper.
            ui.monospace("text-role.strong");
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.label("Writes ");
                ui.strong("one copy");
                ui.label(" of each image.");
            });
            ui.end_row();
        });

    ui.space_role(Space::Section);
    ui.section_title("Code, tones and dividers");
    ui.space_role(Space::Inline);
    ui.code_text("12:00:01  import  IMG_0001.raw  read 24 MB");
    ui.divider();
    ui.horizontal_wrapped(|ui| {
        for tone in [
            Tone::Danger,
            Tone::Warning,
            Tone::Success,
            Tone::Info,
            Tone::Muted,
        ] {
            ui.text_tone(tone, format!("Text: {}", tone.name()));
            ui.divider();
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

/// `ui.row` and `ui.grid`: tracks, one baseline, gaps by intent.
fn rows_and_grids(ui: &mut Ui, state: &mut State) {
    ui.space_role(Space::Section);
    ui.section_title("Rows and grids");
    ui.muted("Cells on tracks (content, fractions, em, percent, with bounds), Space apart, every text on one baseline.");
    ui.space_role(Space::Group);

    // A title, a field between 8 and 24 em taking what is left, a button,
    // small print: four text sizes on one baseline.
    let field = Track::fr(1.0).min(Length::Em(8.0)).max(Length::Em(24.0));
    let tracks = [Track::CONTENT, field, Track::CONTENT, Track::CONTENT];
    ui.row(Row::new(tracks), |r| {
        let title = r.cell(|ui| ui.section_title("Name")).id;
        r.cell(|ui| {
            egui::TextEdit::singleline(&mut state.name)
                .desired_width(f32::INFINITY)
                .show(ui)
                .response
                .response
                .labelled_by(title)
        });
        r.cell(|ui| ui.button("Rename"));
        r.cell(|ui| ui.meta("24 MB"));
    });
    ui.space_role(Space::Group);

    // A header/value grid, and the same grid in a narrow column, folded
    // to headers over values.
    let details = |ui: &mut Ui, id: &str| {
        let grid = Grid::new(id, [Track::CONTENT, Track::fr(1.0)]).fold_below(Length::Em(24.0));
        ui.grid(grid, |g| {
            g.header("Kind");
            g.cell(|ui| ui.label("Raw image"));
            g.header("Taken");
            g.cell(|ui| ui.label("12 October, 18:42"));
            g.header("Rating");
            g.cell(|ui| ui.button("Rate"));
        });
    };
    ui.row(
        Row::new([Track::fr(2.0), Track::fr(1.0)])
            .gap(Space::Page)
            .align(Align::Start),
        |r| {
            r.cell(|ui| details(ui, "roles.details"));
            r.cell(|ui| details(ui, "roles.details.narrow"));
        },
    );
    ui.space_role(Space::Group);

    // A button bar: buttons as wide as the widest, at the end.
    let bar = Row::new([Track::CONTENT; 3])
        .justify(Align::End)
        .equal_widths();
    ui.row(bar, |r| {
        r.cell(|ui| ui.button("Cancel"));
        r.cell(|ui| ui.button("Apply"));
        r.cell(|ui| ui.button_tone(Tone::Accent, "Export all"));
    });
}

/// `ui.sized`, `ui.sized_ui` and `ui.rect_ui`: a widget, content, and
/// content in a rectangle, each in a box of a set size.
fn fixed_sizes(ui: &mut Ui, state: &mut State) {
    let t = Tokens::of(ui.ctx());
    ui.space_role(Space::Section);
    ui.section_title("Fixed sizes");
    ui.muted("A widget filling a box of a set size (sized), content in one (sized_ui), content clipped to a rectangle (rect_ui).");
    ui.space_role(Space::Group);
    let row = egui::vec2(16.0 * t.space.group, t.size.row_height);
    for (i, name) in ["Show Exposure", "Show Contrast"].into_iter().enumerate() {
        if ui
            .sized(row, SelectableRow::new(state.shown == i, name))
            .clicked()
        {
            state.shown = i;
        }
    }
    ui.space_role(Space::Group);
    ui.horizontal(|ui| {
        let cell = egui::vec2(3.0 * t.space.group, t.size.row_height);
        for n in 1..=3 {
            ui.sized_ui(
                cell,
                egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                |ui| ui.muted(n.to_string()),
            );
        }
    });
    ui.space_role(Space::Group);
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(10.0 * t.space.group, t.size.row_height),
        egui::Sense::hover(),
    );
    ui.rect_ui(
        rect,
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| ui.label("A long line clipped to its rectangle, never past it"),
    );
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
