//! A tree's filter: the query, how a node matches it, and the field that
//! types it ([`FilterField`]).
//!
//! **Matching**: the query is cut into words at spaces; a node matches
//! when every word is somewhere in its match text
//! ([`TreeSource::match_text`](super::TreeSource::match_text): its plain
//! name, plus the words its source adds), as a substring, ignoring case
//! and the accents of Latin letters (`ecran` finds `Écran`). Substrings
//! rather than a fuzzy subsequence (the command palette's): a filter
//! hides what does not match, and a few letters in order are found in
//! almost any long description, so a fuzzy filter would hide nothing.
//! Substrings rather than word starts: `board` finds `keyboard`. This is
//! VS Code's "contiguous" find mode for trees, with several words.
//!
//! **Showing**: while the query is not empty, the nodes that match show
//! with their ancestors, which open without touching the fold the tree
//! keeps (opening or closing a branch meanwhile lasts until the query
//! changes, and runs no `tree.expand`); a branch that matches opens onto
//! all of its children. The rest is hidden; nothing left shows a "No
//! match" row. Only the nodes loaded are searched: a lazy branch never
//! opened is not asked for.
//!
//! ```
//! use egui_pop_views::tree::filter::Query;
//!
//! let q = Query::new("ecran lum");
//! assert!(q.matches("Écran et luminosité"));
//! assert!(!q.matches("Écran"));
//! assert_eq!(q.positions("Écran"), [0, 1, 2, 3, 4]);
//! ```

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use egui::text::LayoutJob;
use egui::{
    Color32, Context, FontId, Id, Key as EguiKey, Response, Role, Stroke, TextEdit, TextFormat, Ui,
};
use egui_pop_core::strings::{tr, tr_with};
use egui_pop_core::{a11y, focus, lock};
use serde_json::{Value as Json, json};

use super::{Row, TreeState};
use crate::select::Key;
use crate::strings as s;

/// The focus surface of a tree drawn with a filter field: its keys
/// (`tree.filter`'s Ctrl+F and `/`) are bound there (`treeFilterFocus`).
pub const SURFACE: &str = "treeFilter";

/// A character as the filter compares it: lower case, a Latin letter
/// without its accent. One character for one, so that positions in the
/// folded text are positions in the text.
pub fn fold(c: char) -> char {
    let c = c.to_lowercase().next().unwrap_or(c);
    match c {
        'à'..='å' | 'ā' | 'ă' | 'ą' => 'a',
        'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => 'c',
        'ď' | 'đ' => 'd',
        'è'..='ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => 'e',
        'ĝ' | 'ğ' | 'ġ' | 'ģ' => 'g',
        'ĥ' | 'ħ' => 'h',
        'ì'..='ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' => 'i',
        'ĵ' => 'j',
        'ķ' => 'k',
        'ĺ' | 'ļ' | 'ľ' | 'ŀ' | 'ł' => 'l',
        'ñ' | 'ń' | 'ņ' | 'ň' => 'n',
        'ò'..='ö' | 'ø' | 'ō' | 'ŏ' | 'ő' => 'o',
        'ŕ' | 'ŗ' | 'ř' => 'r',
        'ś' | 'ŝ' | 'ş' | 'š' => 's',
        'ţ' | 'ť' | 'ŧ' => 't',
        'ù'..='ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => 'u',
        'ŵ' => 'w',
        'ý' | 'ÿ' | 'ŷ' => 'y',
        'ź' | 'ż' | 'ž' => 'z',
        c => c,
    }
}

fn folded(text: &str) -> Vec<char> {
    text.chars().map(fold).collect()
}

/// Where `word` starts in `text`, every time.
fn occurrences<'a>(text: &'a [char], word: &'a [char]) -> impl Iterator<Item = usize> + 'a {
    let last = (text.len() + 1).saturating_sub(word.len());
    (0..last).filter(move |&i| text[i..i + word.len()] == *word)
}

/// A filter's query: its words, folded.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Query {
    words: Vec<Vec<char>>,
}

impl Query {
    pub fn new(text: &str) -> Self {
        Self {
            words: text.split_whitespace().map(folded).collect(),
        }
    }

    /// No words: everything shows.
    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    /// Whether every word is in `text`.
    pub fn matches(&self, text: &str) -> bool {
        let t = folded(text);
        self.words
            .iter()
            .all(|w| occurrences(&t, w).next().is_some())
    }

