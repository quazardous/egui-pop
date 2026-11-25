//! A search field with filter chips, giving the application a
//! structured [`Filter`].
//!
//! Words typed search the images' names and tags; typed criteria become
//! chips when Enter is pressed: `rating>=3` (or `rating:3`), `label:red`,
//! `flag:pick` (`reject`, `none`), `tag:beach` (or `#beach`),
//! `date:2024-06-01..2024-06-30`, `from:2024-06-01`, `to:2024-06-30`.
//! The menu beside the field adds a rating, a label or a flag; each chip
//! has its remove button, Backspace in the empty field removes the last
//! one, and the clear button removes everything. Every change of the
//! filter runs the `library.filter` step: a before-hook may refuse it or
//! rewrite it; the application filters its images with what the output
//! gives ([`Filter::matches`] does the usual matching).
//!
//! ```
//! use egui_pop_views::library::filter::{Candidate, SearchField, SearchState};
//!
//! fn ui(ui: &mut egui::Ui, state: &mut SearchState, names: &[String], shown: &mut Vec<usize>) {
//!     if let Some(filter) = SearchField::new("shoot").show(ui, state).changed {
//!         *shown = (0..names.len())
//!             .filter(|&i| filter.matches(&Candidate::named(&names[i])))
//!             .collect();
//!     }
//! }
//! ```

use crate::app_link::AppHandle;
use egui::{Key as EKey, Modifiers, Response, Role, TextEdit, Ui};
use egui_pop_core::strings::{tr, tr_args, tr_with};
use egui_pop_core::theme::Tokens;
use egui_pop_core::tooltip::TooltipExt as _;
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};

use super::marks::{ColorLabel, Flag, MAX_RATING, Marks};
use super::plugin as lib;
use super::tags::TagChip;
use crate::strings as s;

/// What the application filters its images by. Empty criteria filter
/// nothing; all criteria must hold (labels and flags: any of them; tags:
/// all of them).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Filter {
    /// Words, each found in the name or a tag (ignoring case).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub text: String,
    /// At least this many stars.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_rating: Option<u8>,
    /// One of these labels.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<ColorLabel>,
    /// One of these flags.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub flags: Vec<Flag>,
    /// All of these tags.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// From this day (`YYYY-MM-DD`), included.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// Until this day, included.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
}

/// One criterion of a filter, as a chip shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FilterChip {
    Rating(u8),
    Label(ColorLabel),
    Flag(Flag),
    Tag(String),
    /// The date range (either end may be open).
    Dates {
        from: Option<String>,
        to: Option<String>,
    },
}

/// An image as [`Filter::matches`] sees it.
#[derive(Clone, Copy, Debug, Default)]
pub struct Candidate<'a> {
    pub name: &'a str,
    pub marks: Marks,
    pub tags: &'a [String],
    /// Its day, `YYYY-MM-DD`.
    pub date: Option<&'a str>,
}

