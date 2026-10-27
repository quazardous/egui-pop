//! A theme being edited: a base theme, the tokens changed over it, and
//! the history of those changes. What a theme editor works on, with no
//! egui in it.
//!
//! A draft is saved as an ordinary token file ([`TokenFile`]): `$extends`
//! names the base theme, and the file gives the changed tokens only, with
//! their types. Any file egui-pop loads can be read back as a draft.
//!
//! A theme with a light and a dark variant is edited as a pair
//! ([`ThemeDraft::pair`], [`ThemeDraft::from_pair_spec`]): the draft
//! holds both variants, each with its own changes and history, and edits
//! one at a time ([`ThemeDraft::switch`]). It is saved as one file, the
//! first variant's, carrying the other
//! ([`crate::COUNTERPART_EXTENSION`]).
//!
//! ```
//! use egui_pop_tokens::{ThemeDraft, Themes};
//! use serde_json::json;
//!
//! let themes = Themes::default();
//! let mut draft = ThemeDraft::new("pop-light");
//! let edit = draft.edit(&themes, "accent.default", Some(json!("#2060c0")));
//! draft.apply(edit.unwrap().expect("a change"));
//! let text = draft.export(&themes, "my-theme");
//! let (back, skipped) = ThemeDraft::import(&themes, "my-theme", &text).unwrap();
//! assert!(skipped.is_empty());
//! assert_eq!(back.overrides(), draft.overrides());
//! assert!(draft.undo().is_some());
//! assert!(draft.overrides().is_empty());
//! ```

use std::collections::BTreeMap;

use serde_json::{Value as Json, json};

use crate::file::{RawToken, TokenFile, alias_target};
use crate::palette::{Ramp, Seed};
use crate::theme::{
    COUNTERPART_EXTENSION, Counterpart, FALLBACK_THEME, ThemeSpec, Themes, in_scales,
    seed_backdrop, seeded_scales,
};
use crate::{Error, Value, Warning};

/// The `$extensions` key the theme editor stamped its files with before
/// the format stamp; still read.
#[deprecated(note = "the editor stamps its files with `FORMAT_EXTENSION` (`FormatStamp`)")]
pub const DRAFT_EXTENSION: &str = LEGACY_EXTENSION;

/// The version the theme editor wrote under its former stamp.
#[deprecated(note = "the editor stamps its files with `FORMAT_EXTENSION` (`FormatStamp`)")]
pub const DRAFT_VERSION: u64 = LEGACY_VERSION;

/// The editor's former stamp, read in files written before the format
/// stamp.
const LEGACY_EXTENSION: &str = "org.egui-pop.theme-editor";
const LEGACY_VERSION: u64 = 1;

/// How many edits a draft's history keeps by default, the oldest
/// dropped first ([`ThemeDraft::set_history_cap`]).
pub const DRAFT_HISTORY_CAP: usize = 500;

/// One change of one token: from a value to another. `None` is the
/// value the base theme gives (no override).
#[derive(Clone, Debug, PartialEq)]
pub struct TokenEdit {
    pub path: String,
    pub from: Option<Json>,
    pub to: Option<Json>,
}

impl TokenEdit {
    /// The edit taking this one back.
    pub fn inverse(&self) -> TokenEdit {
        TokenEdit {
            path: self.path.clone(),
            from: self.to.clone(),
            to: self.from.clone(),
        }
    }

    /// As JSON: `{path, from, to}`, `null` for the base theme's value.
    pub fn to_json(&self) -> Json {
        json!({ "path": self.path, "from": self.from, "to": self.to })
    }
}

/// A pair's other variant generated from the one edited
/// ([`ThemeDraft::generate_counterpart`]): a proposal, for a preview,
/// that changes nothing until recorded.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GeneratedVariant {
    /// The edits making the other variant: each checked against its
    /// base, from its own values. Recorded on it as one change (the
    /// first with [`ThemeDraft::record`], the others with
    /// [`ThemeDraft::record_joined`]).
    pub edits: Vec<TokenEdit>,
    /// The seeds its scales are generated from, for its scheme: what it
    /// records ([`ThemeDraft::set_seed`]) in place of its own.
    pub seeds: BTreeMap<String, Seed>,
    /// The tokens, or seeded scales, of the variant edited not carried
    /// over: the other base has no such token, or the value does not
    /// check there.
    pub skipped: Vec<String>,
}

/// A theme being edited: one theme, or a light and dark pair.
///
/// Its methods read and change the variant edited: its base, its
/// overrides, its history and its seeds. A pair's other variant keeps
/// its own ([`ThemeDraft::counterpart`]) until [`ThemeDraft::switch`]
/// makes it the one edited.
///
/// ```
/// use egui_pop_tokens::{ThemeDraft, Themes};
/// use serde_json::json;
///
/// let themes = Themes::default();
/// let mut draft = ThemeDraft::for_theme(&themes, "pop-dark");
/// assert_eq!(draft.counterpart().map(ThemeDraft::base), Some("pop-light"));
/// let edit = draft.edit(&themes, "danger", Some(json!("#ff4040"))).unwrap();
/// draft.apply(edit.unwrap());
/// draft.switch();
/// assert_eq!(draft.base(), "pop-light");
/// assert!(draft.overrides().is_empty());
/// assert!(!draft.can_undo(), "each variant its own history");
/// let edit = draft.edit(&themes, "danger", Some(json!("#c00000"))).unwrap();
/// draft.apply(edit.unwrap());
/// // Installed as a pair, each variant with its own overrides.
/// let spec = draft.pair_spec(&Default::default());
/// assert_eq!(spec.base, "pop-dark");
/// assert_eq!(spec.overrides["danger"], json!("#ff4040"));
/// assert_eq!(spec.counterpart_overrides.unwrap()["danger"], json!("#c00000"));
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct ThemeDraft {
    base: String,
    overrides: BTreeMap<String, Json>,
    undo: Vec<TokenEdit>,
    redo: Vec<TokenEdit>,
    /// Per edit of `undo` and `redo`: whether it goes with the edit made
    /// just before it, as one change ([`ThemeDraft::record_joined`]).
    joined_undo: Vec<bool>,
    joined_redo: Vec<bool>,
    /// The scales generated from a seed ([`ThemeDraft::set_seed`]), by
    /// scale: saved as their seed while their values are still the ones
    /// it gives ([`ThemeDraft::to_file`]).
    seeds: BTreeMap<String, Seed>,
    /// A pair's other variant, light or dark: its own base, overrides,
    /// history and seeds (its own `other` is `None`). `None` for a theme
    /// alone.
    other: Option<Box<ThemeDraft>>,
    /// Whether the variant edited is the pair's second, the counterpart
    /// of the theme named first.
    second: bool,
    /// How many edits the history keeps ([`ThemeDraft::set_history_cap`]).
    cap: usize,
}

impl Default for ThemeDraft {
    fn default() -> Self {
        ThemeDraft::new(FALLBACK_THEME)
    }
}

impl ThemeDraft {
    /// A draft over a base theme, nothing changed yet.
    pub fn new(base: impl Into<String>) -> ThemeDraft {
        ThemeDraft {
            base: base.into(),
            overrides: BTreeMap::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            joined_undo: Vec::new(),
            joined_redo: Vec::new(),
            seeds: BTreeMap::new(),
            other: None,
            second: false,
            cap: DRAFT_HISTORY_CAP,
        }
    }

    /// A draft from a spec: its base and its overrides, no history; the
    /// base alone (see [`ThemeDraft::from_pair_spec`] for its pair).
    pub fn from_spec(spec: &ThemeSpec) -> ThemeDraft {
        ThemeDraft {
            overrides: spec.overrides.clone(),
            ..ThemeDraft::new(spec.base.clone())
        }
    }

