//! Themes: base themes, density overlays, accents and overrides composed
//! into one set of resolved tokens.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use serde_json::{Map, Value as Json, json};

use crate::contract::{Tier, contract, required};
use crate::file::{RawToken, TokenFile, alias_target};
use crate::{Color, Error, Value};

/// The theme everything falls back to.
pub const FALLBACK_THEME: &str = "pop-dark";

/// egui-pop's own base themes, in the order a switcher shows them.
pub const POP_THEMES: &[&str] = &["pop-dark", "pop-light", "pop-high-contrast"];

/// Every built-in theme, in the order a switcher shows them: egui-pop's
/// own, then the editor-inspired ones by family, light before dark (their
/// sources and licences are in `NOTICE`).
pub const BUILT_IN_THEMES: &[&str] = &[
    "pop-dark",
    "pop-light",
    "pop-high-contrast",
    "github-light",
    "github-dark",
    "github-light-high-contrast",
    "github-dark-high-contrast",
    "catppuccin-latte",
    "catppuccin-mocha",
    "one-light",
    "one-dark-pro",
    "solarized-light",
    "solarized-dark",
    "gruvbox-light",
    "gruvbox-dark",
];

/// The built-in themes' names for people (ids stay as they are): the
/// editor-inspired ones say so, their names only describing where the
/// colours come from.
const BUILT_IN_LABELS: &[(&str, &str)] = &[
    ("pop-dark", "Pop dark"),
    ("pop-light", "Pop light"),
    ("pop-high-contrast", "Pop high contrast"),
    ("github-light", "GitHub-inspired Light"),
    ("github-dark", "GitHub-inspired Dark"),
    (
        "github-light-high-contrast",
        "GitHub-inspired Light High Contrast",
    ),
    (
        "github-dark-high-contrast",
        "GitHub-inspired Dark High Contrast",
    ),
    ("catppuccin-latte", "Catppuccin-inspired Latte"),
    ("catppuccin-mocha", "Catppuccin-inspired Mocha"),
    ("one-light", "One Light-inspired"),
    ("one-dark-pro", "One Dark Pro-inspired"),
    ("solarized-light", "Solarized-inspired Light"),
    ("solarized-dark", "Solarized-inspired Dark"),
    ("gruvbox-light", "Gruvbox-inspired Light"),
    ("gruvbox-dark", "Gruvbox-inspired Dark"),
];

/// A theme's name for people: a built-in theme's label, else its id.
pub fn theme_label(name: &str) -> String {
    BUILT_IN_LABELS
        .iter()
        .find(|(id, _)| *id == name)
        .map_or_else(|| name.to_string(), |(_, label)| (*label).to_string())
}

/// The built-in colour scales usable as an accent.
pub const BUILT_IN_ACCENTS: &[&str] = &["gold", "blue", "green", "amber", "red"];

/// The default kit's components (`docs/kit.md`), one file each, layered
/// into `pop-dark` (and so into every theme extending it). One line per
/// component, in alphabetical order.
const KIT_FILES: &[(&str, &str)] = &[
    (
        "assist-card",
        include_str!("../themes/kit/assist-card.tokens.json"),
    ),
    (
        "backdrop",
        include_str!("../themes/kit/backdrop.tokens.json"),
    ),
    ("dialog", include_str!("../themes/kit/dialog.tokens.json")),
    (
        "editor-frame",
        include_str!("../themes/kit/editor-frame.tokens.json"),
    ),
    (
        "fault-card",
        include_str!("../themes/kit/fault-card.tokens.json"),
    ),
    (
        "find-bar",
        include_str!("../themes/kit/find-bar.tokens.json"),
    ),
    (
        "fullscreen-exit",
        include_str!("../themes/kit/fullscreen-exit.tokens.json"),
    ),
    ("job-log", include_str!("../themes/kit/job-log.tokens.json")),
    (
        "palette-window",
        include_str!("../themes/kit/palette-window.tokens.json"),
    ),
    ("popover", include_str!("../themes/kit/popover.tokens.json")),
    ("tab", include_str!("../themes/kit/tab.tokens.json")),
    ("tab-bar", include_str!("../themes/kit/tab-bar.tokens.json")),
    (
        "terminal-frame",
        include_str!("../themes/kit/terminal-frame.tokens.json"),
    ),
    (
        "tip-card",
        include_str!("../themes/kit/tip-card.tokens.json"),
    ),
    (
        "title-bar",
        include_str!("../themes/kit/title-bar.tokens.json"),
    ),
    ("toast", include_str!("../themes/kit/toast.tokens.json")),
    ("window", include_str!("../themes/kit/window.tokens.json")),
    (
        "window-button",
        include_str!("../themes/kit/window-button.tokens.json"),
    ),
    (
        "window-close",
        include_str!("../themes/kit/window-close.tokens.json"),
    ),
];