impl<'a> Candidate<'a> {
    /// An image known by its name only.
    pub fn named(name: &'a str) -> Candidate<'a> {
        Candidate {
            name,
            ..Candidate::default()
        }
    }
}

/// Whether `text` is a day, `YYYY-MM-DD`.
pub fn is_day(text: &str) -> bool {
    let b = text.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

impl Filter {
    pub fn is_empty(&self) -> bool {
        *self == Filter::default()
    }

    /// The criteria typed in `text`, the other words kept as the text.
    pub fn parse(text: &str) -> Filter {
        let mut f = Filter::default();
        let mut words = Vec::new();
        for w in text.split_whitespace() {
            if !f.take(w) {
                words.push(w);
            }
        }
        f.text = words.join(" ");
        f
    }

    /// Reads one typed criterion into the filter; whether it was one.
    fn take(&mut self, w: &str) -> bool {
        let lower = w.to_lowercase();
        let rating = lower
            .strip_prefix("rating>=")
            .or_else(|| lower.strip_prefix("rating:"))
            .or_else(|| lower.strip_prefix("stars:"));
        if let Some(r) = rating.and_then(|r| r.parse::<u8>().ok()) {
            self.min_rating = Some(r.min(MAX_RATING));
            return true;
        }
        if let Some(l) = lower.strip_prefix("label:").and_then(ColorLabel::parse) {
            if !self.labels.contains(&l) {
                self.labels.push(l);
            }
            return true;
        }
        if let Some(fl) = lower.strip_prefix("flag:").and_then(Flag::parse) {
            if !self.flags.contains(&fl) {
                self.flags.push(fl);
            }
            return true;
        }
        if let Some(t) = w
            .strip_prefix("tag:")
            .or_else(|| w.strip_prefix('#'))
            .filter(|t| !t.is_empty())
        {
            if !self.tags.iter().any(|x| x == t) {
                self.tags.push(t.to_string());
            }
            return true;
        }
        if let Some((a, b)) = lower.strip_prefix("date:").and_then(|d| d.split_once(".."))
            && (a.is_empty() || is_day(a))
            && (b.is_empty() || is_day(b))
            && !(a.is_empty() && b.is_empty())
        {
            self.from = (!a.is_empty()).then(|| a.to_string());
            self.to = (!b.is_empty()).then(|| b.to_string());
            return true;
        }
        if let Some(d) = lower.strip_prefix("from:").filter(|d| is_day(d)) {
            self.from = Some(d.to_string());
            return true;
        }
        if let Some(d) = lower.strip_prefix("to:").filter(|d| is_day(d)) {
            self.to = Some(d.to_string());
            return true;
        }
        false
    }

    /// This filter with `other`'s criteria added (its text after this
    /// one's).
    #[must_use]
    pub fn merged(&self, other: &Filter) -> Filter {
        let mut f = self.clone();
        if !other.text.is_empty() {
            f.text = [f.text.as_str(), other.text.as_str()]
                .iter()
                .filter(|t| !t.is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join(" ");
        }
        if other.min_rating.is_some() {
            f.min_rating = other.min_rating;
        }
        for l in &other.labels {
            if !f.labels.contains(l) {
                f.labels.push(*l);
            }
        }
        for fl in &other.flags {
            if !f.flags.contains(fl) {
                f.flags.push(*fl);
            }
        }
        for t in &other.tags {
            if !f.tags.contains(t) {
                f.tags.push(t.clone());
            }
        }
        if other.from.is_some() {
            f.from.clone_from(&other.from);
        }
        if other.to.is_some() {
            f.to.clone_from(&other.to);
        }
        f
    }

    /// Its criteria as chips (the text is not one).
    pub fn chips(&self) -> Vec<FilterChip> {
        let mut out = Vec::new();
        if let Some(r) = self.min_rating {
            out.push(FilterChip::Rating(r));
        }
        out.extend(self.labels.iter().map(|l| FilterChip::Label(*l)));
        out.extend(self.flags.iter().map(|f| FilterChip::Flag(*f)));
        out.extend(self.tags.iter().map(|t| FilterChip::Tag(t.clone())));
        if self.from.is_some() || self.to.is_some() {
            out.push(FilterChip::Dates {
                from: self.from.clone(),
                to: self.to.clone(),
            });
        }
        out
    }

    /// Without that criterion.
    pub fn remove(&mut self, chip: &FilterChip) {
        match chip {
            FilterChip::Rating(_) => self.min_rating = None,
            FilterChip::Label(l) => self.labels.retain(|x| x != l),
            FilterChip::Flag(f) => self.flags.retain(|x| x != f),
            FilterChip::Tag(t) => self.tags.retain(|x| x != t),
            FilterChip::Dates { .. } => {
                self.from = None;
                self.to = None;
            }
        }
    }

    /// Whether an image passes.
    pub fn matches(&self, c: &Candidate<'_>) -> bool {
        let words_ok = self.text.split_whitespace().all(|w| {
            let w = w.to_lowercase();
            c.name.to_lowercase().contains(&w)
                || c.tags.iter().any(|t| t.to_lowercase().contains(&w))
        });
        let rating_ok = self.min_rating.is_none_or(|r| c.marks.rating >= r);
        let labels_ok =
            self.labels.is_empty() || self.labels.iter().any(|l| c.marks.labels.has(*l));
        let flags_ok = self.flags.is_empty() || self.flags.contains(&c.marks.flag);
        let tags_ok = self
            .tags
            .iter()
            .all(|t| c.tags.iter().any(|x| x.eq_ignore_ascii_case(t)));
        let from_ok = self
            .from
            .as_deref()
            .is_none_or(|f| c.date.is_some_and(|d| d >= f));
        let to_ok = self
            .to
            .as_deref()
            .is_none_or(|t| c.date.is_some_and(|d| d <= t));
        words_ok && rating_ok && labels_ok && flags_ok && tags_ok && from_ok && to_ok
    }
}

impl FilterChip {
    /// What the chip shows (translated).
    pub fn title(&self, ctx: &egui::Context) -> String {
        match self {
            FilterChip::Rating(r) => tr_with(ctx, &s::FILTER_RATING, &[("rating", (*r).into())]),
            FilterChip::Label(l) => tr_args(ctx, &s::FILTER_LABEL, &[("label", &l.title(ctx))]),
            FilterChip::Flag(f) => tr_args(ctx, &s::FILTER_FLAG, &[("flag", &f.title(ctx))]),
            FilterChip::Tag(t) => tr_args(ctx, &s::FILTER_TAG, &[("tag", t)]),
            FilterChip::Dates { from, to } => match (from, to) {
                (Some(f), Some(t)) => tr_args(ctx, &s::FILTER_DATES, &[("from", f), ("to", t)]),
                (Some(f), None) => tr_args(ctx, &s::FILTER_FROM, &[("from", f)]),
                (None, Some(t)) => tr_args(ctx, &s::FILTER_TO, &[("to", t)]),
                (None, None) => String::new(),
            },
        }
    }
}

/// A suggestion the application offers under the field
/// ([`SearchField::suggestions`]).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Suggestion {
    /// What the row shows, and its accessible name.
    pub label: String,
    /// What kind of suggestion it is (a key, a value, a recent search),
    /// shown muted beside the label; empty for none.
    pub kind: String,
    /// The field's whole text once the suggestion is taken.
    pub insert: String,
}

impl Suggestion {
    /// A row showing `label` that sets the field's text to `insert`.
    pub fn new(label: impl Into<String>, insert: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            kind: String::new(),
            insert: insert.into(),
        }
    }

    /// Its kind, shown muted beside the label.
    pub fn kind(mut self, kind: impl Into<String>) -> Self {
        self.kind = kind.into();
        self
    }
}

/// A chip the application draws in the field ([`SearchField::chips`]):
/// a part of its own reading of the text, in words.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AppChip {
    /// The application's id for it, given back when it is removed.
    pub id: String,
    /// What the chip shows.
    pub words: String,
}

