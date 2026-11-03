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

use egui::{Key as EKey, Modifiers, Response, Role, TextEdit, Ui};
use egui_pop_api::AppHandle;
use egui_pop_core::strings::{tr, tr_args};
use egui_pop_core::theme::Tokens;
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};

use super::marks::{ColorLabel, Flag, MAX_RATING, Marks};
use super::plugin::{self as lib, LIBRARIES};
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
            FilterChip::Rating(r) => tr_args(ctx, &s::FILTER_RATING, &[("rating", &r.to_string())]),
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

/// A search field's state between passes: the chips, the text typed,
/// the filter last given.
#[derive(Clone, Debug, Default)]
pub struct SearchState {
    chips: Filter,
    text: String,
    last: Filter,
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

    fn effective(&self) -> Filter {
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
        }
    }

    /// Runs its changes as steps of this application, and takes the
    /// `library.filter` command.
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
        let registry = self.app.and_then(|a| a.service(&LIBRARIES));
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
        if focused {
            let (enter, back) = ui.input_mut(|i| {
                (
                    i.consume_key(Modifiers::NONE, EKey::Enter),
                    state.text.is_empty() && i.consume_key(Modifiers::NONE, EKey::Backspace),
                )
            });
            if enter {
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
                    let field = ui.add(
                        TextEdit::singleline(&mut state.text)
                            .id(text_id)
                            .frame(egui::Frame::NONE)
                            .hint_text(label.clone())
                            .desired_width(self.width.unwrap_or(180.0)),
                    );
                    egui_pop_core::a11y::name(&field, Role::TextInput, &label);
                    let add = ui.menu_button("+", |ui| add_menu(ui, &mut state.chips));
                    egui_pop_core::a11y::name(
                        &add.response,
                        Role::Button,
                        &tr(&ctx, &s::ADD_FILTER),
                    );
                    if !state.chips.is_empty() || !state.text.is_empty() {
                        let clear = ui.button("×");
                        egui_pop_core::a11y::name(
                            &clear,
                            Role::Button,
                            &tr(&ctx, &s::CLEAR_FILTERS),
                        );
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
        let now = proposed.unwrap_or_else(|| state.effective());
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