const BUILT_IN_FILES: &[(&str, &str)] = &[
    ("pop-dark", include_str!("../themes/pop-dark.tokens.json")),
    ("pop-light", include_str!("../themes/pop-light.tokens.json")),
    (
        "pop-high-contrast",
        include_str!("../themes/pop-high-contrast.tokens.json"),
    ),
    (
        "github-light",
        include_str!("../themes/github-light.tokens.json"),
    ),
    (
        "github-dark",
        include_str!("../themes/github-dark.tokens.json"),
    ),
    (
        "github-light-high-contrast",
        include_str!("../themes/github-light-high-contrast.tokens.json"),
    ),
    (
        "github-dark-high-contrast",
        include_str!("../themes/github-dark-high-contrast.tokens.json"),
    ),
    (
        "catppuccin-latte",
        include_str!("../themes/catppuccin-latte.tokens.json"),
    ),
    (
        "catppuccin-mocha",
        include_str!("../themes/catppuccin-mocha.tokens.json"),
    ),
    ("one-light", include_str!("../themes/one-light.tokens.json")),
    (
        "one-dark-pro",
        include_str!("../themes/one-dark-pro.tokens.json"),
    ),
    (
        "solarized-light",
        include_str!("../themes/solarized-light.tokens.json"),
    ),
    (
        "solarized-dark",
        include_str!("../themes/solarized-dark.tokens.json"),
    ),
    (
        "gruvbox-light",
        include_str!("../themes/gruvbox-light.tokens.json"),
    ),
    (
        "gruvbox-dark",
        include_str!("../themes/gruvbox-dark.tokens.json"),
    ),
];

/// The scale steps an accent re-aliases from, and what they become.
const ACCENT_ROLES: &[(&str, &str)] = &[
    ("accent.default", "9"),
    ("accent.hover", "10"),
    ("accent.active", "8"),
    ("accent.subtle", "3"),
    ("text.on-accent", "contrast"),
    ("selection", "5"),
];

/// How much room the interface takes: an overlay of space and size tokens
/// only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Density {
    /// The values the base themes carry.
    #[default]
    Comfortable,
    /// Tighter spacing and smaller controls.
    Compact,
}

impl Density {
    pub const ALL: [Density; 2] = [Density::Comfortable, Density::Compact];

    pub fn name(self) -> &'static str {
        match self {
            Density::Comfortable => "comfortable",
            Density::Compact => "compact",
        }
    }

    /// The built-in overlay file of this density.
    pub fn built_in_file(self) -> TokenFile {
        let text = match self {
            Density::Comfortable => include_str!("../themes/density-comfortable.tokens.json"),
            Density::Compact => include_str!("../themes/density-compact.tokens.json"),
        };
        TokenFile::parse(&format!("density-{}", self.name()), text)
            .expect("built-in density files parse")
    }
}

impl fmt::Display for Density {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for Density {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Density::ALL
            .into_iter()
            .find(|d| d.name() == s)
            .ok_or_else(|| format!("`{s}` is not a density (comfortable, compact)"))
    }
}