    /// The characters of `text` (indices in `char`s, ascending) that a
    /// word covers, wherever it is: what a row highlights.
    pub fn positions(&self, text: &str) -> Vec<usize> {
        let t = folded(text);
        let mut hit = vec![false; t.len()];
        for w in &self.words {
            for at in occurrences(&t, w) {
                hit[at..at + w.len()].iter_mut().for_each(|h| *h = true);
            }
        }
        (0..t.len()).filter(|&i| hit[i]).collect()
    }
}

/// The field typing a tree's filter, drawn above the tree (or anywhere
/// near it) with the tree's state.
///
/// - Its text is the tree's filter ([`TreeState::set_filter`]); beside
///   it, while it filters, how many nodes match of how many are loaded
///   ("3 of 57"), which is also announced to screen readers.
/// - Ctrl+F or `/` in the tree comes here: the `tree.filter` command,
///   bound on the tree's [`SURFACE`] by [`ViewsPlugin`](crate::ViewsPlugin)
///   (rebindable in the keymap; that plugin must be connected).
/// - Escape clears the text, then a second one gives the focus back to
///   the tree; Down goes to the tree, its cursor on the first match.
#[derive(Debug)]
pub struct FilterField {
    id: String,
    hint: Option<String>,
}

impl FilterField {
    /// The field of the tree named `tree` (its [`TreeView`](super::TreeView)'s id).
    pub fn new(tree: impl Into<String>) -> Self {
        Self {
            id: tree.into(),
            hint: None,
        }
    }

    /// Its placeholder and accessible name (default: "Filter").
    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// Draws the field; the tree shows the filter from its next draw.
    pub fn show<K: Key>(self, ui: &mut Ui, state: &mut TreeState<K>) -> Response {
        let id = Id::unique(("egui_pop_views::tree_filter", &self.id));
        state.field = Some(id);
        let ctx = ui.ctx().clone();
        let hint = self.hint.unwrap_or_else(|| tr(&ctx, &s::TREE_FILTER));
        let count = state.filter_count();
        // One line high, the count at the right end.
        let size = egui::vec2(ui.available_width(), ui.spacing().interact_size.y);
        let layout = egui::Layout::right_to_left(egui::Align::Center);
        ui.allocate_ui_with_layout(size, layout, |ui| {
            match count {
                Some((0, _)) => {
                    ui.weak(tr(&ctx, &s::TREE_FILTER_NONE));
                }
                Some((n, total)) => {
                    ui.weak(tr_with(
                        &ctx,
                        &s::TREE_FILTER_COUNT,
                        &[("count", n.into()), ("total", total.into())],
                    ));
                }
                None => {}
            }
            let mut text = state.filter().to_string();
            let edit = ui.add(
                TextEdit::singleline(&mut text)
                    .id(id)
                    .hint_text(&hint)
                    .desired_width(ui.available_width()),
            );
            a11y::name(&edit, Role::SearchInput, &hint);
            if std::mem::take(&mut state.field_focus) {
                edit.request_focus();
            }
            if edit.changed() {
                state.set_filter(text);
                ctx.request_repaint();
            }
            let escaped = focus::escape_left(&ctx, id)
                || (edit.lost_focus() && ui.input(|i| i.key_pressed(EguiKey::Escape)));
            if escaped {
                if state.filter().is_empty() {
                    state.focus_tree = true;
                } else {
                    state.set_filter("");
                    edit.request_focus();
                }
                ctx.request_repaint();
            } else if edit.has_focus() && ui.input(|i| i.key_pressed(EguiKey::ArrowDown)) {
                state.focus_tree = true;
                ctx.request_repaint();
            }
            edit
        })
        .inner
    }
}

