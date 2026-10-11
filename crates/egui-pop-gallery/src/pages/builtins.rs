//! egui's own widgets, drawn with the theme and no per-widget code.

use egui::Ui;

use super::section;

/// The widgets' values and the playground's options.
#[derive(Clone, Debug)]
pub struct State {
    /// Draw the widgets enabled (else disabled).
    pub enabled: bool,
    pub checked: bool,
    pub radio: usize,
    pub selected: usize,
    pub value: f32,
    pub count: i32,
    pub progress: f32,
    pub text: String,
    pub notes: String,
    pub choice: usize,
}

impl Default for State {
    fn default() -> Self {
        State {
            enabled: true,
            checked: true,
            radio: 1,
            selected: 0,
            value: 40.0,
            count: 7,
            progress: 0.6,
            text: "Some text".into(),
            notes: "Several lines\nof text".into(),
            choice: 0,
        }
    }
}

const CHOICES: [&str; 3] = ["First", "Second", "Third"];

pub fn playground(ui: &mut Ui, state: &mut State) {
    ui.checkbox(&mut state.enabled, "Enabled");
    ui.add(egui::Slider::new(&mut state.progress, 0.0..=1.0).text("Progress"));
    if ui.button("Reset values").clicked() {
        *state = State {
            enabled: state.enabled,
            ..State::default()
        };
    }
}

pub fn show(ui: &mut Ui, state: &mut State) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_enabled_ui(state.enabled, |ui| widgets(ui, state));
        });
}

fn widgets(ui: &mut Ui, state: &mut State) {
    section(ui, "Buttons");
    ui.horizontal_wrapped(|ui| {
        let _ = ui.button("Button");
        ui.add(egui::Button::new("Selected").selected(true));
        let _ = ui.small_button("Small");
        let _ = ui.link("Link");
        ui.hyperlink_to("Hyperlink", "https://github.com/quazardous/egui-pop");
    });

    section(ui, "Toggles");
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut state.checked, "Checkbox");
        for (i, name) in CHOICES.iter().enumerate() {
            ui.radio_value(&mut state.radio, i, *name);
        }
    });
    ui.horizontal_wrapped(|ui| {
        for (i, name) in CHOICES.iter().enumerate() {
            ui.selectable_value(&mut state.selected, i, *name);
        }
    });

    section(ui, "Values");
    ui.add(egui::Slider::new(&mut state.value, 0.0..=100.0).text("Slider"));
    ui.horizontal(|ui| {
        let value = ui.add(egui::DragValue::new(&mut state.count).range(0..=99));
        let label = ui.label("Drag value");
        value.labelled_by(label.id);
    });
    ui.add(egui::ProgressBar::new(state.progress).show_percentage());

    section(ui, "Text input");
    let label = ui.label("One line");
    ui.add(egui::TextEdit::singleline(&mut state.text).hint_text("Type here"))
        .labelled_by(label.id);
    let label = ui.label("Several lines");
    ui.add(egui::TextEdit::multiline(&mut state.notes).desired_rows(3))
        .labelled_by(label.id);

    section(ui, "Choice");
    egui::ComboBox::new("builtins.combo", "Combo box")
        .selected_text(CHOICES[state.choice])
        .show_ui(ui, |ui| {
            for (i, name) in CHOICES.iter().enumerate() {
                ui.selectable_value(&mut state.choice, i, *name);
            }
        });

    section(ui, "Containers");
    egui::CollapsingHeader::new("Collapsing header")
        .default_open(true)
        .show(ui, |ui| {
            ui.label("Content inside a collapsing header.");
        });
    ui.group(|ui| {
        ui.label("A group frame");
    });

    section(ui, "Labels");
    ui.heading("Heading");
    ui.label("Label");
    ui.strong("Strong");
    ui.weak("Weak");
    ui.monospace("Monospace");
    ui.small("Small");
    let (warn, error) = (ui.visuals().warn_fg_color, ui.visuals().error_fg_color);
    ui.colored_label(warn, "Warning");
    ui.colored_label(error, "Error");
}