impl AppChip {
    pub fn new(id: impl Into<String>, words: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            words: words.into(),
        }
    }
}

/// What the application hooks did in a pass
/// ([`SearchState::take_events`]).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SearchEvent {
    /// The suggestion at this index was taken: the text is now its
    /// `insert`.
    Picked(usize),
    /// The application chip with this id was asked away (its remove
    /// button, Backspace twice or Ctrl+Backspace in the empty field): the
    /// application drops it from its text.
    ChipRemoved(String),
    /// Escape closed the suggestion list.
    ListClosed,
    /// Escape cleared the text typed (the list already closed).
    Cleared,
    /// Escape in the empty field, the list closed: the field let go of the
    /// keyboard (back to the grid).
    Left,
}

/// A search field's state between passes: the chips, the text typed,
/// the filter last given.
#[derive(Clone, Debug, Default)]
pub struct SearchState {
    chips: Filter,
    text: String,
    last: Filter,
    /// The suggestion list is open.
    list_open: bool,
    /// The suggestion highlighted.
    highlight: Option<usize>,
    /// The pointer is over the suggestion list.
    list_hovered: bool,
    /// The application chip Backspace selected (a second press removes it).
    chip_selected: Option<usize>,
    /// The text had the keyboard at the end of the last pass.
    had_focus: bool,
    events: Vec<SearchEvent>,
}