/// A label with the characters at `hits` in `strong`, underlined.
pub(crate) fn highlighted(
    label: &str,
    hits: &[usize],
    font: FontId,
    normal: Color32,
    strong: Color32,
) -> LayoutJob {
    let mut job = LayoutJob::default();
    let format = |hit: bool| TextFormat {
        font_id: font.clone(),
        color: if hit { strong } else { normal },
        underline: if hit {
            Stroke::new(1.0, strong)
        } else {
            Stroke::NONE
        },
        ..TextFormat::default()
    };
    // In runs of the same format.
    let mut run = String::new();
    let mut run_hit = false;
    for (i, c) in label.chars().enumerate() {
        let hit = hits.binary_search(&i).is_ok();
        if hit != run_hit && !run.is_empty() {
            job.append(&std::mem::take(&mut run), 0.0, format(run_hit));
        }
        run_hit = hit;
        run.push(c);
    }
    if !run.is_empty() {
        job.append(&run, 0.0, format(run_hit));
    }
    job
}

fn request_id() -> Id {
    Id::unique("egui_pop_views::tree_filter::request")
}

/// Asks the focused tree's filter field for the focus (`tree.filter`).
pub(crate) fn request(ctx: &Context) {
    let now = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| d.insert_temp(request_id(), now));
    ctx.request_repaint();
}

/// Whether the field was asked for, this pass or the last; spent once.
pub(crate) fn take_request(ctx: &Context) -> bool {
    let now = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| {
        d.remove_temp::<u64>(request_id())
            .is_some_and(|at| at + 1 >= now)
    })
}

/// How many rows a tree reports to agents at most.
pub const REPORTED_ROWS: usize = 200;

type Reports = Arc<Mutex<BTreeMap<String, Json>>>;

fn reports(ctx: &Context) -> Reports {
    ctx.data_mut(|d| {
        d.get_temp_mut_or_default::<Reports>(Id::unique("egui_pop_views::trees"))
            .clone()
    })
}

/// Reports a tree to agents when its rows changed since the last time.
pub(crate) fn report<K: Key>(ctx: &Context, tree: &str, state: &mut TreeState<K>) {
    let at = state.model.flattens();
    if state.reported == Some(at) {
        return;
    }
    state.reported = Some(at);
    let model = &state.model;
    let rows = model.rows_now();
    let shown: Vec<Json> = rows
        .iter()
        .take(REPORTED_ROWS)
        .map(|r| match r {
            Row::Node { key, depth } => {
                let label = model.node(key).map_or("", |n| n.label.as_str());
                let mut row = json!({ "key": key, "label": label, "depth": depth });
                if model.is_filtered() {
                    row["match"] = json!(model.is_match(key));
                }
                if model.node(key).is_some_and(|n| n.branch) {
                    row["open"] = json!(model.is_open(key));
                }
                row
            }
            Row::Loading { depth, .. } => json!({ "row": "loading", "depth": depth }),
            Row::Failed { depth, reason, .. } => {
                json!({ "row": "failed", "depth": depth, "reason": reason })
            }
            Row::Empty { depth, .. } => json!({ "row": "empty", "depth": depth }),
        })
        .collect();
    let report = json!({
        "filter": state.filter(),
        "matches": model.matches(),
        "nodes": model.len(),
        "rows": rows.len(),
        "no_match": rows.is_empty() && model.is_filtered(),
        "shown": shown,
        "field": state.field.is_some(),
    });
    lock(&reports(ctx)).insert(tree.to_string(), report);
}

/// The trees drawn so far, by id, as last reported: the filter, how many
/// nodes match it (`null`: none set) of how many are loaded, and the rows
/// shown (the first [`REPORTED_ROWS`]: key, label, depth, whether it
/// matches, whether a branch is open). The `trees` debug collector of
/// [`ViewsPlugin`](crate::ViewsPlugin).
pub fn drawn(ctx: &Context) -> Json {
    Json::Object(
        lock(&reports(ctx))
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_word_is_a_substring_ignoring_case_and_accents() {
        let q = Query::new("  BOARD  ecr ");
        assert!(q.matches("Keyboard: the écran"));
        assert!(!q.matches("Keyboard"));
        assert!(Query::new("").is_empty());
        assert!(Query::new("Œuvre").matches("œuvre"));
        assert!(Query::new("zurich").matches("Zürich"));
    }

    #[test]
    fn positions_cover_every_occurrence_of_every_word() {
        let q = Query::new("an é");
        assert_eq!(q.positions("Anne et Jean"), [0, 1, 3, 5, 9, 10, 11]);
        assert!(Query::new("x").positions("abc").is_empty());
        // Folding keeps one character for one.
        assert_eq!(Query::new("i").positions("İi"), [0, 1]);
    }
}