/// What to resolve: a base theme (mode and contrast), a density, an accent,
/// reduced motion and the user's own overrides, composed in that order.
///
/// ```
/// use egui_pop_tokens::{Density, ThemeSpec, Themes};
///
/// let spec = ThemeSpec::new("pop-light").density(Density::Compact).accent("blue");
/// let tokens = Themes::default().resolve(&spec);
/// assert!(tokens.problems().is_empty());
/// assert_eq!(tokens.dimension("space.m"), Some(4.0));
/// ```
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ThemeSpec {
    /// The base theme's name (`pop-dark`, `pop-light`, ...).
    pub base: String,
    pub density: Density,
    /// A colour scale whose steps become the accent roles; `None` keeps
    /// the base theme's accent.
    pub accent: Option<String>,
    /// Sets `motion.duration` to zero.
    pub reduced_motion: bool,
    /// Raw DTCG values by token path, applied last; values may be aliases.
    pub overrides: BTreeMap<String, Json>,
}

impl Default for ThemeSpec {
    fn default() -> Self {
        ThemeSpec::new(FALLBACK_THEME)
    }
}

impl ThemeSpec {
    pub fn new(base: impl Into<String>) -> ThemeSpec {
        ThemeSpec {
            base: base.into(),
            density: Density::default(),
            accent: None,
            reduced_motion: false,
            overrides: BTreeMap::new(),
        }
    }

    pub fn density(mut self, density: Density) -> Self {
        self.density = density;
        self
    }

    pub fn accent(mut self, scale: impl Into<String>) -> Self {
        self.accent = Some(scale.into());
        self
    }

    pub fn reduced_motion(mut self, reduced: bool) -> Self {
        self.reduced_motion = reduced;
        self
    }

    /// Overrides one token with a raw DTCG value (`json!("#3080ff")`,
    /// `json!("{blue.9}")`).
    pub fn override_token(mut self, path: impl Into<String>, value: Json) -> Self {
        self.overrides.insert(path.into(), value);
        self
    }

    /// A short label: `pop-dark · compact · blue`.
    pub fn label(&self) -> String {
        let mut parts = vec![self.base.clone(), self.density.name().to_string()];
        if let Some(accent) = &self.accent {
            parts.push(accent.clone());
        }
        if self.reduced_motion {
            parts.push("reduced motion".into());
        }
        if !self.overrides.is_empty() {
            parts.push(format!("{} overrides", self.overrides.len()));
        }
        parts.join(" · ")
    }
}

/// A resolved token.
#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub path: String,
    /// The DTCG type.
    pub ty: String,
    pub value: Value,
    /// The path this token directly aliases, if it is an alias.
    pub alias: Option<String>,
    pub description: Option<String>,
    /// `$extensions`, kept as written.
    pub extensions: Option<Json>,
    /// Whether the value came from the fallback theme after a problem.
    pub fallback: bool,
}

impl Token {
    /// The token's tier: from the contract, else the palette.
    pub fn tier(&self) -> Tier {
        required(&self.path).map_or(Tier::Reference, |r| r.tier)
    }
}

/// A theme ready to use: every contract token has a value of its type.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedTokens {
    spec: ThemeSpec,
    tokens: BTreeMap<String, Token>,
    problems: Vec<Error>,
}

impl ResolvedTokens {
    pub fn spec(&self) -> &ThemeSpec {
        &self.spec
    }

    /// What was wrong, in the order met; empty for a clean theme.
    pub fn problems(&self) -> &[Error] {
        &self.problems
    }

    pub fn get(&self, path: &str) -> Option<&Token> {
        self.tokens.get(path)
    }

    pub fn value(&self, path: &str) -> Option<&Value> {
        self.tokens.get(path).map(|t| &t.value)
    }

    pub fn color(&self, path: &str) -> Option<Color> {
        match self.value(path)? {
            Value::Color(c) => Some(*c),
            _ => None,
        }
    }

    /// A dimension (or a number) in logical pixels.
    pub fn dimension(&self, path: &str) -> Option<f32> {
        match self.value(path)? {
            Value::Dimension(d) | Value::Number(d) => Some(*d),
            _ => None,
        }
    }

