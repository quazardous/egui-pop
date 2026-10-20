//! Tag chips and a tag input completing from the application's tags.
//!
//! The application owns the tags: a chip says it was clicked or asked
//! to be removed, the input says which tags were added or removed; the
//! application changes its data (to tag images through the library's
//! `library.tag` step, see [`change`](super::grid::change)).
//!
//! ```
//! use egui_pop_views::library::tags::{TagInput, TagInputState};
//!
//! fn ui(ui: &mut egui::Ui, state: &mut TagInputState, tags: &mut Vec<String>, known: &Vec<String>) {
//!     let out = TagInput::new("keywords").source(known).show(ui, state, tags);
//!     tags.retain(|t| !out.removed.contains(t));
//!     tags.extend(out.added);
//! }
//! ```

use egui::{
    Color32, Id, Key as EKey, Modifiers, Order, Response, Sense, TextEdit, TextStyle, Ui,
    WidgetInfo, WidgetType, vec2,
};
use egui_pop_core::icon::Icon;
use egui_pop_core::kit::{Kit, KitCx, Part, State};
use egui_pop_core::strings::{tr, tr_args};
use egui_pop_core::theme::Tokens;
use egui_pop_core::widgets;
use egui_pop_model::fuzzy;

use crate::strings as s;

/// Where completions come from: the application's known tags.
pub trait TagSource {
    /// The tags completing `text`, best first, at most `limit`.
    fn complete(&self, text: &str, limit: usize) -> Vec<String>;
}

/// A plain list of tags completes by fuzzy match (characters in order,
/// word starts first).
impl<S: AsRef<str>> TagSource for Vec<S> {
    fn complete(&self, text: &str, limit: usize) -> Vec<String> {
        complete(self, text, limit)
    }
}

/// The tags of `list` matching `text`, best first.
pub fn complete<S: AsRef<str>>(list: &[S], text: &str, limit: usize) -> Vec<String> {
    let text = text.trim();
    if text.is_empty() {
        return Vec::new();
    }
    let mut hits: Vec<(i32, &str)> = list
        .iter()
        .filter_map(|t| fuzzy::score(text, t.as_ref()).map(|m| (m.score, t.as_ref())))
        .collect();
    hits.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then(a.1.len().cmp(&b.1.len()))
            .then(a.1.cmp(b.1))
    });
    hits.into_iter()
        .take(limit)
        .map(|(_, t)| t.to_string())
        .collect()
}

/// What a chip returns.
#[derive(Debug)]
pub struct ChipResponse {
    /// The chip's body.
    pub response: Response,
    /// Its remove button was clicked.
    pub removed: bool,
}

/// A tag (or a filter) as a pill: selectable, and removable by its own
/// button.
#[derive(Clone, Debug)]
#[must_use = "show it with `show`"]
pub struct TagChip {
    text: String,
    removable: bool,
    selected: bool,
    remove_label: Option<String>,
}

impl TagChip {
    pub fn new(text: impl Into<String>) -> TagChip {
        TagChip {
            text: text.into(),
            removable: false,
            selected: false,
            remove_label: None,
        }
    }

    /// With a remove button.
    pub fn removable(mut self, on: bool) -> TagChip {
        self.removable = on;
        self
    }

    pub fn selected(mut self, on: bool) -> TagChip {
        self.selected = on;
        self
    }

    /// The remove button's accessible name ("Remove" and its text by default).
    pub fn remove_label(mut self, label: impl Into<String>) -> TagChip {
        self.remove_label = Some(label.into());
        self
    }