impl SearchState {
    pub fn new() -> Self {
        Self::default()
    }

    /// The filter in force (the chips and the text typed).
    pub fn filter(&self) -> &Filter {
        &self.last
    }

    /// The text typed.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Sets the text typed (the application rewriting it, a chip of its
    /// own removed). The filter follows at the next pass.
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
    }

    /// The suggestion list is open (shown while the field has the keyboard
    /// and the application gives suggestions).
    pub fn list_open(&self) -> bool {
        self.list_open
    }

    /// The suggestion highlighted, by index.
    pub fn highlight(&self) -> Option<usize> {
        self.highlight
    }

    /// What the application hooks did since the last call: suggestions
    /// taken, application chips removed, the Escape sequence.
    pub fn take_events(&mut self) -> Vec<SearchEvent> {
        std::mem::take(&mut self.events)
    }

    /// Sets the filter without running a step (its criteria as chips,
    /// its text typed).
    pub fn set_filter(&mut self, filter: Filter) {
        self.text = filter.text.clone();
        self.chips = Filter {
            text: String::new(),
            ..filter.clone()
        };
        self.last = filter;
    }

    fn effective(&self, whole_text: bool) -> Filter {
        if whole_text {
            return self.chips.merged(&Filter {
                text: self.text.trim().to_string(),
                ..Filter::default()
            });
        }
        self.chips.merged(&Filter::parse(&self.text))
    }
}

/// What [`SearchField::show`] returns.
#[derive(Debug)]
pub struct SearchOutput {
    /// The text field.
    pub response: Response,
    /// The filter, when it changed this pass.
    pub changed: Option<Filter>,
    /// Why a change was refused (a before-hook on `library.filter`).
    pub vetoed: Option<String>,
}

/// The search field.
#[derive(Debug)]
#[must_use = "show it with `show`"]
pub struct SearchField<'a> {
    library: String,
    app: Option<&'a AppHandle>,
    width: Option<f32>,
    label: Option<String>,
    whole_text: bool,
    suggestions: Option<&'a [Suggestion]>,
    app_chips: &'a [AppChip],
}

impl<'a> SearchField<'a> {
    /// The search field of library `library` (its name in the
    /// `library.filter` step and in commands).
    pub fn new(library: impl Into<String>) -> Self {
        Self {
            library: library.into(),
            app: None,
            width: None,
            label: None,
            whole_text: false,
            suggestions: None,
            app_chips: &[],
        }
    }

    /// The application's suggestions for the text typed, given each pass
    /// (meant with [`Self::whole_text`]). They show in a list under the
    /// field once something is typed, or on Down: Up and Down move the
    /// highlight, Enter or a click takes the highlighted one (the text
    /// becomes its [`Suggestion::insert`], [`SearchEvent::Picked`]).
    /// Escape closes the list, then clears the text, then leaves the
    /// field, each reported ([`SearchState::take_events`]).
    pub fn suggestions(mut self, suggestions: &'a [Suggestion]) -> Self {
        self.suggestions = Some(suggestions);
        self
    }

    /// The application's chips, drawn in the field after the field's own
    /// (meant with [`Self::whole_text`]: the application's reading of the
    /// text, in words). Each has its remove button; in the empty field,
    /// Backspace selects the last one and removes it at a second press,
    /// Ctrl+Backspace removes it at once. A removal is reported
    /// ([`SearchEvent::ChipRemoved`]): the application drops the chip
    /// from its text.
    pub fn chips(mut self, chips: &'a [AppChip]) -> Self {
        self.app_chips = chips;
        self
    }

