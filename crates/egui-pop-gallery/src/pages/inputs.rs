//! The inputs page: the decorated input with every decorator
//! (placeholder, slots, clear, attached and split buttons, a dropdown),
//! every field state on inputs and sliders, and Tab order: a form whose
//! columns read row by row, a group, two panels F6 moves between, and a
//! form in a modal where Tab cycles.

use egui::Ui;
use egui_pop::api::stories::Story;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::core::binding::{self, Binding};
use egui_pop::core::icon::Icon;
use egui_pop::core::input::{self, Attached, Input};
use egui_pop::core::lock;
use egui_pop::core::overlay::{self, Kind, OverlaySpec};
use egui_pop::core::slider::{RangeSlider, Slider};
use egui_pop::core::tab::{self, ScopeKind, TabStop};
use egui_pop::model::field::{Baseline, FieldState};
use egui_pop::model::ids::{PluginId, StoryId};
use egui_pop::model::values::Descriptor;

use super::section;
use crate::shell::SHELL;

pub const INPUTS: PluginId = PluginId::from_static("gallery.inputs");

/// The store path the caption field is bound to (clearing it is one
/// undo step).
pub const CAPTION: &str = "gallery/inputs/caption";

/// The widest the page's inputs get.
const WIDTH: f32 = 250.0;

/// The width of a column's inputs: what its labels leave, at most
/// [`WIDTH`].
fn input_width(ui: &Ui) -> f32 {
    (ui.available_width() - 5.0 * ui.text_style_height(&egui::TextStyle::Body)).min(WIDTH)
}

#[derive(Debug)]
struct State {
    query: String,
    site: String,
    folder: String,
    size: f64,
    format: String,
    export: String,
    exports: u32,
    modified: String,
    required: String,
    invalid: String,
    disabled: String,
    read_only: String,
    pending: String,
    exposure: f64,
    locked: f64,
    off: f64,
    iso: (f64, f64),
    street: String,
    city: String,
    zip: String,
    country: String,
    quality: String,
    tags: String,
    modal: bool,
    title: String,
    author: String,
    answered: String,
}

impl Default for State {
    fn default() -> Self {
        Self {
            query: "harbour".into(),
            site: "example.org".into(),
            folder: "/photos/2026".into(),
            size: 2048.0,
            format: "JPEG".into(),
            export: "Holiday".into(),
            exports: 0,
            modified: "Golden hour".into(),
            required: String::new(),
            invalid: "ab".into(),
            disabled: "IMG_0042".into(),
            read_only: "Canon EOS R6".into(),
            pending: "holiday-2026".into(),
            exposure: 0.7,
            locked: 5600.0,
            off: 0.0,
            iso: (100.0, 3200.0),
            street: String::new(),
            city: String::new(),
            zip: String::new(),
            country: String::new(),
            quality: "90".into(),
            tags: String::new(),
            modal: false,
            title: String::new(),
            author: String::new(),
            answered: "nothing yet".into(),
        }
    }
}

/// The inputs page.
#[derive(Debug, Default)]
pub struct InputsPage;

impl Plugin for InputsPage {
    fn id(&self) -> PluginId {
        INPUTS
    }

    fn title(&self) -> String {
        "Inputs".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let owner = ctx.owner().clone();
        {
            let store = ctx.app().store();
            let mut s = lock(&store);
            s.declare(&owner, CAPTION, Descriptor::text("Caption", "Sunset"))
                .map_err(|e| PluginError::Failed(e.to_string()))?;
        }
        let state = std::rc::Rc::new(std::cell::RefCell::new(State::default()));
        ctx.story(
            Story::new(StoryId::from_static("inputs"), "Inputs", move |ui, app| {
                show(ui, app, &mut state.borrow_mut())
            })
            .description("The decorated input (placeholder, icons, prefix and suffix, clear, attached and split buttons, a dropdown), the field states on inputs and sliders, and Tab order: reading order, groups, panels (F6) and a modal.")
            .group("Components", 47),
        )?;
        Ok(())
    }
}

/// A label and its input, one row of a two-column grid.
fn row(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui)) {
    ui.label(label);
    add(ui);
    ui.end_row();
}