    pub fn show(self, ui: &mut Ui) -> ChipResponse {
        let t = Tokens::of(ui.ctx());
        let kit = Kit::of(ui.ctx());
        let chip = &kit.tag_chip;
        let font = TextStyle::Body.resolve(ui.style());
        let galley = ui
            .painter()
            .layout_no_wrap(self.text.clone(), font, Color32::PLACEHOLDER);
        let h = (t.size.control_height * 0.8).max(galley.size().y + 2.0);
        let x = if self.removable { h * 0.8 } else { 0.0 };
        let pad = chip.data.padding_x;
        let size = vec2(galley.size().x + pad * 2.0 + x, h);
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        let mut removed = false;
        if ui.is_rect_visible(rect) {
            // A selected chip is not hovered: its fill stays under the
            // pointer.
            let state = State::REST
                .with(State::SELECTED, self.selected)
                .with(State::HOVER, response.hovered() && !self.selected)
                .with(State::FOCUS, response.has_focus());
            chip.paint_part(
                &KitCx {
                    painter: ui.painter(),
                    ppp: ui.ctx().pixels_per_point(),
                },
                &Part {
                    rect,
                    room: rect,
                    state,
                    label: Some(&galley),
                },
            );
        }
        if self.removable {
            let b = egui::Rect::from_center_size(
                rect.right_center() - vec2(x / 2.0 + pad / 4.0, 0.0),
                vec2(x * 0.75, x * 0.75),
            );
            let r = ui.interact(b, response.id.with("remove"), Sense::click());
            let color = if r.hovered() {
                t.text.strong
            } else {
                t.text.muted
            };
            Icon::Close.paint(ui.painter(), b.shrink(1.0), color);
            let name = self
                .remove_label
                .clone()
                .unwrap_or_else(|| tr_args(ui.ctx(), &s::REMOVE, &[("item", &self.text)]));
            r.widget_info(move || WidgetInfo::labeled(WidgetType::Button, true, &name));
            removed = r.clicked();
        }
        let text = self.text;
        let selected = self.selected;
        response
            .widget_info(move || WidgetInfo::selected(WidgetType::Button, true, selected, &text));
        ChipResponse { response, removed }
    }
}

/// A tag input's state between passes.
#[derive(Clone, Debug, Default)]
pub struct TagInputState {
    text: String,
    highlighted: usize,
    popup_hovered: bool,
    focus: bool,
}

impl TagInputState {
    /// What is typed.
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// What [`TagInput::show`] returns.
#[derive(Debug)]
pub struct TagInputOutput {
    /// The text field.
    pub response: Response,
    /// Tags the user added (Enter, a comma, Tab or a click on a
    /// completion), not already there.
    pub added: Vec<String>,
    /// Tags the user removed (a chip's button, Backspace in the empty
    /// field).
    pub removed: Vec<String>,
}

/// The tags as removable chips, then a field adding tags, completing
/// from a [`TagSource`] (Up and Down move in the completions, Enter or
/// Tab take one).
#[must_use = "show it with `show`"]
pub struct TagInput<'a> {
    id: String,
    label: Option<String>,
    source: Option<&'a dyn TagSource>,
    limit: usize,
    width: Option<f32>,
}

impl std::fmt::Debug for TagInput<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TagInput")
            .field("id", &self.id)
            .field("label", &self.label)
            .field("source", &self.source.is_some())
            .field("limit", &self.limit)
            .finish()
    }
}