    /// The application reads the text typed with its own grammar: the
    /// text is never split into this field's criteria ([`Filter::parse`]),
    /// Enter makes no chip of it, and the filter's `text` is the text as
    /// typed (trimmed). The chips are only those picked from the "+" menu
    /// or set by the application. Off by default.
    pub fn whole_text(mut self, whole: bool) -> Self {
        self.whole_text = whole;
        self
    }

    /// Runs its changes as steps of this application, and takes the
    /// `library.filter` command.
    #[cfg(feature = "api")]
    pub fn app(mut self, app: &'a AppHandle) -> Self {
        self.app = Some(app);
        self
    }

    /// The text field's width.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Its accessible name and hint ("Search" by default).
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn show(self, ui: &mut Ui, state: &mut SearchState) -> SearchOutput {
        let t = Tokens::of(ui.ctx());
        let ctx = ui.ctx().clone();
        let text_id = egui::Id::unique(("egui_pop_views::search", &self.library));
        let label = self.label.clone().unwrap_or_else(|| tr(&ctx, &s::SEARCH));
        let registry = lib::registry(self.app);
        let mut proposed: Option<Filter> = None;
        if let Some(reg) = &registry
            && let Some(f) = reg.take_filter(&self.library)
            && let Ok(f) = lib::parse::<Filter>(&f, "filter")
        {
            state.chips = Filter {
                text: String::new(),
                ..f.clone()
            };
            state.text = f.text.clone();
            proposed = Some(f);
        }
        let focused = ui.memory(|m| m.has_focus(text_id));
        let rows = self.suggestions.unwrap_or(&[]);
        if state.highlight.is_some_and(|h| h >= rows.len()) {
            state.highlight = None;
        }
        if state
            .chip_selected
            .is_some_and(|c| c >= self.app_chips.len())
        {
            state.chip_selected = None;
        }
        let mut picked: Option<usize> = None;
        // egui takes the focus off on Escape before the field is drawn:
        // the field that had it last pass still answers that Escape.
        let escaped = self.suggestions.is_some()
            && !focused
            && state.had_focus
            && ui.input(|i| i.key_pressed(EKey::Escape));
        if (focused || escaped) && self.suggestions.is_some() {
            let shown = state.list_open && !rows.is_empty();
            let (down, up, escape) = ui.input_mut(|i| {
                (
                    !rows.is_empty() && i.consume_key(Modifiers::NONE, EKey::ArrowDown),
                    shown && i.consume_key(Modifiers::NONE, EKey::ArrowUp),
                    i.consume_key(Modifiers::NONE, EKey::Escape) || escaped,
                )
            });
            let n = rows.len();
            if down {
                state.highlight = Some(match (shown, state.highlight) {
                    (true, Some(h)) => (h + 1) % n,
                    _ => 0,
                });
                state.list_open = true;
            }
            if up {
                state.highlight = Some(match state.highlight {
                    Some(h) => (h + n - 1) % n,
                    None => n - 1,
                });
            }
            if escape {
                // The field keeps the focus until the last step.
                ui.memory_mut(|m| m.request_focus(text_id));
                if shown {
                    state.list_open = false;
                    state.highlight = None;
                    state.events.push(SearchEvent::ListClosed);
                } else if !state.text.is_empty() {
                    state.text.clear();
                    state.events.push(SearchEvent::Cleared);
                } else {
                    ui.memory_mut(|m| m.surrender_focus(text_id));
                    state.events.push(SearchEvent::Left);
                }
            }
            if shown
                && let Some(h) = state.highlight
                && ui.input_mut(|i| i.consume_key(Modifiers::NONE, EKey::Enter))
            {
                picked = Some(h);
            }
        }
        if focused && !self.app_chips.is_empty() && state.text.is_empty() {
            let (now, back) = ui.input_mut(|i| {
                (
                    i.consume_key(Modifiers::COMMAND, EKey::Backspace),
                    i.consume_key(Modifiers::NONE, EKey::Backspace),
                )
            });
            let last = self.app_chips.len() - 1;
            if now || (back && state.chip_selected == Some(last)) {
                state.chip_selected = None;
                state
                    .events
                    .push(SearchEvent::ChipRemoved(self.app_chips[last].id.clone()));
            } else if back {
                state.chip_selected = Some(last);
            }
        }
        if let Some(k) = picked {
            take_suggestion(ui.ctx(), text_id, state, rows, k);
        }
        if focused {
            let (enter, back) = ui.input_mut(|i| {
                (
                    i.consume_key(Modifiers::NONE, EKey::Enter),
                    state.text.is_empty() && i.consume_key(Modifiers::NONE, EKey::Backspace),
                )
            });
            if enter && !self.whole_text {
                // Typed criteria become chips.
                let typed = Filter::parse(&state.text);
                state.text = typed.text.clone();
                state.chips = state.chips.merged(&Filter {
                    text: String::new(),
                    ..typed
                });
            }
            if back && let Some(last) = state.chips.chips().last() {
                state.chips.remove(last);
            }
        }
        // The field's box, its ring while the text has the keyboard.
        let kit = egui_pop_core::kit::Kit::of(ui.ctx());
        let field_state =
            egui_pop_core::kit::State::REST.with(egui_pop_core::kit::State::FOCUS, focused);
        let inner = kit
            .search_field
            .show(ui, field_state, egui::epaint::Shadow::NONE, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let side = t.size.icon_inline;
                    let (r, _) =
                        ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
                    egui_pop_core::icon::Icon::Search.paint(ui.painter(), r, t.text.muted);
                    for chip in state.chips.chips() {
                        let title = chip.title(&ctx);
                        let name = tr_args(&ctx, &s::REMOVE, &[("item", &title)]);
                        if TagChip::new(title)
                            .removable(true)
                            .remove_label(name)
                            .show(ui)
                            .removed
                        {
                            state.chips.remove(&chip);
                        }
                    }
                    for (k, chip) in self.app_chips.iter().enumerate() {
                        let name = tr_args(&ctx, &s::REMOVE, &[("item", &chip.words)]);
                        if TagChip::new(chip.words.clone())
                            .removable(true)
                            .selected(state.chip_selected == Some(k))
                            .remove_label(name)
                            .show(ui)
                            .removed
                        {
                            state.chip_selected = None;
                            state.events.push(SearchEvent::ChipRemoved(chip.id.clone()));
                        }
                    }
                    let before = state.text.clone();
                    let field = ui.add(
                        TextEdit::singleline(&mut state.text)
                            .id(text_id)
                            .frame(egui::Frame::NONE)
                            .hint_text(label.clone())
                            .desired_width(self.width.unwrap_or(180.0)),
                    );
                    egui_pop_core::a11y::name(&field, Role::TextInput, &label);
                    if state.text != before && self.suggestions.is_some() {
                        // Typing opens the list on what the application
                        // offers for the new text (at the next pass).
                        state.list_open = true;
                        state.highlight = None;
                        state.chip_selected = None;
                        ui.ctx().request_repaint();
                    }
                    // Each button's name is its tooltip too.
                    let add = ui.menu_button("+", |ui| add_menu(ui, &mut state.chips));
                    egui_pop_core::overlay::follow_menu(&add.response, "add filter menu");
                    let add_name = tr(&ctx, &s::ADD_FILTER);
                    egui_pop_core::a11y::name(&add.response, Role::Button, &add_name);
                    let _ = add.response.tip(add_name);
                    if !state.chips.is_empty() || !state.text.is_empty() {
                        let clear = ui.button("×");
                        let clear_name = tr(&ctx, &s::CLEAR_FILTERS);
                        egui_pop_core::a11y::name(&clear, Role::Button, &clear_name);
                        let clear = clear.tip(clear_name);
                        if clear.clicked() {
                            state.chips = Filter::default();
                            state.text.clear();
                        }
                    }
                    field
                })
                .inner
            });
        let response = inner.inner;
        state.had_focus = ui.memory(|m| m.has_focus(text_id));
        if self.suggestions.is_some() {
            let box_rect = inner.response.rect;
            suggestion_list(ui, text_id, box_rect, state, rows);
        }
        let now = proposed.unwrap_or_else(|| state.effective(self.whole_text));
        let mut changed = None;
        let mut vetoed = None;
        if now != state.last {
            let args = json!({ "library": self.library, "filter": now });
            match lib::report(self.app, &lib::LIBRARY_FILTER, args) {
                Ok(ran) => {
                    let done: Filter =
                        lib::parse(&ran["filter"], "filter").unwrap_or_else(|_| now.clone());
                    if done != now {
                        state.set_filter(done.clone());
                    }
                    state.last = done.clone();
                    changed = Some(done);
                }
                Err(reason) => {
                    // Back to the filter in force.
                    let last = state.last.clone();
                    state.set_filter(last);
                    vetoed = Some(reason);
                }
            }
            if let Some(reg) = &registry {
                reg.publish_filter(
                    &self.library,
                    serde_json::to_value(&state.last).unwrap_or(Json::Null),
                );
            }
            ui.ctx().request_repaint();
        } else if let Some(reg) = &registry {
            reg.touch(&self.library);
        }
        SearchOutput {
            response,
            changed,
            vetoed,
        }
    }
}