    /// A duration in milliseconds.
    pub fn duration_ms(&self, path: &str) -> Option<f32> {
        match self.value(path)? {
            Value::Duration(ms) => Some(*ms),
            _ => None,
        }
    }

    pub fn font_family(&self, path: &str) -> Option<&[String]> {
        match self.value(path)? {
            Value::FontFamily(f) => Some(f),
            _ => None,
        }
    }

    /// Every token, by path.
    pub fn iter(&self) -> impl Iterator<Item = &Token> {
        self.tokens.values()
    }

    pub fn len(&self) -> usize {
        self.tokens.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    /// The theme as one DTCG file: every token with its resolved value,
    /// nothing extended, no alias left.
    pub fn to_dtcg(&self) -> Json {
        let mut root = Map::new();
        root.insert("$description".into(), json!(self.spec.label()));
        for token in self.tokens.values() {
            let mut node = &mut root;
            let parts: Vec<&str> = token.path.split('.').collect();
            let (leaf, groups) = parts.split_last().expect("paths are not empty");
            for part in groups {
                node = node
                    .entry(part.to_string())
                    .or_insert_with(|| json!({}))
                    .as_object_mut()
                    .expect("groups are objects");
            }
            let mut entry = Map::new();
            entry.insert("$type".into(), json!(token.ty));
            entry.insert("$value".into(), token.value.to_json());
            if let Some(d) = &token.description {
                entry.insert("$description".into(), json!(d));
            }
            if let Some(e) = &token.extensions {
                entry.insert("$extensions".into(), e.clone());
            }
            node.insert(leaf.to_string(), Json::Object(entry));
        }
        Json::Object(root)
    }
}

/// The themes known: the built-in ones and any added.
#[derive(Clone, Debug)]
pub struct Themes {
    themes: BTreeMap<String, TokenFile>,
    densities: BTreeMap<Density, TokenFile>,
}

impl Default for Themes {
    /// The built-in themes and densities.
    fn default() -> Self {
        let mut themes = Themes {
            themes: BTreeMap::new(),
            densities: Density::ALL
                .into_iter()
                .map(|d| (d, d.built_in_file()))
                .collect(),
        };
        for (name, text) in BUILT_IN_FILES {
            let mut file = TokenFile::parse(name, text).expect("built-in themes parse");
            if *name == FALLBACK_THEME {
                for (component, text) in KIT_FILES {
                    let part = TokenFile::parse(&format!("kit/{component}"), text)
                        .expect("the default kit's files parse");
                    for (path, token) in part.tokens {
                        let clash = file.tokens.insert(path.clone(), token);
                        assert!(clash.is_none(), "kit/{component} repeats {path}");
                    }
                }
            }
            themes.add(file);
        }
        themes
    }
}

impl Themes {
    /// Adds a theme, or replaces the one of the same name (returned).
    pub fn add(&mut self, theme: TokenFile) -> Option<TokenFile> {
        self.themes.insert(theme.name.clone(), theme)
    }

    /// Reads a theme file and adds it under `name`.
    pub fn add_json(&mut self, name: &str, text: &str) -> Result<(), Error> {
        self.add(TokenFile::parse(name, text)?);
        Ok(())
    }

    /// Replaces a density's overlay.
    pub fn set_density(&mut self, density: Density, overlay: TokenFile) {
        self.densities.insert(density, overlay);
    }

    pub fn get(&self, name: &str) -> Option<&TokenFile> {
        self.themes.get(name)
    }

    /// The names of the themes known, sorted.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.themes.keys().map(String::as_str)
    }

