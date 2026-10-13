//! A theme being edited: a base theme, the tokens changed over it, and
//! the history of those changes. What a theme editor works on, with no
//! egui in it.
//!
//! A draft is saved as an ordinary token file ([`TokenFile`]): `$extends`
//! names the base theme, and the file gives the changed tokens only, with
//! their types. Any file egui-pop loads can be read back as a draft.
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
use crate::theme::{FALLBACK_THEME, ThemeSpec, Themes};
use crate::{Error, Value};

/// The `$extensions` key a draft's file is stamped with.
pub const DRAFT_EXTENSION: &str = "org.egui-pop.theme-editor";

/// The version of what a draft writes under [`DRAFT_EXTENSION`]. The
/// file itself is a plain token file; the stamp says which editor wrote
/// it, for migrations to come.
pub const DRAFT_VERSION: u64 = 1;

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

/// A theme being edited.
#[derive(Clone, Debug, PartialEq)]
pub struct ThemeDraft {
    base: String,
    overrides: BTreeMap<String, Json>,
    undo: Vec<TokenEdit>,
    redo: Vec<TokenEdit>,
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
        }
    }

    /// A draft from a spec: its base and its overrides, no history.
    pub fn from_spec(spec: &ThemeSpec) -> ThemeDraft {
        ThemeDraft {
            overrides: spec.overrides.clone(),
            ..ThemeDraft::new(spec.base.clone())
        }
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

    /// `template` with this draft's base and overrides (its density,
    /// accent and reduced motion kept).
    pub fn spec(&self, template: &ThemeSpec) -> ThemeSpec {
        let mut spec = template.clone();
        spec.base = self.base.clone();
        spec.overrides = self.overrides.clone();
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
        self.preview(&edit.path.clone(), edit.to.clone());
        self.undo.push(edit);
        self.redo.clear();
    }

    /// Makes an edit and records it (the same as [`ThemeDraft::record`],
    /// named for edits not previewed first).
    pub fn apply(&mut self, edit: TokenEdit) {
        self.record(edit);
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
        self.preview(&edit.path, edit.from.clone());
        let made = edit.inverse();
        self.redo.push(edit);
        Some(made)
    }

    /// Makes the last undone edit again; returns it.
    pub fn redo(&mut self) -> Option<TokenEdit> {
        let edit = self.redo.pop()?;
        self.preview(&edit.path, edit.to.clone());
        self.undo.push(edit.clone());
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
    /// overrides with their types, stamped with [`DRAFT_EXTENSION`].
    pub fn to_file(&self, themes: &Themes, name: &str) -> TokenFile {
        let base = themes.resolve(&ThemeSpec::new(self.base.clone()));
        let mut file = TokenFile::new(name);
        file.description = Some(format!("Edited from {}", self.base));
        file.extends = Some(self.base.clone());
        let mut stamp = serde_json::Map::new();
        stamp.insert(
            DRAFT_EXTENSION.to_string(),
            json!({ "version": DRAFT_VERSION }),
        );
        file.extensions = Some(Json::Object(stamp));
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
        file
    }

    /// The draft as the text of a `.tokens.json` file.
    pub fn export(&self, themes: &Themes, name: &str) -> String {
        self.to_file(themes, name).to_json_pretty()
    }

    /// Reads a token file as a draft: its `$extends` becomes the base
    /// (the fallback theme when it has none), its tokens the overrides.
    /// Tokens the base theme does not have, or whose value does not
    /// check, are left out and listed. An unknown base is an error; so
    /// is a file stamped by a newer editor.
    pub fn import(
        themes: &Themes,
        name: &str,
        text: &str,
    ) -> Result<(ThemeDraft, Vec<String>), Error> {
        let file = TokenFile::parse(name, text)?;
        if let Some(v) = file
            .extensions
            .as_ref()
            .and_then(|x| x.get(DRAFT_EXTENSION))
            .and_then(|x| x.get("version"))
            .and_then(Json::as_u64)
            && v > DRAFT_VERSION
        {
            return Err(Error::Malformed {
                file: name.to_string(),
                path: format!("$extensions.{DRAFT_EXTENSION}.version"),
                reason: format!("version {v} is newer than this editor's ({DRAFT_VERSION})"),
            });
        }
        let base = file
            .extends
            .clone()
            .unwrap_or_else(|| FALLBACK_THEME.to_string());
        if themes.get(&base).is_none() {
            return Err(Error::UnknownTheme { name: base });
        }
        let mut draft = ThemeDraft::new(base);
        let mut skipped = Vec::new();
        for (path, token) in file.tokens {
            match check(themes, &draft.base, &path, &token.value) {
                Ok(()) => {
                    draft.overrides.insert(path, token.value);
                }
                Err(_) => skipped.push(path),
            }
        }
        Ok((draft, skipped))
    }
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
            ("button.border", json!("{border.strong}")),
        ] {
            let e = d.edit(&t, path, Some(v)).unwrap().unwrap();
            d.apply(e);
        }
        let text = d.export(&t, "mine");
        let file = TokenFile::parse("mine", &text).unwrap();
        assert_eq!(file.extends.as_deref(), Some("pop-light"));
        assert_eq!(file.tokens["accent.default"].ty.as_deref(), Some("color"));
        let (back, skipped) = ThemeDraft::import(&t, "mine", &text).unwrap();
        assert!(skipped.is_empty());
        assert_eq!(back.base(), "pop-light");
        assert_eq!(back.overrides(), d.overrides());
        // The file resolves as a theme like any other.
        let mut with = t.clone();
        with.add_json("mine", &text).unwrap();
        let r = with.resolve(&ThemeSpec::new("mine"));
        assert!(r.problems().is_empty(), "{:?}", r.problems());
        assert_eq!(r.color("accent.default"), crate::Color::from_hex("#2060c0"));
    }

    #[test]
    fn imports_leave_out_what_does_not_fit_and_refuse_unknown_bases() {
        let t = themes();
        let (d, skipped) = ThemeDraft::import(
            &t,
            "x",
            r##"{ "surface": { "$type": "color", "panel": { "$value": "#000000" }, "nope": { "$value": "#000000" } } }"##,
        )
        .unwrap();
        assert_eq!(d.base(), FALLBACK_THEME);
        assert_eq!(skipped, vec!["surface.nope".to_string()]);
        assert!(matches!(
            ThemeDraft::import(&t, "x", r#"{ "$extends": "nowhere" }"#),
            Err(Error::UnknownTheme { .. })
        ));
        let newer = format!(
            r#"{{ "$extends": "pop-dark", "$extensions": {{ "{DRAFT_EXTENSION}": {{ "version": 99 }} }} }}"#
        );
        assert!(ThemeDraft::import(&t, "x", &newer).is_err());
    }
}
