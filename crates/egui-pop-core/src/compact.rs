//! Controls for dense rows, where a control has a slot of a given size
//! and no label beside it (a HUD's parts, a property sheet, a card's
//! fold): a choice in a compact cell and a menu row. The scrub field is
//! the slider's [`scrub`](crate::slider::Slider::scrub) look, and a text
//! in a compact cell the decorated input's [`cell`](crate::input::Input::cell).
//!
//! - [`ChoiceCell`]: one of a few words in a slot, egui-pop's
//!   [`combo_box`](crate::widgets::combo_box) sized to it. Its name is
//!   given to assistive technologies and shown in its tooltip; while it
//!   has the focus and its list is closed, Up and Down choose the
//!   previous and next word, Home and End the first and last, as a
//!   native drop-down does; Space or Enter opens the list.
//! - [`MenuRow`]: a row the width of its room in the look of a menu's
//!   items (the kit's `menu-item`), its text at the start and a muted
//!   note at the end (a count, a key): an entry of a "More (n)" fold, a
//!   "Show all". A button to assistive technologies, focused by Tab and
//!   pressed with Space or Enter.
//!
//! ```
//! use egui_pop_core::compact::{ChoiceCell, MenuRow};
//!
//! fn row(ui: &mut egui::Ui, mode: &mut usize) {
//!     let size = egui::vec2(120.0, 22.0);
//!     ChoiceCell::new("Blend mode", mode, &["Normal", "Multiply", "Screen"])
//!         .size(size)
//!         .show(ui);
//!     if ui.add(MenuRow::new("Show Contrast").note("hidden")).clicked() {
//!         // reveal it
//!     }
//! }
//! ```

use egui::{Color32, Id, Key, Rect, Response, Role, Sense, Ui, Vec2, Widget, WidgetInfo, vec2};

use crate::kit::{Kit, KitCx, Part, State, Tone};
use crate::theme::Tokens;
use crate::widgets::{combo_box, selectable_value};

/// One of a few words in a compact cell (see [the module](self)).
#[derive(Debug)]
#[must_use = "draw it with `show`"]
pub struct ChoiceCell<'a, S> {
    name: &'a str,
    selected: &'a mut usize,
    choices: &'a [S],
    size: Option<Vec2>,
    salt: Option<Id>,
}

impl<'a, S: AsRef<str>> ChoiceCell<'a, S> {
    /// A cell choosing `selected` among `choices`, named `name`.
    pub fn new(name: &'a str, selected: &'a mut usize, choices: &'a [S]) -> Self {
        Self {
            name,
            selected,
            choices,
            size: None,
            salt: None,
        }
    }

    /// Its size; by default the available width and a row's height.
    pub fn size(mut self, size: Vec2) -> Self {
        self.size = Some(size);
        self
    }

    /// Distinguishes cells with the same name in one place.
    pub fn id_salt(mut self, salt: impl std::hash::Hash + std::fmt::Debug) -> Self {
        self.salt = Some(Id::unique(salt));
        self
    }

    /// Draws it; its response is marked changed when the choice changed.
    pub fn show(self, ui: &mut Ui) -> Response {
        let ChoiceCell {
            name,
            selected,
            choices,
            size,
            salt,
        } = self;
        let size = size.unwrap_or_else(|| vec2(ui.available_width(), ui.spacing().interact_size.y));
        let id = salt.unwrap_or_else(|| Id::unique(("egui_pop_choice_cell", name)));
        let word = |i: usize| choices.get(i).map_or("", AsRef::as_ref);
        let before = *selected;
        let pad = Tokens::of(ui.ctx()).space.xs;
        let mut response = ui
            .scope(|ui| {
                let spacing = ui.spacing_mut();
                spacing.interact_size.y = size.y;
                spacing.button_padding = vec2(pad, 0.0);
                combo_box(id)
                    .selected_text(word(*selected))
                    .width(size.x)
                    .truncate()
                    .show_ui(ui, |ui| {
                        for (i, c) in choices.iter().enumerate() {
                            selectable_value(ui, selected, i, c.as_ref());
                        }
                    })
                    .response
            })
            .inner;
        let n = choices.len();
        crate::focus::takes_keys(ui.ctx(), response.id);
        if n > 0 && response.has_focus() && !egui::ComboBox::is_open(ui.ctx(), response.id) {
            // The arrows choose, they do not move the focus.
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    response.id,
                    egui::EventFilter {
                        vertical_arrows: true,
                        ..Default::default()
                    },
                );
            });
            let last = n - 1;
            ui.input(|i| {
                if i.key_pressed(Key::ArrowDown) {
                    *selected = (*selected + 1).min(last);
                }
                if i.key_pressed(Key::ArrowUp) {
                    *selected = selected.saturating_sub(1).min(last);
                }
                if i.key_pressed(Key::Home) {
                    *selected = 0;
                }
                if i.key_pressed(Key::End) {
                    *selected = last;
                }
            });
        }
        if *selected != before {
            response.mark_changed();
        }
        let shown = word(*selected).to_owned();
        ui.ctx().accesskit_node_builder(response.id, |node| {
            node.set_label(name);
            node.set_value(shown);
        });
        response.on_hover_text(name)
    }
}

/// A row in the look of a menu's items (see [the module](self)).
#[derive(Clone, Debug)]
#[must_use = "add it with `ui.add`"]
pub struct MenuRow {
    text: String,
    note: Option<String>,
    height: Option<f32>,
}

impl MenuRow {
    /// A row saying `text`, its accessible name.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            note: None,
            height: None,
        }
    }

    /// A muted text at the row's end (a count, a key), read after the
    /// row's text by assistive technologies.
    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }

    /// Its height; by default the kit's `menu-item.height`, at least a
    /// row's.
    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }
}

impl Widget for MenuRow {
    fn ui(self, ui: &mut Ui) -> Response {
        let ctx = ui.ctx().clone();
        let kit = Kit::of(&ctx);
        let item = &kit.menu_item;
        let height = self
            .height
            .unwrap_or_else(|| item.data.height.max(ui.spacing().interact_size.y).max(1.0));
        let width = ui.available_width().max(height);
        let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
        if ui.is_rect_visible(rect) {
            let state = State::from_response(&response);
            let font = egui::TextStyle::Button.resolve(ui.style());
            let label =
                ui.painter()
                    .layout_no_wrap(self.text.clone(), font.clone(), Color32::PLACEHOLDER);
            let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
            let cx = KitCx {
                painter: &painter,
                ppp: ctx.pixels_per_point(),
            };
            let part = Part {
                rect,
                room: rect,
                state,
                label: Some(&label),
            };
            item.paint_part(&cx, &part);
            if let Some(note) = &self.note {
                let pad = item.data.padding_x.max(0.0);
                let galley = painter.layout_no_wrap(
                    note.clone(),
                    font,
                    item.style_toned(Tone::Muted, state).text,
                );
                let at = Rect::from_min_size(
                    egui::pos2(
                        rect.right() - pad - galley.size().x,
                        rect.center().y - galley.size().y / 2.0,
                    ),
                    galley.size(),
                );
                painter.galley(at.min, galley, Color32::PLACEHOLDER);
            }
        }
        let name = match &self.note {
            Some(n) => format!("{}, {n}", self.text),
            None => self.text.clone(),
        };
        let enabled = ui.is_enabled();
        response.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, &name));
        response
    }
}