    /// A draft over a light and dark pair, nothing changed in either:
    /// `first` the theme named first (shown when nothing asks for the
    /// other mode), edited first; `second` its counterpart.
    pub fn pair(first: impl Into<String>, second: impl Into<String>) -> ThemeDraft {
        ThemeDraft {
            other: Some(Box::new(ThemeDraft::new(second))),
            ..ThemeDraft::new(first)
        }
    }

    /// A draft over a theme and, when it has one of the other mode, its
    /// counterpart ([`Themes::counterpart_of`]), nothing changed: what
    /// "start from a theme" gives.
    pub fn for_theme(themes: &Themes, base: &str) -> ThemeDraft {
        ThemeDraft::from_pair_spec(themes, &ThemeSpec::new(base))
    }

    /// A draft from a spec and its counterpart ([`Themes::counterpart`]),
    /// no history: the base edited, with the spec's overrides; the
    /// counterpart with its own overrides, or the spec's when it has
    /// none of its own ([`ThemeSpec::counterpart_overrides`]). A
    /// counterpart of the same mode as the base is not one: the draft is
    /// the base alone.
    pub fn from_pair_spec(themes: &Themes, spec: &ThemeSpec) -> ThemeDraft {
        let mut draft = ThemeDraft::from_spec(spec);
        if let Some(other) = themes
            .counterpart(spec)
            .filter(|other| is_dark(themes, other) != is_dark(themes, &spec.base))
        {
            draft.other = Some(Box::new(ThemeDraft {
                overrides: spec
                    .counterpart_overrides
                    .clone()
                    .unwrap_or_else(|| spec.overrides.clone()),
                ..ThemeDraft::new(other)
            }));
        }
        draft
    }

    /// Whether the draft is a light and dark pair.
    pub fn is_paired(&self) -> bool {
        self.other.is_some()
    }

    /// The pair's other variant, as it is: its base, overrides, history
    /// and seeds. `None` for a theme alone.
    pub fn counterpart(&self) -> Option<&ThemeDraft> {
        self.other.as_deref()
    }

    /// Whether the variant edited is the pair's second (the counterpart
    /// of the theme named first).
    pub fn editing_second(&self) -> bool {
        self.second
    }

    /// The pair's variants: the theme named first, then its counterpart
    /// (`None` for a theme alone). Read their base, overrides, history
    /// and seeds.
    pub fn variants(&self) -> (&ThemeDraft, Option<&ThemeDraft>) {
        match self.other.as_deref() {
            None => (self, None),
            Some(other) if self.second => (other, Some(self)),
            Some(other) => (self, Some(other)),
        }
    }

    /// The pair's other variant as generated from the one edited, so
    /// that it becomes its counterpart: its base with the edited
    /// variant's changes carried over, its own changes not among them
    /// taken back. `None` for a theme alone. Deterministic, and nothing
    /// is changed: record [`GeneratedVariant::edits`] on the other
    /// variant (after [`ThemeDraft::switch`]) to apply it.
    ///
    /// How each change is carried over:
    ///
    /// - a colour value: mirrored ([`crate::palette::mirror`], the
    ///   same OkHsl hue and saturation, the lightness `1 - l`);
    /// - an alias (`{blue.9}`): as it is, so it names the other
    ///   variant's own token;
    /// - any other value (a dimension, a number, a duration, font
    ///   families): as it is, shared by both;
    /// - a scale generated from a seed ([`ThemeDraft::set_seed`], its
    ///   step 9 still changed): the same seed for the other scheme,
    ///   generated again ([`ThemeDraft::seeded_ramp`]) over the other
    ///   variant with the changes above, its steps written; the seed is
    ///   in [`GeneratedVariant::seeds`].
    ///
    /// ```
    /// use egui_pop_tokens::{ThemeDraft, Themes};
    /// use serde_json::json;
    ///
    /// let themes = Themes::default();
    /// let mut draft = ThemeDraft::for_theme(&themes, "pop-dark");
    /// let e = draft.edit(&themes, "surface.panel", Some(json!("#000000"))).unwrap();
    /// draft.apply(e.unwrap());
    /// let generated = draft.generate_counterpart(&themes).unwrap();
    /// assert_eq!(generated.edits[0].path, "surface.panel");
    /// assert_eq!(generated.edits[0].to, Some(json!("#ffffff")));
    /// ```
    pub fn generate_counterpart(&self, themes: &Themes) -> Option<GeneratedVariant> {
        let other = self.other.as_deref()?;
        let scheme = if is_dark(themes, &other.base) {
            crate::palette::Scheme::Dark
        } else {
            crate::palette::Scheme::Light
        };
        let resolved = themes.resolve(&ThemeSpec::new(other.base.clone()));
        let live: BTreeMap<&String, &Seed> = self
            .seeds
            .iter()
            .filter(|(scale, _)| self.overrides.contains_key(&format!("{scale}.9")))
            .collect();
        let mut out = GeneratedVariant::default();
        let mut wanted: BTreeMap<String, Json> = BTreeMap::new();
        for (path, value) in &self.overrides {
            if in_scales(live.keys().copied(), path) {
                continue;
            }
            let is_color = resolved.get(path).is_some_and(|t| t.ty == "color");
            let carried = match Value::parse("color", value) {
                Ok(Value::Color(c)) if is_color && alias_target(value).is_none() => {
                    json!(crate::palette::mirror(c).to_hex())
                }
                _ => value.clone(),
            };
            match check(themes, &other.base, path, &carried) {
                Ok(()) => {
                    wanted.insert(path.clone(), carried);
                }
                Err(_) => out.skipped.push(path.clone()),
            }
        }
        // The seeded scales, fitted to the other variant as generated.
        let fitted = ThemeDraft {
            overrides: wanted.clone(),
            ..ThemeDraft::new(other.base.clone())
        };
        for (scale, seed) in live {
            if resolved.get(&format!("{scale}.9")).is_none() {
                out.skipped.push(scale.clone());
                continue;
            }
            let seed = Seed::new(seed.color, scheme);
            let ramp = fitted.seeded_ramp(themes, scale, seed);
            for (path, value) in ramp.tokens(scale) {
                if resolved.get(&path).is_some() {
                    wanted.insert(path, value);
                }
            }
            out.seeds.insert(scale.clone(), seed);
        }
        let paths: std::collections::BTreeSet<&String> =
            other.overrides.keys().chain(wanted.keys()).collect();
        for path in paths {
            let from = other.overrides.get(path).cloned();
            let to = wanted.get(path).cloned();
            if from != to {
                out.edits.push(TokenEdit {
                    path: path.clone(),
                    from,
                    to,
                });
            }
        }
        Some(out)
    }