fn show(ui: &mut Ui, app: &AppHandle, s: &mut State) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.columns(2, |cols| {
                decorators(&mut cols[0], app, s);
                states(&mut cols[1], s);
            });
            tab_order(ui, s);
        });
    modal(ui, s);
}

fn decorators(ui: &mut Ui, app: &AppHandle, s: &mut State) {
    let width = input_width(ui);
    section(ui, "Decorators");
    egui::Grid::new("inputs-decorators")
        .num_columns(2)
        .show(ui, |ui| {
            row(ui, "Search", |ui| {
                Input::search(&mut s.query, "Search")
                    .placeholder("Search photos")
                    .width(width)
                    .show(ui);
            });
            row(ui, "Site", |ui| {
                Input::text(&mut s.site, "Site")
                    .prefix("https://")
                    .suffix("/gallery")
                    .clearable(true)
                    .width(width)
                    .show(ui);
            });
            row(ui, "Folder", |ui| {
                let out = Input::path(&mut s.folder, "Folder")
                    .leading_icon(Icon::Folder)
                    .button(Attached::text("Browse…", "Browse for a folder"))
                    .width(width)
                    .show(ui);
                if out.button == Some(0) {
                    s.folder = "/photos/2026/chosen".into();
                }
            });
            row(ui, "Long edge", |ui| {
                Input::number(&mut s.size, "Long edge")
                    .unit("px")
                    .choices(["1024", "2048", "4096"])
                    .width(width)
                    .show(ui);
            });
            row(ui, "Format", |ui| {
                Input::text(&mut s.format, "Format")
                    .choices(["JPEG", "PNG", "TIFF", "AVIF"])
                    .width(width)
                    .show(ui);
            });
            row(ui, "Export", |ui| {
                let out = Input::text(&mut s.export, "Export name")
                    .placeholder("File name")
                    .button(
                        Attached::icon(Icon::Save, "Export")
                            .menu(["Export as…", "Export settings…"]),
                    )
                    .width(width)
                    .show(ui);
                if out.button.is_some() || out.menu.is_some() {
                    s.exports += 1;
                }
            });
            row(ui, "Caption", |ui| {
                let store = app.store();
                binding::edit(ui, Binding::bound(&store, CAPTION), |ui, v: &mut String| {
                    Input::text(v, "Caption")
                        .clearable(true)
                        .placeholder("No caption")
                        .width(width)
                        .show(ui)
                        .response
                });
            });
            row(ui, "Split button", |ui| {
                let out =
                    input::split_button(ui, "Publish", &["Publish to album", "Publish to web"]);
                if out.clicked || out.chosen.is_some() {
                    s.exports += 1;
                }
            });
        });
    ui.label(format!("Exports asked: {}", s.exports));
}

fn states(ui: &mut Ui, s: &mut State) {
    let width = input_width(ui);
    section(ui, "States");
    egui::Grid::new("inputs-states")
        .num_columns(2)
        .show(ui, |ui| {
            row(ui, "Modified", |ui| {
                let state = FieldState::new().compare(&s.modified, None, &"Sunset".to_string());
                Input::text(&mut s.modified, "Title")
                    .state(state)
                    .default_value("Sunset")
                    .width(width)
                    .show(ui);
            });
            row(ui, "Required", |ui| {
                let mut state = FieldState::new().required();
                if s.required.is_empty() {
                    state = state.invalid("A keyword is required");
                }
                Input::text(&mut s.required, "Keyword")
                    .placeholder("Type a keyword")
                    .state(state)
                    .width(width)
                    .show(ui);
            });
            row(ui, "Invalid", |ui| {
                let message =
                    (s.invalid.chars().count() < 3).then(|| "At least three characters".into());
                Input::text(&mut s.invalid, "Album")
                    .state(FieldState::new().validated(message))
                    .width(width)
                    .show(ui);
            });
            row(ui, "Disabled", |ui| {
                Input::text(&mut s.disabled, "Image")
                    .state(FieldState::new().disabled("Needs an open image"))
                    .width(width)
                    .show(ui);
            });
            row(ui, "Read-only", |ui| {
                Input::text(&mut s.read_only, "Camera")
                    .state(FieldState::new().read_only())
                    .width(width)
                    .show(ui);
            });
            row(ui, "Pending", |ui| {
                Input::text(&mut s.pending, "Collection")
                    .state(FieldState::new().pending())
                    .width(width)
                    .show(ui);
            });
        });
    section(ui, "Sliders");
    ui.add(
        Slider::new(&mut s.exposure, "Exposure", -3.0..=4.0)
            .step(0.01)
            .unit("EV")
            .default(0.0)
            .state(FieldState::new().modified(Baseline::Saved)),
    );
    ui.add(
        Slider::new(&mut s.locked, "White balance", 2000.0..=12000.0)
            .step(10.0)
            .integer()
            .unit("K")
            .state(FieldState::new().read_only()),
    );
    ui.add(
        Slider::new(&mut s.off, "Denoise", 0.0..=1.0)
            .step(0.01)
            .state(FieldState::new().disabled("Only for raw files")),
    );
    ui.add(
        RangeSlider::new(&mut s.iso.0, &mut s.iso.1, "ISO", 50.0..=25600.0)
            .logarithmic()
            .integer()
            .state(FieldState::new().required()),
    );
}