/// Takes suggestion `k`: the text becomes its insert, the cursor at its
/// end.
fn take_suggestion(
    ctx: &egui::Context,
    text_id: egui::Id,
    state: &mut SearchState,
    rows: &[Suggestion],
    k: usize,
) {
    let Some(row) = rows.get(k) else {
        return;
    };
    state.text = row.insert.clone();
    state.list_open = false;
    state.highlight = None;
    state.events.push(SearchEvent::Picked(k));
    let mut te = egui::text_edit::TextEditState::load(ctx, text_id).unwrap_or_default();
    let end = egui::text::CCursor::new(state.text.chars().count());
    te.cursor
        .set_char_range(Some(egui::text::CCursorRange::one(end)));
    te.store(ctx, text_id);
    ctx.request_repaint();
}

/// The application's suggestions under the field's box: a list box of
/// options, the highlighted one selected.
fn suggestion_list(
    ui: &mut Ui,
    text_id: egui::Id,
    below: egui::Rect,
    state: &mut SearchState,
    rows: &[Suggestion],
) {
    let ctx = ui.ctx().clone();
    let focused = ctx.memory(|m| m.has_focus(text_id));
    if !focused && !state.list_hovered {
        state.list_open = false;
        state.highlight = None;
    }
    let list_id = text_id.with("suggestions");
    let visible = state.list_open && !rows.is_empty();
    let mut hovered = false;
    if visible {
        let name = tr(&ctx, &s::SUGGESTIONS);
        let mut clicked = None;
        let area = egui::Area::new(list_id)
            .order(egui::Order::Foreground)
            .fixed_pos(below.left_bottom())
            .show(&ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.scope_builder(
                        egui::UiBuilder::new()
                            .accessibility_role(egui::accesskit::Role::ListBox)
                            .accessibility_label(name.clone()),
                        |ui| {
                            for (k, row) in rows.iter().enumerate() {
                                let on = state.highlight == Some(k);
                                ui.horizontal(|ui| {
                                    let r = egui_pop_core::widgets::selectable_label(
                                        ui,
                                        on,
                                        row.label.as_str(),
                                    );
                                    let label = row.label.clone();
                                    r.widget_info(|| {
                                        egui::WidgetInfo::selected(
                                            Role::ListBoxOption,
                                            true,
                                            on,
                                            &label,
                                        )
                                    });
                                    if on {
                                        r.scroll_to_me(None);
                                    }
                                    if !row.kind.is_empty() {
                                        ui.weak(row.kind.as_str());
                                    }
                                    if r.clicked() {
                                        clicked = Some(k);
                                    }
                                });
                            }
                        },
                    );
                });
            });
        hovered = area.response.contains_pointer();
        if let Some(k) = clicked {
            take_suggestion(&ctx, text_id, state, rows, k);
            ctx.memory_mut(|m| m.request_focus(text_id));
        }
    }
    state.list_hovered = hovered;
    egui_pop_core::overlay::follow_popup(
        &ctx,
        list_id,
        "search suggestions",
        state.list_open && !rows.is_empty(),
        None,
    );
}