    /// The theme's tokens with everything it extends, the theme's own
    /// winning. An unknown theme falls back to [`FALLBACK_THEME`].
    fn flatten(&self, name: &str, problems: &mut Vec<Error>) -> BTreeMap<String, RawToken> {
        let mut chain: Vec<String> = Vec::new();
        let mut at = Some(name.to_string());
        while let Some(n) = at {
            if chain.contains(&n) {
                chain.push(n);
                problems.push(Error::ExtendsCycle {
                    chain: chain.clone(),
                });
                chain.pop();
                break;
            }
            let Some(theme) = self.themes.get(&n) else {
                problems.push(Error::UnknownTheme { name: n });
                if !chain.iter().any(|c| c == FALLBACK_THEME) {
                    chain.push(FALLBACK_THEME.to_string());
                }
                break;
            };
            chain.push(n);
            at = theme.extends.clone();
        }
        let mut tokens = BTreeMap::new();
        for n in chain.iter().rev() {
            if let Some(theme) = self.themes.get(n) {
                layer(&mut tokens, &renamed(&theme.tokens, problems));
            }
        }
        tokens
    }

    /// The colour scales of a base theme usable as an accent.
    pub fn accents(&self, base: &str) -> Vec<String> {
        let raw = self.flatten(base, &mut Vec::new());
        let mut scales: Vec<String> = raw
            .keys()
            .filter_map(|p| p.strip_suffix(".contrast"))
            .filter(|s| is_accent_scale(&raw, s))
            .map(String::from)
            .collect();
        scales.sort_by_key(|s| {
            BUILT_IN_ACCENTS
                .iter()
                .position(|b| b == s)
                .unwrap_or(usize::MAX)
        });
        scales
    }

    /// Resolves a spec. Never fails: problems are reported in the result,
    /// and every contract token that has one takes the built-in
    /// `pop-dark` value.
    pub fn resolve(&self, spec: &ThemeSpec) -> ResolvedTokens {
        let mut resolved = self.resolve_without_fallback(spec);
        let missing: Vec<&str> = contract()
            .filter(|r| resolved.tokens.get(r.path).is_none_or(|t| t.ty != r.ty))
            .map(|r| r.path)
            .collect();
        if missing.is_empty() {
            return resolved;
        }
        let mut fallback_spec = spec.clone();
        fallback_spec.base = FALLBACK_THEME.to_string();
        fallback_spec.overrides.clear();
        let fallback = Themes::default().resolve_without_fallback(&fallback_spec);
        for path in missing {
            if let Some(t) = fallback.tokens.get(path) {
                let mut t = t.clone();
                t.fallback = true;
                resolved.tokens.insert(path.to_string(), t);
            }
        }
        resolved
    }

    /// Resolves a spec without filling the contract's gaps.
    fn resolve_without_fallback(&self, spec: &ThemeSpec) -> ResolvedTokens {
        let mut problems = Vec::new();
        let mut raw = self.flatten(&spec.base, &mut problems);
        if let Some(density) = self.densities.get(&spec.density) {
            layer(&mut raw, &density.tokens);
        }
        if let Some(scale) = &spec.accent {
            if is_accent_scale(&raw, scale) {
                for (role, step) in ACCENT_ROLES {
                    let entry = raw.entry(role.to_string()).or_insert_with(|| RawToken {
                        ty: Some("color".into()),
                        value: Json::Null,
                        description: None,
                        extensions: None,
                    });
                    entry.value = json!(format!("{{{scale}.{step}}}"));
                }
            } else {
                problems.push(Error::UnknownAccent {
                    name: scale.clone(),
                });
            }
        }
        if spec.reduced_motion {
            let entry = raw
                .entry("motion.duration".to_string())
                .or_insert_with(|| RawToken {
                    ty: Some("duration".into()),
                    value: Json::Null,
                    description: None,
                    extensions: None,
                });
            entry.value = json!({ "value": 0, "unit": "ms" });
        }
        for (path, value) in &spec.overrides {
            match raw.get_mut(path) {
                Some(token) => token.value = value.clone(),
                None => problems.push(Error::UnknownOverride { path: path.clone() }),
            }
        }

        let mut tokens = BTreeMap::new();
        for (path, token) in &raw {
            match resolve_one(&raw, path) {
                Ok((ty, value)) => {
                    let contract = required(path);
                    if let Some(r) = contract.filter(|r| r.ty != ty) {
                        problems.push(Error::TypeMismatch {
                            path: path.clone(),
                            expected: r.ty.to_string(),
                            found: ty,
                        });
                        continue;
                    }
                    tokens.insert(
                        path.clone(),
                        Token {
                            path: path.clone(),
                            ty,
                            value,
                            alias: token.alias().map(String::from),
                            description: token.description.clone(),
                            extensions: token.extensions.clone(),
                            fallback: false,
                        },
                    );
                }
                // A broken token is reported once, not again for each
                // token aliasing it.
                Err(e) => {
                    if !problems.contains(&e) {
                        problems.push(e);
                    }
                }
            }
        }
        for r in contract() {
            if !raw.contains_key(r.path) {
                problems.push(Error::Missing {
                    path: r.path.to_string(),
                });
            }
        }
        ResolvedTokens {
            spec: spec.clone(),
            tokens,
            problems,
        }
    }
}