fn tab_order(ui: &mut Ui, s: &mut State) {
    section(ui, "Tab order");
    ui.label("Tab follows reading order inside each panel: row by row, though each column is drawn as a whole. The tags and quality form a group. F6 moves between the two panels.");
    ui.horizontal_top(|ui| {
        tab::scope(ui, "inputs-address", ScopeKind::Panel, |ui| {
            ui.vertical(|ui| {
                ui.strong("Address");
                // Drawn column by column; read row by row.
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        Input::text(&mut s.street, "Street")
                            .placeholder("Street")
                            .width(180.0)
                            .show(ui);
                        Input::text(&mut s.zip, "Postcode")
                            .placeholder("Postcode")
                            .width(180.0)
                            .show(ui);
                    });
                    ui.vertical(|ui| {
                        Input::text(&mut s.city, "City")
                            .placeholder("City")
                            .width(180.0)
                            .show(ui);
                        Input::text(&mut s.country, "Country")
                            .placeholder("Country")
                            .width(180.0)
                            .show(ui);
                    });
                });
            });
        });
        ui.add_space(24.0);
        tab::scope(ui, "inputs-options", ScopeKind::Panel, |ui| {
            ui.vertical(|ui| {
                ui.strong("Options");
                Input::text(&mut s.tags, "Tags")
                    .placeholder("Tags")
                    .tab(TabStop::new().group("export"))
                    .width(180.0)
                    .show(ui);
                Input::number(&mut s.size, "Size")
                    .unit("px")
                    .id_salt("options-size")
                    .width(180.0)
                    .show(ui);
                Input::text(&mut s.quality, "Quality")
                    .unit("%")
                    .tab(TabStop::new().group("export"))
                    .width(180.0)
                    .show(ui);
                let r = ui.button("Open the form in a modal");
                tab::stop(ui, &r, TabStop::new());
                if r.clicked() {
                    s.modal = true;
                }
            });
        });
    });
    ui.label(format!("Modal answered: {}", s.answered));
}

fn modal(ui: &mut Ui, s: &mut State) {
    let ctx = ui.ctx().clone();
    let spec = OverlaySpec::new("gallery.inputs.modal", "A form in a modal", Kind::Modal);
    let mut open = s.modal;
    let mut answer = None;
    overlay::show(&ctx, &spec, &mut open, None, |ui| {
        tab::scope(ui, "inputs-modal", ScopeKind::Modal, |ui| {
            ui.heading("A form in a modal");
            ui.label("Tab cycles inside: after Cancel comes the title again.");
            Input::text(&mut s.title, "Book title")
                .placeholder("Title")
                .state(FieldState::new().required())
                .show(ui);
            Input::text(&mut s.author, "Author")
                .placeholder("Author")
                .show(ui);
            ui.horizontal(|ui| {
                let ok = ui.button("OK");
                tab::stop(ui, &ok, TabStop::new());
                let cancel = ui.button("Cancel");
                tab::stop(ui, &cancel, TabStop::new());
                if ok.clicked() {
                    answer = Some(format!("OK, {}", s.title));
                }
                if cancel.clicked() {
                    answer = Some("cancelled".into());
                }
            });
        });
    });
    if let Some(a) = answer {
        s.answered = a;
        open = false;
    }
    s.modal = open;
}