/// The menu adding a criterion.
fn add_menu(ui: &mut Ui, chips: &mut Filter) {
    let ctx = ui.ctx().clone();
    ui.menu_button(tr(&ctx, &s::RATING), |ui| {
        for r in 1..=MAX_RATING {
            if ui.button(FilterChip::Rating(r).title(&ctx)).clicked() {
                chips.min_rating = Some(r);
                ui.close();
            }
        }
    });
    ui.menu_button(tr(&ctx, &s::LABEL), |ui| {
        for l in ColorLabel::ALL {
            if ui.button(l.title(&ctx)).clicked() {
                if !chips.labels.contains(&l) {
                    chips.labels.push(l);
                }
                ui.close();
            }
        }
    });
    ui.menu_button(tr(&ctx, &s::FLAG), |ui| {
        for f in Flag::ALL {
            if ui.button(f.title(&ctx)).clicked() {
                if !chips.flags.contains(&f) {
                    chips.flags.push(f);
                }
                ui.close();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::marks::Labels;

    #[test]
    fn typed_criteria_become_a_structured_filter() {
        let f = Filter::parse(
            "beach rating>=3 label:red #sea flag:pick date:2024-06-01..2024-06-30 sunset",
        );
        assert_eq!(f.text, "beach sunset");
        assert_eq!(f.min_rating, Some(3));
        assert_eq!(f.labels, [ColorLabel::Red]);
        assert_eq!(f.flags, [Flag::Pick]);
        assert_eq!(f.tags, ["sea"]);
        assert_eq!(f.from.as_deref(), Some("2024-06-01"));
        assert_eq!(f.to.as_deref(), Some("2024-06-30"));
        assert_eq!(f.chips().len(), 5);
        // Not criteria: kept as words.
        let g = Filter::parse("label:mauve date:soon");
        assert_eq!(g.text, "label:mauve date:soon");
        let json = serde_json::to_value(&f).unwrap();
        assert_eq!(json["labels"], json!(["red"]));
        assert_eq!(serde_json::from_value::<Filter>(json).unwrap(), f);
        assert_eq!(serde_json::to_value(Filter::default()).unwrap(), json!({}));
    }

    #[test]
    fn a_filter_matches_marks_tags_and_days() {
        let tags = vec!["Sea".to_string(), "summer".to_string()];
        let c = Candidate {
            name: "IMG_0042.raw",
            marks: Marks {
                rating: 4,
                labels: Labels::NONE.with(ColorLabel::Red, true),
                flag: Flag::Pick,
            },
            tags: &tags,
            date: Some("2024-06-12"),
        };
        assert!(
            Filter::parse("0042 rating>=4 label:red flag:pick #sea from:2024-06-01").matches(&c)
        );
        assert!(Filter::parse("summer").matches(&c), "words search tags");
        assert!(!Filter::parse("rating:5").matches(&c));
        assert!(!Filter::parse("label:blue").matches(&c));
        assert!(!Filter::parse("to:2024-06-11").matches(&c));
        let mut f = Filter::parse("label:red label:blue");
        assert!(f.matches(&c), "any label");
        f.remove(&FilterChip::Label(ColorLabel::Red));
        assert!(!f.matches(&c));
    }
}