/// Lays `over` on `under`: values replace, a token without a type keeps the
/// type it had.
/// A theme's tokens with the renamed ones under their new names
/// ([`crate::contract::RENAMED`]); a new name the theme gives itself wins.
fn renamed<'a>(
    tokens: &'a BTreeMap<String, RawToken>,
    problems: &mut Vec<Error>,
) -> std::borrow::Cow<'a, BTreeMap<String, RawToken>> {
    let olds: Vec<&(&str, &[&str])> = crate::contract::RENAMED
        .iter()
        .filter(|(old, _)| tokens.contains_key(*old))
        .collect();
    if olds.is_empty() {
        return std::borrow::Cow::Borrowed(tokens);
    }
    let mut out = tokens.clone();
    for (old, new) in olds {
        let token = out.remove(*old).expect("filtered on presence");
        for n in *new {
            out.entry((*n).to_string()).or_insert_with(|| token.clone());
        }
        problems.push(Error::Renamed {
            old: (*old).to_string(),
            new: new.iter().map(|n| (*n).to_string()).collect(),
        });
    }
    std::borrow::Cow::Owned(out)
}

fn layer(under: &mut BTreeMap<String, RawToken>, over: &BTreeMap<String, RawToken>) {
    for (path, token) in over {
        let mut token = token.clone();
        if token.ty.is_none() {
            token.ty = under.get(path).and_then(|t| t.ty.clone());
        }
        if token.description.is_none() {
            token.description = under.get(path).and_then(|t| t.description.clone());
        }
        under.insert(path.clone(), token);
    }
}

fn is_accent_scale(raw: &BTreeMap<String, RawToken>, scale: &str) -> bool {
    ACCENT_ROLES
        .iter()
        .all(|(_, step)| raw.contains_key(&format!("{scale}.{step}")))
}

/// A token's type and value, aliases followed.
fn resolve_one(raw: &BTreeMap<String, RawToken>, path: &str) -> Result<(String, Value), Error> {
    let mut chain = vec![path.to_string()];
    let mut declared = raw.get(path).and_then(|t| t.ty.clone());
    let mut at = raw.get(path).ok_or_else(|| Error::Missing {
        path: path.to_string(),
    })?;
    while let Some(target) = alias_target(&at.value) {
        if chain.iter().any(|c| c == target) {
            chain.push(target.to_string());
            return Err(Error::AliasCycle { chain });
        }
        let next = raw.get(target).ok_or_else(|| Error::UnknownAlias {
            path: chain.last().cloned().unwrap_or_default(),
            target: target.to_string(),
        })?;
        if let (Some(d), Some(t)) = (&declared, &next.ty)
            && d != t
        {
            return Err(Error::TypeMismatch {
                path: path.to_string(),
                expected: d.clone(),
                found: format!("{t} (through `{{{target}}}`)"),
            });
        }
        declared = declared.or_else(|| next.ty.clone());
        chain.push(target.to_string());
        at = next;
    }
    let ty = declared.ok_or_else(|| Error::MissingType {
        path: path.to_string(),
    })?;
    let value = Value::parse(&ty, &at.value).map_err(|reason| Error::InvalidValue {
        path: path.to_string(),
        ty: ty.clone(),
        reason,
    })?;
    Ok((ty, value))
}