    /// Edits the pair's other variant: what was edited keeps its changes
    /// and history, and gets them back when switched to again. `false`
    /// for a theme alone (nothing to switch to).
    pub fn switch(&mut self) -> bool {
        let Some(mut other) = self.other.take() else {
            return false;
        };
        std::mem::swap(self, &mut *other);
        self.second = !other.second;
        other.second = false;
        self.other = Some(other);
        true
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    /// The tokens changed over the base theme, raw DTCG values by path.
    pub fn overrides(&self) -> &BTreeMap<String, Json> {
        &self.overrides
    }

    /// A token's own value in this draft, `None` when the base theme
    /// gives it.
    pub fn value(&self, path: &str) -> Option<&Json> {
        self.overrides.get(path)
    }

    /// `template` with the variant edited's base and overrides (its
    /// density, accent and reduced motion kept): what the variant edited
    /// resolves from.
    pub fn spec(&self, template: &ThemeSpec) -> ThemeSpec {
        let mut spec = template.clone();
        spec.base = self.base.clone();
        spec.overrides = self.overrides.clone();
        spec.counterpart_overrides = None;
        spec
    }

    /// `template` with the whole draft, as it is installed: the theme
    /// named first, its overrides, and its counterpart with overrides of
    /// its own ([`ThemeSpec::counterpart_overrides`]); a theme alone has
    /// no counterpart (its overrides are not another theme's).
    pub fn pair_spec(&self, template: &ThemeSpec) -> ThemeSpec {
        let (first, second) = self.variants();
        let mut spec = template.clone();
        spec.base = first.base.clone();
        spec.overrides = first.overrides.clone();
        match second {
            Some(second) => {
                spec.counterpart = Counterpart::Theme(second.base.clone());
                spec.counterpart_overrides = Some(second.overrides.clone());
            }
            None => {
                spec.counterpart = Counterpart::None;
                spec.counterpart_overrides = None;
            }
        }
        spec
    }

    /// Checks a new value for a token and gives the edit setting it, or
    /// why it cannot be: the token must be one of the base theme's, and
    /// the value one of its type or an alias of a token of that type.
    /// `None` goes back to the base theme's value. `Ok(None)`: nothing
    /// would change.
    pub fn edit(
        &self,
        themes: &Themes,
        path: &str,
        to: Option<Json>,
    ) -> Result<Option<TokenEdit>, String> {
        if let Some(value) = &to {
            check(themes, &self.base, path, value)?;
        } else if !base_has(themes, &self.base, path) {
            return Err(format!("`{}` has no token `{path}`", self.base));
        }
        let from = self.overrides.get(path).cloned();
        if from == to {
            return Ok(None);
        }
        Ok(Some(TokenEdit {
            path: path.to_string(),
            from,
            to,
        }))
    }

    /// Sets a token without recording it: a gesture in progress, shown
    /// live, recorded once at its end with [`ThemeDraft::record`].
    pub fn preview(&mut self, path: &str, to: Option<Json>) {
        match to {
            Some(v) => self.overrides.insert(path.to_string(), v),
            None => self.overrides.remove(path),
        };
    }

    /// Records an edit already shown (by [`ThemeDraft::preview`]): it
    /// becomes the next to undo, and nothing is left to redo.
    pub fn record(&mut self, edit: TokenEdit) {
        self.push(edit, false);
    }

    /// Records an edit as part of the change recorded just before it (a
    /// palette generated at once): [`ThemeDraft::undo_group`] and
    /// [`ThemeDraft::redo_group`] count them as one.
    pub fn record_joined(&mut self, edit: TokenEdit) {
        let joined = !self.undo.is_empty();
        self.push(edit, joined);
    }

    fn push(&mut self, edit: TokenEdit, joined: bool) {
        self.preview(&edit.path.clone(), edit.to.clone());
        self.undo.push(edit);
        self.joined_undo.push(joined);
        self.redo.clear();
        self.joined_redo.clear();
        self.trim_history();
    }

    /// How many edits the next undo takes back as one change: 1, or the
    /// edits recorded together ([`ThemeDraft::record_joined`]); 0 when
    /// there is nothing to undo.
    pub fn undo_group(&self) -> usize {
        if self.undo.is_empty() {
            return 0;
        }
        1 + self.joined_undo.iter().rev().take_while(|j| **j).count()
    }

    /// How many edits the next redo makes again as one change.
    pub fn redo_group(&self) -> usize {
        if self.redo.is_empty() {
            return 0;
        }
        1 + self
            .joined_redo
            .iter()
            .rev()
            .skip(1)
            .take_while(|j| **j)
            .count()
    }

    /// How many edits the history keeps ([`DRAFT_HISTORY_CAP`] by default).
    pub fn history_cap(&self) -> usize {
        self.cap
    }

    /// Keeps at most `cap` edits to undo (and as many to redo), for this
    /// variant and a pair's other one, dropping the oldest now and
    /// whenever a new edit goes over it. Edits recorded together
    /// ([`ThemeDraft::record_joined`]) go as one: the oldest change goes
    /// whole, and the newest stays whole even when it alone is over the
    /// cap (a generated scale). 0 keeps none.
    pub fn set_history_cap(&mut self, cap: usize) {
        self.cap = cap;
        self.trim_history();
        if let Some(other) = self.other.as_deref_mut() {
            other.set_history_cap(cap);
        }
    }

    fn trim_history(&mut self) {
        let cap = self.cap;
        // `undo` runs oldest first, each change a `false` then its joined
        // `true`s; `redo` runs farthest first, each change its `true`s then
        // its `false`.
        trim(&mut self.undo, &mut self.joined_undo, cap, |j| {
            j.iter()
                .skip(1)
                .position(|j| !*j)
                .map_or(j.len(), |n| n + 1)
        });
        trim(&mut self.redo, &mut self.joined_redo, cap, |j| {
            j.iter().position(|j| !*j).map_or(j.len(), |n| n + 1)
        });
    }

    /// Makes an edit and records it (the same as [`ThemeDraft::record`],
    /// named for edits not previewed first).
    pub fn apply(&mut self, edit: TokenEdit) {
        self.record(edit);
    }

    /// The edits writing a generated scale over one of the base theme's
    /// scales (`blue`, `gray`): its steps 1 to 12 and its `contrast`,
    /// those the base has, each checked as [`ThemeDraft::edit`] does,
    /// those already at that value left out. Record them with
    /// [`ThemeDraft::record`] then [`ThemeDraft::record_joined`] so they
    /// undo as one change.
    ///
    /// ```
    /// use egui_pop_tokens::palette::{Ramp, Scheme};
    /// use egui_pop_tokens::{Color, ThemeDraft, Themes};
    ///
    /// let themes = Themes::default();
    /// let mut draft = ThemeDraft::new("pop-light");
    /// let ramp = Ramp::new(Color::from_hex("#8e44ad").unwrap(), Scheme::Light);
    /// let edits = draft.scale_edits(&themes, "blue", &ramp).unwrap();
    /// assert_eq!(edits.len(), 13);
    /// for (i, e) in edits.into_iter().enumerate() {
    ///     if i == 0 { draft.record(e) } else { draft.record_joined(e) }
    /// }
    /// assert_eq!(draft.undo_group(), 13);
    /// ```
    pub fn scale_edits(
        &self,
        themes: &Themes,
        scale: &str,
        ramp: &crate::palette::Ramp,
    ) -> Result<Vec<TokenEdit>, String> {
        if !base_has(themes, &self.base, &format!("{scale}.9")) {
            return Err(format!("`{}` has no colour scale `{scale}`", self.base));
        }
        let mut edits = Vec::new();
        for (path, value) in ramp.tokens(scale) {
            if !base_has(themes, &self.base, &path) {
                continue;
            }
            if let Some(e) = self.edit(themes, &path, Some(value))? {
                edits.push(e);
            }
        }
        Ok(edits)
    }

    /// The ramp a seed gives for one of the base theme's scales, as a
    /// file giving that scale by its seed is resolved: fitted to the
    /// draft's theme without that scale's own tokens
    /// ([`crate::palette::SEEDS_EXTENSION`]). Write it with
    /// [`ThemeDraft::scale_edits`], then [`ThemeDraft::set_seed`], so the
    /// file saves the seed rather than twelve values.
    pub fn seeded_ramp(&self, themes: &Themes, scale: &str, seed: Seed) -> Ramp {
        let under = themes.flatten(&self.base, &mut Vec::new());
        let one = BTreeMap::from([(scale.to_string(), seed)]);
        let (backdrop, scheme) = seed_backdrop(&under, &own_tokens(&self.overrides), &one);
        Ramp::fitted(seed.color, seed.scheme.unwrap_or(scheme), &backdrop)
    }

    /// Records that a scale was generated from a seed (`None` forgets
    /// it). Not an edit: the scale's values are ([`ThemeDraft::scale_edits`]).
    /// The file saves the seed in place of the scale's values as long as
    /// they are the ones it gives; once changed by hand or undone, the
    /// values are saved as they are.
    pub fn set_seed(&mut self, scale: &str, seed: Option<Seed>) {
        match seed {
            Some(seed) => self.seeds.insert(scale.to_string(), seed),
            None => self.seeds.remove(scale),
        };
    }

    /// The seed a scale was generated from, as recorded.
    pub fn seed(&self, scale: &str) -> Option<Seed> {
        self.seeds.get(scale).copied()
    }

    /// The seeds recorded, by scale.
    pub fn seeds(&self) -> &BTreeMap<String, Seed> {
        &self.seeds
    }

    /// The changes an undo walks back, oldest first: each the edits
    /// recorded together ([`ThemeDraft::record_joined`]), in the order
    /// they were made.
    pub fn undo_changes(&self) -> Vec<Vec<TokenEdit>> {
        let mut out: Vec<Vec<TokenEdit>> = Vec::new();
        for (edit, joined) in self.undo.iter().zip(&self.joined_undo) {
            match out.last_mut() {
                Some(group) if *joined => group.push(edit.clone()),
                _ => out.push(vec![edit.clone()]),
            }
        }
        out
    }

    /// The changes a redo makes again, the next one last, each in the
    /// order its edits were made.
    pub fn redo_changes(&self) -> Vec<Vec<TokenEdit>> {
        let mut out = Vec::new();
        let mut end = self.redo.len();
        while end > 0 {
            // The change's first edit is the nearest; the edits joined
            // to it lie before it in the list.
            let first = end - 1;
            let mut start = first;
            while start > 0 && self.joined_redo.get(start - 1).copied().unwrap_or(false) {
                start -= 1;
            }
            out.push(self.redo[start..=first].iter().rev().cloned().collect());
            end = start;
        }
        out.reverse();
        out
    }

    /// The edit an undo would make (the last one's inverse).
    pub fn undo_edit(&self) -> Option<TokenEdit> {
        self.undo.last().map(TokenEdit::inverse)
    }

    /// The edit a redo would make.
    pub fn redo_edit(&self) -> Option<TokenEdit> {
        self.redo.last().cloned()
    }

    /// Takes the last edit back; returns the edit made (its inverse).
    pub fn undo(&mut self) -> Option<TokenEdit> {
        let edit = self.undo.pop()?;
        let joined = self.joined_undo.pop().unwrap_or(false);
        self.preview(&edit.path, edit.from.clone());
        let made = edit.inverse();
        self.redo.push(edit);
        self.joined_redo.push(joined);
        Some(made)
    }

    /// Makes the last undone edit again; returns it.
    pub fn redo(&mut self) -> Option<TokenEdit> {
        let edit = self.redo.pop()?;
        let joined = self.joined_redo.pop().unwrap_or(false);
        self.preview(&edit.path, edit.to.clone());
        self.undo.push(edit.clone());
        self.joined_undo.push(joined);
        Some(edit)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// The edits made, oldest first (what an undo takes back last).
    pub fn history(&self) -> &[TokenEdit] {
        &self.undo
    }

    /// The draft as a token file named `name`: `$extends` its base, its
    /// overrides with their types, stamped with the current format
    /// ([`crate::FormatStamp`]). A scale generated from a seed
    /// ([`ThemeDraft::set_seed`]) is saved as its seed
    /// ([`crate::palette::SEEDS_EXTENSION`]): the steps whose value is the
    /// one the seed gives are left out (regenerated when the file is
    /// loaded), a step changed since is written as it is. A seed none of
    /// whose steps still has its value (undone, or the theme changed
    /// under it) is not saved. Loading the file gives the draft's values.
    ///
    /// A pair is saved as a new pair: the file is the theme named first's,
    /// named `name`, and carries its counterpart, named `name` and its
    /// mode (`ocean-light`), each naming the other
    /// ([`TokenFile::set_counterpart_file`]); loaded with
    /// [`Themes::add_text`], both are added.
    pub fn to_file(&self, themes: &Themes, name: &str) -> TokenFile {
        let (first, second) = self.variants();
        let mut file = first.variant_file(themes, name);
        if let Some(second) = second {
            let mode = if is_dark(themes, &second.base) {
                "dark"
            } else {
                "light"
            };
            file.set_counterpart_file(Some(second.variant_file(themes, &format!("{name}-{mode}"))));
        }
        file
    }

    /// The variant edited as a token file, alone.
    fn variant_file(&self, themes: &Themes, name: &str) -> TokenFile {
        let base = themes.resolve(&ThemeSpec::new(self.base.clone()));
        let mut file = TokenFile::new(name);
        file.description = Some(format!("Edited from {}", self.base));
        file.extends = Some(self.base.clone());
        file.stamp();
        for (path, value) in &self.overrides {
            file.tokens.insert(
                path.clone(),
                RawToken {
                    ty: base.get(path).map(|t| t.ty.clone()),
                    value: value.clone(),
                    description: None,
                    extensions: None,
                },
            );
        }
        if self.seeds.is_empty() {
            return file;
        }
        // The seeds still giving the draft's values, until that holds for
        // every seed kept (which seeds are kept changes what the others
        // are fitted to).
        let under = themes.flatten(&self.base, &mut Vec::new());
        let own = own_tokens(&self.overrides);
        let mut kept = self.seeds.clone();
        let generated = loop {
            let generated = seeded_scales(&under, &own, &kept);
            let stale: Vec<String> = kept
                .keys()
                .filter(|scale| {
                    !generated.iter().any(|(path, t)| {
                        in_scales(std::iter::once(*scale), path)
                            && self.overrides.get(path) == Some(&t.value)
                    })
                })
                .cloned()
                .collect();
            if stale.is_empty() {
                break generated;
            }
            for scale in stale {
                kept.remove(&scale);
            }
        };
        for (path, t) in generated {
            match self.overrides.get(&path) {
                Some(v) if *v == t.value => {
                    file.tokens.remove(&path);
                }
                Some(_) => {}
                // The base's value, which the seed would replace.
                None => {
                    if let Some(raw) = under.get(&path) {
                        file.tokens.insert(path, raw.clone());
                    }
                }
            }
        }
        for (scale, seed) in kept {
            file.set_seed(&scale, Some(seed));
        }
        file
    }

    /// The draft as the text of a `.tokens.json` file.
    pub fn export(&self, themes: &Themes, name: &str) -> String {
        self.to_file(themes, name).to_json_pretty()
    }

    /// Reads a token file as a draft: its `$extends` becomes the base
    /// (the fallback theme when it has none), its tokens the overrides.
    /// The file loads over its base as any theme file does
    /// ([`Themes::check_file`]): brought to the current contract, a token
    /// whose value does not check left out, the base's value kept. A
    /// draft changes its base's tokens only, so a token the base does
    /// not have is left out too. Each is a warning, as is a file stamped
    /// newer than this egui-pop knows (read best-effort). Only an
    /// unreadable file or an unknown base is an error.
    ///
    /// A file carrying its counterpart ([`COUNTERPART_EXTENSION`]) is a
    /// pair: the draft holds both variants, the file's own edited first.
    /// A counterpart that cannot be read, of an unknown base or of the
    /// same mode is a warning, the file's variant read alone.
    pub fn import(
        themes: &Themes,
        name: &str,
        text: &str,
    ) -> Result<(ThemeDraft, Vec<Warning>), Error> {
        let file = TokenFile::parse(name, text)?;
        let counterpart = file.counterpart_file();
        let (mut draft, mut warnings) = ThemeDraft::import_file(themes, file)?;
        let at = format!("$extensions.{COUNTERPART_EXTENSION}");
        match counterpart.map(|c| c.and_then(|c| ThemeDraft::import_file(themes, c))) {
            None => {}
            Some(Ok((other, more))) => {
                if is_dark(themes, &other.base) == is_dark(themes, &draft.base) {
                    warnings.push(Warning::Invalid {
                        path: at,
                        reason: format!(
                            "`{}` and `{}` are both light or both dark",
                            draft.base, other.base
                        ),
                    });
                } else {
                    warnings.extend(more);
                    draft.other = Some(Box::new(other));
                }
            }
            Some(Err(e)) => warnings.push(Warning::Invalid {
                path: at,
                reason: e.to_string(),
            }),
        }
        Ok((draft, warnings))
    }

    /// Reads one variant's token file as a draft alone.
    fn import_file(
        themes: &Themes,
        mut file: TokenFile,
    ) -> Result<(ThemeDraft, Vec<Warning>), Error> {
        let mut warnings = file.upgrade();
        if let Some(v) = file
            .extensions
            .as_ref()
            .and_then(|x| x.get(LEGACY_EXTENSION))
            .and_then(|x| x.get("version"))
            .and_then(Json::as_u64)
            && v > LEGACY_VERSION
        {
            warnings.push(Warning::Newer {
                path: format!("$extensions.{LEGACY_EXTENSION}.version"),
                found: v,
                known: LEGACY_VERSION,
            });
        }
        let base = file
            .extends
            .clone()
            .unwrap_or_else(|| FALLBACK_THEME.to_string());
        if themes.get(&base).is_none() {
            return Err(Error::UnknownTheme { name: base });
        }
        let resolved = themes.resolve(&ThemeSpec::new(base.clone()));
        let mut draft = ThemeDraft::new(base);
        let (mut seeds, seed_warnings) = file.seeds();
        warnings.extend(seed_warnings);
        // A draft changes its base's scales only.
        seeds.retain(|scale, _| {
            let known = resolved.get(&format!("{scale}.1")).is_some();
            if !known {
                warnings.push(Warning::Unknown {
                    path: format!("$extensions.{}.{scale}", crate::palette::SEEDS_EXTENSION),
                    base: draft.base.clone(),
                    kept: false,
                });
            }
            known
        });
        for (path, token) in file.tokens {
            if resolved.get(&path).is_none() {
                warnings.push(Warning::Unknown {
                    path,
                    base: draft.base.clone(),
                    kept: false,
                });
                continue;
            }
            match check(themes, &draft.base, &path, &token.value) {
                Ok(()) => {
                    draft.overrides.insert(path, token.value);
                }
                Err(reason) => warnings.push(Warning::Invalid { path, reason }),
            }
        }
        // The seeded scales' steps the file does not give, as the file
        // resolves them.
        if !seeds.is_empty() {
            let under = themes.flatten(&draft.base, &mut Vec::new());
            for (path, t) in seeded_scales(&under, &own_tokens(&draft.overrides), &seeds) {
                if !draft.overrides.contains_key(&path) && resolved.get(&path).is_some() {
                    draft.overrides.insert(path, t.value);
                }
            }
            draft.seeds = seeds;
        }
        Ok((draft, warnings))
    }
}

/// A draft's overrides as raw tokens (their types from the tokens they
/// override).
/// Drops the oldest changes of a history until it holds at most `cap`
/// edits, each change whole: `first` gives where the oldest change ends.
/// The newest change stays, even over the cap, unless the cap is 0.
fn trim(
    edits: &mut Vec<TokenEdit>,
    joined: &mut Vec<bool>,
    cap: usize,
    first: impl Fn(&[bool]) -> usize,
) {
    while edits.len() > cap {
        let end = first(joined).min(edits.len());
        if end == edits.len() && cap > 0 {
            break;
        }
        edits.drain(..end);
        joined.drain(..end.min(joined.len()));
    }
}

fn own_tokens(overrides: &BTreeMap<String, Json>) -> BTreeMap<String, RawToken> {
    overrides
        .iter()
        .map(|(path, value)| {
            (
                path.clone(),
                RawToken {
                    ty: None,
                    value: value.clone(),
                    description: None,
                    extensions: None,
                },
            )
        })
        .collect()
}

/// Whether a theme is a dark one (its panels are), alone.
fn is_dark(themes: &Themes, name: &str) -> bool {
    themes.resolve(&ThemeSpec::new(name).alone()).is_dark()
}

/// Whether the base theme has the token.
fn base_has(themes: &Themes, base: &str, path: &str) -> bool {
    themes
        .resolve(&ThemeSpec::new(base.to_string()))
        .get(path)
        .is_some()
}

/// Whether `value` may be `path`'s in the base theme.
fn check(themes: &Themes, base: &str, path: &str, value: &Json) -> Result<(), String> {
    let resolved = themes.resolve(&ThemeSpec::new(base.to_string()));
    let token = resolved
        .get(path)
        .ok_or_else(|| format!("`{base}` has no token `{path}`"))?;
    match alias_target(value) {
        Some(target) => {
            let to = resolved
                .get(target)
                .ok_or_else(|| format!("`{target}` is not a token of `{base}`"))?;
            if target == path {
                return Err(format!("`{path}` cannot alias itself"));
            }
            if to.ty != token.ty {
                return Err(format!(
                    "`{target}` is a {}, `{path}` a {}",
                    to.ty, token.ty
                ));
            }
            // An alias back to the token being edited would loop.
            let mut at = to;
            let mut seen = 0;
            while let Some(next) = at.alias.as_deref() {
                if next == path || seen > 64 {
                    return Err(format!("`{target}` leads back to `{path}`"));
                }
                match resolved.get(next) {
                    Some(t) => at = t,
                    None => break,
                }
                seen += 1;
            }
            Ok(())
        }
        None => Value::parse(&token.ty, value).map(|_| ()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn themes() -> Themes {
        Themes::default()
    }

    #[test]
    fn edits_undo_and_redo_in_order() {
        let t = themes();
        let mut d = ThemeDraft::new("pop-dark");
        let a = d
            .edit(&t, "surface.panel", Some(json!("#101010")))
            .unwrap()
            .unwrap();
        d.apply(a);
        let b = d
            .edit(&t, "surface.panel", Some(json!("#202020")))
            .unwrap()
            .unwrap();
        assert_eq!(b.from, Some(json!("#101010")));
        d.apply(b);
        assert_eq!(d.value("surface.panel"), Some(&json!("#202020")));
        let undone = d.undo().unwrap();
        assert_eq!(undone.to, Some(json!("#101010")));
        d.undo();
        assert!(d.overrides().is_empty());
        assert!(!d.can_undo());
        d.redo();
        assert_eq!(d.value("surface.panel"), Some(&json!("#101010")));
        // A new edit drops what was left to redo.
        let c = d.edit(&t, "text.default", Some(json!("#eeeeee"))).unwrap();
        d.apply(c.unwrap());
        assert!(!d.can_redo());
    }

    #[test]
    fn the_history_keeps_its_newest_edits_up_to_its_cap() {
        let t = themes();
        let mut d = ThemeDraft::new("pop-dark");
        assert_eq!(d.history_cap(), DRAFT_HISTORY_CAP);
        let grey = |n: usize| json!(format!("#{n:06x}"));
        for n in 1..=DRAFT_HISTORY_CAP + 5 {
            let e = d.edit(&t, "surface.panel", Some(grey(n))).unwrap();
            d.apply(e.unwrap());
        }
        assert_eq!(
            d.history().len(),
            DRAFT_HISTORY_CAP,
            "the oldest five dropped"
        );
        assert_eq!(d.history()[0].to, Some(grey(6)));
        d.undo();
        d.set_history_cap(2);
        assert_eq!(d.history().len(), 2);
        assert!(d.can_redo());
        d.undo();
        d.undo();
        assert!(!d.can_undo());
        assert_eq!(d.value("surface.panel"), Some(&grey(DRAFT_HISTORY_CAP + 2)));
    }

    #[test]
    fn the_cap_drops_and_keeps_joined_edits_as_one_change() {
        let edit = |path: &str| TokenEdit {
            path: path.to_string(),
            from: None,
            to: Some(json!("#000000")),
        };
        let mut d = ThemeDraft::pair("pop-dark", "pop-light");
        d.record(edit("a"));
        d.record(edit("b"));
        d.record(edit("g1"));
        d.record_joined(edit("g2"));
        d.record_joined(edit("g3"));
        // A cap of two drops `a` and `b`, and keeps the newest change
        // whole although it alone is over the cap.
        d.set_history_cap(2);
        assert_eq!(d.history().len(), 3);
        assert_eq!(d.undo_group(), 3);
        assert_eq!(d.counterpart().unwrap().history_cap(), 2);
        // The redo list, farthest first: c, then the joined change.
        d.set_history_cap(10);
        d.record(edit("c"));
        for _ in 0..4 {
            d.undo();
        }
        assert_eq!(d.redo_group(), 3);
        d.set_history_cap(3);
        assert_eq!(d.redo_group(), 3);
        assert_eq!(d.redo_edit().unwrap().path, "g1");
        d.set_history_cap(1);
        assert_eq!(d.redo_group(), 3, "the nearest change stays whole");
        d.set_history_cap(0);
        assert!(!d.can_undo() && !d.can_redo());
    }

    #[test]
    fn values_are_checked_against_the_token_type() {
        let t = themes();
        let d = ThemeDraft::new("pop-dark");
        assert!(d.edit(&t, "surface.panel", Some(json!("blue"))).is_err());
        assert!(d.edit(&t, "surface.panel", Some(json!(4))).is_err());
        assert!(d.edit(&t, "no.such", Some(json!("#000000"))).is_err());
        assert!(
            d.edit(&t, "surface.panel", Some(json!("{space.m}")))
                .is_err()
        );
        assert!(
            d.edit(&t, "surface.panel", Some(json!("{surface.panel}")))
                .is_err()
        );
        assert!(
            d.edit(&t, "gray.2", Some(json!("{surface.panel}")))
                .is_err(),
            "surface.panel aliases gray.2: a loop"
        );
        assert!(
            d.edit(&t, "surface.panel", Some(json!("{gray.3}")))
                .unwrap()
                .is_some()
        );
        assert!(
            d.edit(
                &t,
                "radius.widget",
                Some(json!({ "value": 2, "unit": "px" }))
            )
            .unwrap()
            .is_some()
        );
        // Going back to the base value when nothing is changed: no edit.
        assert_eq!(d.edit(&t, "surface.panel", None), Ok(None));
    }

    #[test]
    fn a_draft_exports_and_imports_as_a_token_file() {
        let t = themes();
        let mut d = ThemeDraft::new("pop-light");
        for (path, v) in [
            ("accent.default", json!("#2060c0")),
            ("radius.widget", json!({ "value": 2, "unit": "px" })),
            ("button.rest.border-color", json!("{border.strong}")),
        ] {
            let e = d.edit(&t, path, Some(v)).unwrap().unwrap();
            d.apply(e);
        }
        let text = d.export(&t, "mine");
        let file = TokenFile::parse("mine", &text).unwrap();
        assert_eq!(file.extends.as_deref(), Some("pop-light"));
        assert_eq!(file.tokens["accent.default"].ty.as_deref(), Some("color"));
        assert!(text.contains(crate::FORMAT_EXTENSION));
        let (back, warnings) = ThemeDraft::import(&t, "mine", &text).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(back.base(), "pop-light");
        assert_eq!(back.overrides(), d.overrides());
        // The file resolves as a theme like any other.
        let mut with = t.clone();
        assert_eq!(with.add_text("mine", &text), Ok(vec![]));
        let r = with.resolve(&ThemeSpec::new("mine"));
        assert!(r.problems().is_empty(), "{:?}", r.problems());
        assert_eq!(r.color("accent.default"), crate::Color::from_hex("#2060c0"));
    }

    #[test]
    fn imports_leave_out_what_does_not_fit_and_refuse_unknown_bases() {
        let t = themes();
        let (d, warnings) = ThemeDraft::import(
            &t,
            "x",
            r##"{ "surface": { "$type": "color", "panel": { "$value": "#000000" }, "nope": { "$value": "#000000" }, "sunken": { "$value": "blue" } } }"##,
        )
        .unwrap();
        assert_eq!(d.base(), FALLBACK_THEME);
        assert_eq!(d.value("surface.panel"), Some(&json!("#000000")));
        let left: Vec<&str> = warnings.iter().filter_map(Warning::path).collect();
        assert_eq!(left, ["surface.nope", "surface.sunken"]);
        assert!(warnings.iter().all(Warning::left_out));
        assert!(
            warnings[1]
                .to_string()
                .starts_with("`surface.sunken` left out"),
            "{}",
            warnings[1]
        );
        assert!(matches!(
            ThemeDraft::import(&t, "x", r#"{ "$extends": "nowhere" }"#),
            Err(Error::UnknownTheme { .. })
        ));
        // A file from a newer editor, or for a newer contract, loads
        // best-effort and says so.
        for newer in [
            format!(
                r#"{{ "$extends": "pop-dark", "$extensions": {{ "{LEGACY_EXTENSION}": {{ "version": 99 }} }} }}"#
            ),
            format!(
                r#"{{ "$extends": "pop-dark", "$extensions": {{ "{}": {{ "version": 1, "contract": 99 }} }} }}"#,
                crate::FORMAT_EXTENSION
            ),
        ] {
            let (d, warnings) = ThemeDraft::import(&t, "x", &newer).unwrap();
            assert_eq!(d.base(), "pop-dark");
            assert!(
                matches!(&warnings[..], [Warning::Newer { found: 99, .. }]),
                "{warnings:?}"
            );
        }
    }

    #[test]
    fn joined_edits_undo_and_redo_as_one_change() {
        let themes = Themes::default();
        let mut d = ThemeDraft::new("pop-dark");
        let e =
            |d: &ThemeDraft, p: &str, v: &str| d.edit(&themes, p, Some(json!(v))).unwrap().unwrap();
        let first = e(&d, "accent.default", "#102030");
        d.record(first);
        let a = e(&d, "gold.1", "#111111");
        d.record(a);
        let b = e(&d, "gold.2", "#222222");
        d.record_joined(b);
        assert_eq!(d.undo_group(), 2);
        d.undo();
        d.undo();
        assert_eq!(d.undo_group(), 1);
        assert_eq!(d.redo_group(), 2);
        d.redo();
        d.redo();
        assert_eq!(d.redo_group(), 0);
        assert_eq!(d.undo_group(), 2);
        // A new edit after an undo starts a group of its own.
        d.undo();
        d.undo();
        let c = e(&d, "gold.3", "#333333");
        d.record(c);
        assert_eq!((d.undo_group(), d.redo_group()), (1, 0));
    }

    #[test]
    fn the_changes_list_joined_edits_as_one_in_the_order_made() {
        let themes = Themes::default();
        let mut d = ThemeDraft::new("pop-dark");
        let e =
            |d: &ThemeDraft, p: &str, v: &str| d.edit(&themes, p, Some(json!(v))).unwrap().unwrap();
        let first = e(&d, "accent.default", "#102030");
        d.record(first);
        let a = e(&d, "gold.1", "#111111");
        d.record(a);
        let b = e(&d, "gold.2", "#222222");
        d.record_joined(b);
        let paths = |changes: Vec<Vec<TokenEdit>>| -> Vec<Vec<String>> {
            changes
                .into_iter()
                .map(|c| c.into_iter().map(|e| e.path).collect())
                .collect()
        };
        assert_eq!(
            paths(d.undo_changes()),
            [vec!["accent.default"], vec!["gold.1", "gold.2"]]
        );
        assert!(d.redo_changes().is_empty());
        // Both undone: the scale is the next redo, the accent after it.
        d.undo();
        d.undo();
        d.undo();
        assert_eq!(
            paths(d.redo_changes()),
            [vec!["gold.1", "gold.2"], vec!["accent.default"]]
        );
        d.redo();
        assert_eq!(paths(d.redo_changes()), [vec!["gold.1", "gold.2"]]);
        assert_eq!(paths(d.undo_changes()), [vec!["accent.default"]]);
    }

    /// Generates `scale` from `hex` as the theme editor does: the edits,
    /// one change, and the seed recorded.
    fn generate(d: &mut ThemeDraft, themes: &Themes, scale: &str, hex: &str) {
        use crate::Color;
        use crate::palette::Scheme;
        let seed = Seed::new(Color::from_hex(hex).unwrap(), Scheme::Light);
        let ramp = d.seeded_ramp(themes, scale, seed);
        let edits = d.scale_edits(themes, scale, &ramp).unwrap();
        assert!(!edits.is_empty());
        for (i, e) in edits.into_iter().enumerate() {
            if i == 0 {
                d.record(e)
            } else {
                d.record_joined(e)
            }
        }
        d.set_seed(scale, Some(seed));
    }

    #[test]
    fn a_generated_scale_is_saved_as_its_seed() {
        let t = themes();
        let mut d = ThemeDraft::new("pop-light");
        let e = d.edit(&t, "surface.panel", Some(json!("#f4f0ea"))).unwrap();
        d.apply(e.unwrap());
        generate(&mut d, &t, "blue", "#8e44ad");
        let file = d.to_file(&t, "mine");
        // The seed, not the twelve values.
        assert!(
            !file.tokens.keys().any(|p| p.starts_with("blue.")),
            "{:?}",
            file.tokens.keys()
        );
        assert_eq!(file.seeds().0.keys().collect::<Vec<_>>(), ["blue"]);
        assert!(file.tokens.contains_key("surface.panel"));
        // Loaded as a theme, the file gives the draft's values.
        let text = file.to_json_pretty();
        let mut with = t.clone();
        assert_eq!(with.add_text("mine", &text), Ok(vec![]));
        let loaded = with.resolve(&ThemeSpec::new("mine"));
        let drafted = t.resolve(&d.spec(&ThemeSpec::new("pop-light")));
        for i in 1..=12 {
            let path = format!("blue.{i}");
            assert_eq!(loaded.color(&path), drafted.color(&path), "{path}");
        }
        assert_eq!(
            loaded.color("blue.contrast"),
            drafted.color("blue.contrast")
        );
        // Imported, it is the same draft, its seed recorded.
        let (back, warnings) = ThemeDraft::import(&t, "mine", &text).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(back.overrides(), d.overrides());
        assert_eq!(back.seed("blue"), d.seed("blue"));
    }

    #[test]
    fn a_seeded_scale_changed_or_undone_is_saved_as_it_is() {
        let t = themes();
        let mut d = ThemeDraft::new("pop-light");
        generate(&mut d, &t, "blue", "#8e44ad");
        // A step changed by hand is written; the others still come from
        // the seed.
        let e = d.edit(&t, "blue.4", Some(json!("#010203"))).unwrap();
        d.apply(e.unwrap());
        let file = d.to_file(&t, "mine");
        assert_eq!(file.tokens["blue.4"].value, json!("#010203"));
        assert!(!file.tokens.contains_key("blue.9"));
        assert!(file.seeds().0.contains_key("blue"));
        let (back, _) = ThemeDraft::import(&t, "mine", &file.to_json_pretty()).unwrap();
        assert_eq!(back.overrides(), d.overrides());
        // Undone, the seed is not saved: the file gives the base's scale.
        d.undo();
        while d.undo_group() > 0 {
            for _ in 0..d.undo_group() {
                d.undo();
            }
        }
        assert!(d.overrides().is_empty());
        let file = d.to_file(&t, "mine");
        assert!(file.seeds().0.is_empty());
        assert!(file.tokens.is_empty(), "{:?}", file.tokens.keys());
    }

    #[test]
    fn a_seed_for_a_scale_the_base_lacks_is_left_out() {
        let t = themes();
        let (d, warnings) = ThemeDraft::import(
            &t,
            "x",
            r##"{ "$extends": "pop-dark", "$extensions": { "org.egui-pop.seeds": {
                "brick": { "seed": "#c0392b" }, "blue": { "seed": "#c0392b", "scheme": "dusk" } } } }"##,
        )
        .unwrap();
        assert!(d.overrides().is_empty());
        assert!(d.seeds().is_empty());
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(warnings.iter().all(Warning::left_out), "{warnings:?}");
    }

    #[test]
    fn a_scale_is_written_from_a_ramp() {
        use crate::Color;
        use crate::palette::{Ramp, Scheme};
        let themes = Themes::default();
        let d = ThemeDraft::new("pop-dark");
        let ramp = Ramp::new(Color::from_hex("#27ae60").unwrap(), Scheme::Dark);
        let edits = d.scale_edits(&themes, "green", &ramp).unwrap();
        assert_eq!(edits.len(), 13);
        assert!(d.scale_edits(&themes, "nope", &ramp).is_err());
        // The neutral scale has no `contrast`: its twelve steps only.
        let grey = Ramp::new(Color::from_hex("#808080").unwrap(), Scheme::Dark);
        assert_eq!(d.scale_edits(&themes, "gray", &grey).unwrap().len(), 12);
    }

    #[test]
    fn a_pair_draft_holds_both_variants_edits() {
        let t = themes();
        let mut d = ThemeDraft::for_theme(&t, "pop-dark");
        assert!(d.is_paired());
        assert!(!d.editing_second());
        let e = d.edit(&t, "danger", Some(json!("#ff4040"))).unwrap();
        d.apply(e.unwrap());
        let e = d.edit(
            &t,
            "radius.widget",
            Some(json!({ "value": 2, "unit": "px" })),
        );
        d.apply(e.unwrap().unwrap());
        assert!(d.switch());
        assert!(d.editing_second());
        assert_eq!(d.base(), "pop-light");
        assert!(d.overrides().is_empty());
        assert!(!d.can_undo());
        // Checked against the variant edited's base.
        let e = d.edit(&t, "danger", Some(json!("#a00000"))).unwrap();
        d.apply(e.unwrap());
        assert_eq!(d.counterpart().unwrap().overrides().len(), 2);
        // Back: the first variant's edits and history are kept.
        assert!(d.switch());
        assert_eq!(d.base(), "pop-dark");
        assert_eq!(d.value("danger"), Some(&json!("#ff4040")));
        assert_eq!(d.history().len(), 2);
        d.undo();
        assert_eq!(d.overrides().len(), 1);
        assert_eq!(
            d.counterpart().unwrap().value("danger"),
            Some(&json!("#a00000"))
        );
        let (first, second) = d.variants();
        assert_eq!(
            (first.base(), second.map(ThemeDraft::base)),
            ("pop-dark", Some("pop-light"))
        );
        // A theme alone has nothing to switch to.
        let mut alone = ThemeDraft::for_theme(&t, "pop-high-contrast");
        assert!(!alone.is_paired());
        assert!(!alone.switch());
        assert_eq!(alone.base(), "pop-high-contrast");
    }

    #[test]
    fn a_pair_draft_comes_from_a_spec_and_installs_as_a_pair() {
        let t = themes();
        // The spec's overrides apply to both, until the counterpart has
        // its own.
        let spec = ThemeSpec::new("gruvbox-light").override_token("danger", json!("#ff0000"));
        let d = ThemeDraft::from_pair_spec(&t, &spec);
        assert_eq!(d.base(), "gruvbox-light");
        let other = d.counterpart().unwrap();
        assert_eq!(other.base(), "gruvbox-dark");
        assert_eq!(other.value("danger"), Some(&json!("#ff0000")));
        let spec = spec.counterpart_override("info", json!("#0000ff"));
        let mut d = ThemeDraft::from_pair_spec(&t, &spec);
        assert_eq!(
            d.counterpart()
                .unwrap()
                .overrides()
                .keys()
                .collect::<Vec<_>>(),
            ["info"]
        );
        // Installed: the theme named first stays first, whichever is
        // edited; the variant edited resolves alone.
        d.switch();
        let installed = d.pair_spec(&ThemeSpec::new("pop-dark").density(crate::Density::Compact));
        assert_eq!(installed.base, "gruvbox-light");
        assert_eq!(
            installed.counterpart,
            Counterpart::Theme("gruvbox-dark".into())
        );
        assert_eq!(installed.density, crate::Density::Compact);
        assert_eq!(
            installed.counterpart_overrides.as_ref().map(BTreeMap::len),
            Some(1)
        );
        let resolved = t.resolve_counterpart(&installed).unwrap();
        assert_eq!(resolved.color("info"), crate::Color::from_hex("#0000ff"));
        assert_eq!(d.spec(&installed).base, "gruvbox-dark");
        // A counterpart of the same mode is not one; alone, no
        // counterpart is installed.
        let d = ThemeDraft::from_pair_spec(&t, &ThemeSpec::pair("pop-dark", "gruvbox-dark"));
        assert!(!d.is_paired());
        assert_eq!(
            d.pair_spec(&ThemeSpec::default()).counterpart,
            Counterpart::None
        );
    }

    #[test]
    fn a_pair_is_saved_as_a_new_pair_and_imported_back() {
        let t = themes();
        let mut d = ThemeDraft::for_theme(&t, "pop-dark");
        let e = d
            .edit(&t, "accent.default", Some(json!("#40a0ff")))
            .unwrap();
        d.apply(e.unwrap());
        d.switch();
        let e = d
            .edit(&t, "accent.default", Some(json!("#2060c0")))
            .unwrap();
        d.apply(e.unwrap());
        generate(&mut d, &t, "blue", "#8e44ad");
        // Saved from either variant: the theme named first's file,
        // carrying its counterpart.
        let file = d.to_file(&t, "ocean");
        assert_eq!(file.name, "ocean");
        assert_eq!(file.extends.as_deref(), Some("pop-dark"));
        assert_eq!(file.tokens["accent.default"].value, json!("#40a0ff"));
        let x = file.extensions.as_ref().unwrap();
        assert_eq!(x[crate::VARIANT_OF_EXTENSION], json!("ocean-light"));
        let inner = file.counterpart_file().unwrap().unwrap();
        assert_eq!(inner.name, "ocean-light");
        assert_eq!(inner.extends.as_deref(), Some("pop-light"));
        assert_eq!(inner.tokens["accent.default"].value, json!("#2060c0"));
        assert_eq!(inner.seeds().0.keys().collect::<Vec<_>>(), ["blue"]);
        let ix = inner.extensions.as_ref().unwrap();
        assert_eq!(ix[crate::VARIANT_OF_EXTENSION], json!("ocean"));
        assert!(ix.get(crate::FORMAT_EXTENSION).is_some(), "its own stamp");

        // Imported: the same pair, the file's variant edited first.
        let text = file.to_json_pretty();
        let (back, warnings) = ThemeDraft::import(&t, "ocean", &text).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(!back.editing_second());
        let (first, second) = back.variants();
        let (want_first, want_second) = d.variants();
        assert_eq!(first.overrides(), want_first.overrides());
        let second = second.unwrap();
        assert_eq!(second.base(), "pop-light");
        assert_eq!(second.overrides(), want_second.unwrap().overrides());
        assert_eq!(second.seed("blue"), want_second.unwrap().seed("blue"));

        // Loaded in an application: both themes, a pair.
        let mut with = t.clone();
        assert_eq!(with.add_text("ocean", &text), Ok(vec![]));
        assert_eq!(with.counterpart_of("ocean"), Some("ocean-light"));
        assert_eq!(with.counterpart_of("ocean-light"), Some("ocean"));
        let light = with.resolve_counterpart(&ThemeSpec::new("ocean")).unwrap();
        assert!(light.problems().is_empty(), "{:?}", light.problems());
        assert_eq!(
            light.color("accent.default"),
            crate::Color::from_hex("#2060c0")
        );
    }

    #[test]
    fn a_counterpart_that_does_not_fit_is_left_out() {
        let t = themes();
        for (inner, said) in [
            (r#"{ "name": "x-light" }"#, "no `file`"),
            (
                r#"{ "name": "x-dark", "file": { "$extends": "gruvbox-dark" } }"#,
                "both light or both dark",
            ),
            (
                r#"{ "name": "x-light", "file": { "$extends": "nowhere" } }"#,
                "nowhere",
            ),
        ] {
            let text = format!(
                r#"{{ "$extends": "pop-dark", "$extensions": {{ "{COUNTERPART_EXTENSION}": {inner} }} }}"#
            );
            let (d, warnings) = ThemeDraft::import(&t, "x", &text).unwrap();
            assert!(!d.is_paired(), "{inner}");
            assert_eq!(d.base(), "pop-dark");
            assert_eq!(warnings.len(), 1, "{warnings:?}");
            assert!(warnings[0].to_string().contains(said), "{}", warnings[0]);
        }
    }

    #[test]
    fn the_other_variant_is_generated_from_the_one_edited() {
        use crate::Color;
        use crate::palette::{Scheme, mirror};
        let t = themes();
        let mut d = ThemeDraft::for_theme(&t, "pop-dark");
        assert!(
            ThemeDraft::new("pop-dark")
                .generate_counterpart(&t)
                .is_none()
        );
        for (path, v) in [
            ("surface.panel", json!("#000000")),
            ("danger", json!("#1a3a6e")),
            ("button.rest.border-color", json!("{border.strong}")),
            ("radius.widget", json!({ "value": 2, "unit": "px" })),
        ] {
            let e = d.edit(&t, path, Some(v)).unwrap().unwrap();
            d.apply(e);
        }
        // A seeded scale, for the dark scheme.
        let seed = Seed::new(Color::from_hex("#27ae60").unwrap(), Scheme::Dark);
        let ramp = d.seeded_ramp(&t, "green", seed);
        for e in d.scale_edits(&t, "green", &ramp).unwrap() {
            d.record_joined(e);
        }
        d.set_seed("green", Some(seed));
        // The other variant has a change of its own, not carried over.
        d.switch();
        let e = d.edit(&t, "info", Some(json!("#00ffff"))).unwrap();
        d.apply(e.unwrap());
        d.switch();

        let g = d.generate_counterpart(&t).unwrap();
        assert!(g.skipped.is_empty(), "{:?}", g.skipped);
        let to = |path: &str| {
            g.edits
                .iter()
                .find(|e| e.path == path)
                .unwrap_or_else(|| panic!("no edit of {path}"))
                .to
                .clone()
        };
        assert_eq!(to("surface.panel"), Some(json!("#ffffff")));
        let deep = Color::from_hex("#1a3a6e").unwrap();
        assert_eq!(to("danger"), Some(json!(mirror(deep).to_hex())));
        assert_eq!(
            to("button.rest.border-color"),
            Some(json!("{border.strong}"))
        );
        assert_eq!(
            to("radius.widget"),
            Some(json!({ "value": 2, "unit": "px" }))
        );
        assert_eq!(to("info"), None, "taken back");
        // The seed for the light scheme, its ramp fitted to the other
        // variant as generated (white panels).
        let light = Seed::new(seed.color, Scheme::Light);
        assert_eq!(g.seeds, BTreeMap::from([("green".to_string(), light)]));
        let mut fitted = ThemeDraft::new("pop-light");
        fitted.preview("surface.panel", Some(json!("#ffffff")));
        fitted.preview("danger", to("danger"));
        fitted.preview("button.rest.border-color", Some(json!("{border.strong}")));
        fitted.preview("radius.widget", Some(json!({ "value": 2, "unit": "px" })));
        for (path, value) in fitted.seeded_ramp(&t, "green", light).tokens("green") {
            assert_eq!(to(&path), Some(value), "{path}");
        }
        // Deterministic: the same proposal again.
        assert_eq!(d.generate_counterpart(&t), Some(g.clone()));

        // Recorded on the other variant: one change, and it becomes the
        // edited one's counterpart.
        d.switch();
        for (i, e) in g.edits.iter().cloned().enumerate() {
            if i == 0 {
                d.record(e)
            } else {
                d.record_joined(e)
            }
        }
        assert_eq!(d.undo_group(), g.edits.len());
        assert_eq!(d.value("surface.panel"), Some(&json!("#ffffff")));
        assert_eq!(d.value("info"), None);
        let r = t.resolve(&d.spec(&ThemeSpec::default()));
        assert!(r.problems().is_empty(), "{:?}", r.problems());
    }
}