impl<'a> TagInput<'a> {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: None,
            source: None,
            limit: 8,
            width: None,
        }
    }

    /// The field's accessible name and hint ("Add a tag" by default).
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Where completions come from.
    pub fn source(mut self, source: &'a dyn TagSource) -> Self {
        self.source = Some(source);
        self
    }

    /// How many completions show (8 by default).
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = limit.max(1);
        self
    }

    /// The field's width.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    pub fn show(self, ui: &mut Ui, state: &mut TagInputState, tags: &[String]) -> TagInputOutput {
        let text_id = Id::new(("egui_pop_views::tag_input", &self.id));
        let label = self
            .label
            .clone()
            .unwrap_or_else(|| tr(ui.ctx(), &s::ADD_TAG));
        let focused = ui.memory(|m| m.has_focus(text_id));
        let open = focused || state.popup_hovered;
        let suggestions: Vec<String> = match self.source {
            Some(src) if open && !state.text.trim().is_empty() => src
                .complete(&state.text, self.limit + tags.len())
                .into_iter()
                .filter(|c| !tags.contains(c))
                .take(self.limit)
                .collect(),
            _ => Vec::new(),
        };
        if state.highlighted >= suggestions.len() {
            state.highlighted = 0;
        }
        let mut added = Vec::new();
        let mut removed = Vec::new();
        let commit = |text: &str, added: &mut Vec<String>| {
            let t = text.trim();
            if !t.is_empty() && !tags.iter().any(|x| x == t) && !added.iter().any(|x| x == t) {
                added.push(t.to_string());
            }
        };
        if focused {
            let (down, up, enter, tab, back) = ui.input_mut(|i| {
                (
                    i.consume_key(Modifiers::NONE, EKey::ArrowDown),
                    i.consume_key(Modifiers::NONE, EKey::ArrowUp),
                    i.consume_key(Modifiers::NONE, EKey::Enter),
                    !suggestions.is_empty() && i.consume_key(Modifiers::NONE, EKey::Tab),
                    state.text.is_empty() && i.consume_key(Modifiers::NONE, EKey::Backspace),
                )
            });
            if !suggestions.is_empty() {
                if down {
                    state.highlighted = (state.highlighted + 1) % suggestions.len();
                }
                if up {
                    state.highlighted =
                        (state.highlighted + suggestions.len() - 1) % suggestions.len();
                }
            }
            if tab || (enter && !suggestions.is_empty()) {
                commit(&suggestions[state.highlighted], &mut added);
                state.text.clear();
            } else if enter {
                let text = std::mem::take(&mut state.text);
                commit(&text, &mut added);
            }
            if back && let Some(last) = tags.last() {
                removed.push(last.clone());
            }
        }
        let width = self.width.unwrap_or(160.0);
        let inner = ui.horizontal_wrapped(|ui| {
            for tag in tags {
                let chip = TagChip::new(tag.clone()).removable(true).show(ui);
                if chip.removed {
                    removed.push(tag.clone());
                }
            }
            ui.add(
                TextEdit::singleline(&mut state.text)
                    .id(text_id)
                    .hint_text(label.clone())
                    .desired_width(width),
            )
        });
        let response = inner.inner;
        egui_pop_core::a11y::name(&response, WidgetType::TextEdit, &label);
        if response.changed() {
            // Completions follow the text at the next pass.
            ui.ctx().request_repaint();
        }
        if state.focus {
            response.request_focus();
            state.focus = false;
        }
        // A comma ends a tag.
        if state.text.contains(',') {
            let text = std::mem::take(&mut state.text);
            let mut parts: Vec<&str> = text.split(',').collect();
            let rest = parts.pop().unwrap_or("");
            for p in parts {
                commit(p, &mut added);
            }
            state.text = rest.trim_start().to_string();
        }
        let mut hovered = false;
        if !suggestions.is_empty() && open {
            let area = egui::Area::new(text_id.with("completions"))
                .order(Order::Foreground)
                .fixed_pos(response.rect.left_bottom())
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        for (k, c) in suggestions.iter().enumerate() {
                            if widgets::selectable_label(ui, k == state.highlighted, c.as_str())
                                .clicked()
                            {
                                commit(c, &mut added);
                                state.text.clear();
                                state.focus = true;
                            }
                        }
                    });
                });
            hovered = area.response.contains_pointer();
        }
        state.popup_hovered = hovered;
        if !added.is_empty() || !removed.is_empty() {
            ui.ctx().request_repaint();
        }
        TagInputOutput {
            response,
            added,
            removed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completions_rank_word_starts_first() {
        let known = vec!["seascape", "beach", "sea", "night sea", "bench"];
        let sea = complete(&known, "sea", 3);
        assert_eq!(sea.len(), 3);
        assert_eq!(sea[0], "sea");
        assert!(!sea.iter().any(|t| t == "beach"));
        assert!(complete(&known, "bch", 5).iter().any(|t| t == "beach"));
        assert!(complete(&known, "  ", 5).is_empty());
    }
}
