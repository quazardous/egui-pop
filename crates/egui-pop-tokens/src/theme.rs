//! Themes: base themes, density overlays, accents and overrides composed
//! into one set of resolved tokens.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;
use std::str::FromStr;

use serde_json::{Map, Value as Json, json};

use crate::cache::Cache;
use crate::contract::{Tier, contract, required};
use crate::file::{RawToken, TokenFile, alias_target};
use crate::palette::{Backdrop, Ramp, Scheme, Seed};
use crate::{Color, Error, Value, Warning};

/// The theme everything falls back to.
pub const FALLBACK_THEME: &str = "pop-dark";

/// The `$extensions` key of the alpha modifier: on a colour token, a
/// number from 0 to 1 the colour's alpha is multiplied by, after its
/// alias is followed (`{"$value": "{surface.overlay}", "$extensions":
/// {"org.egui-pop.alpha": 0.85}}`, the overlay at 85 %). Every modifier
/// met along a chain of aliases applies.
pub const ALPHA_EXTENSION: &str = "org.egui-pop.alpha";

/// The root `$extensions` key naming a theme's counterpart: the other
/// variant, light or dark, of the same theme (`$extensions:
/// {"org.egui-pop.variant-of": "pop-dark"}` in `pop-light`). Saying it
/// on one side is enough: the other is found by looking it up. Not
/// inherited through `$extends`: a theme built on `pop-light` is a
/// theme of its own.
pub const VARIANT_OF_EXTENSION: &str = "org.egui-pop.variant-of";

/// The root `$extensions` key carrying a theme's counterpart inside its
/// file: a light and dark pair saved as one file (the theme editor's).
/// Its value names the counterpart and gives its token file, an
/// ordinary one with its own stamp:
///
/// ```json
/// { "$extends": "pop-dark",
///   "$extensions": {
///     "org.egui-pop.variant-of": "ocean-light",
///     "org.egui-pop.counterpart": {
///       "name": "ocean-light",
///       "file": { "$extends": "pop-light",
///                 "$extensions": { "org.egui-pop.variant-of": "ocean" } } } } }
/// ```
///
/// [`Themes::add_text`] and [`Themes::add_path`] add the counterpart
/// too, under its name; a reader that does not know the key loads the
/// file's own variant alone.
pub const COUNTERPART_EXTENSION: &str = "org.egui-pop.counterpart";

impl TokenFile {
    /// Sets (or, with `None`, removes) one entry of the root
    /// `$extensions`, keeping the others.
    pub fn set_extension(&mut self, key: &str, value: Option<Json>) {
        let mut root = match self.extensions.take() {
            Some(Json::Object(m)) => m,
            _ => Map::new(),
        };
        match value {
            Some(v) => {
                root.insert(key.to_string(), v);
            }
            None => {
                root.remove(key);
            }
        }
        self.extensions = (!root.is_empty()).then_some(Json::Object(root));
    }

    /// The counterpart this file carries ([`COUNTERPART_EXTENSION`]),
    /// under its name; `None` when it carries none, an error when the
    /// entry cannot be read.
    pub fn counterpart_file(&self) -> Option<Result<TokenFile, Error>> {
        let raw = self.extensions.as_ref()?.get(COUNTERPART_EXTENSION)?;
        let malformed = |reason: &str| Error::Malformed {
            file: self.name.clone(),
            path: format!("$extensions.{COUNTERPART_EXTENSION}"),
            reason: reason.to_string(),
        };
        let Some(name) = raw
            .get("name")
            .and_then(Json::as_str)
            .filter(|n| !n.is_empty())
        else {
            return Some(Err(malformed("no `name` naming the counterpart")));
        };
        if name == self.name {
            return Some(Err(malformed("the counterpart has the file's own name")));
        }
        let Some(file) = raw.get("file").filter(|f| f.is_object()) else {
            return Some(Err(malformed("no `file` giving the counterpart's tokens")));
        };
        Some(TokenFile::from_tree(name, file))
    }

    /// Makes this file carry `counterpart` ([`COUNTERPART_EXTENSION`]),
    /// each naming the other ([`VARIANT_OF_EXTENSION`]); `None` removes
    /// it.
    pub fn set_counterpart_file(&mut self, counterpart: Option<TokenFile>) {
        match counterpart {
            Some(mut other) => {
                other.set_extension(VARIANT_OF_EXTENSION, Some(json!(self.name)));
                self.set_extension(VARIANT_OF_EXTENSION, Some(json!(other.name)));
                self.set_extension(
                    COUNTERPART_EXTENSION,
                    Some(json!({ "name": other.name, "file": other.to_json() })),
                );
            }
            None => self.set_extension(COUNTERPART_EXTENSION, None),
        }
    }
}

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
/// into `pop-dark` (and so into every theme extending it): one entry per
/// `themes/kit/<component>.tokens.yaml`, compiled to JSON by `build.rs`.
pub(crate) const KIT_FILES: &[(&str, &str)] = include!(concat!(env!("OUT_DIR"), "/kit_files.rs"));

/// A shipped token file, `themes/<name>.tokens.yaml`, as `build.rs`
/// compiled it to JSON.
macro_rules! shipped {
    ($name:literal) => {
        include_str!(concat!(env!("OUT_DIR"), "/themes/", $name, ".tokens.json"))
    };
}

const BUILT_IN_FILES: &[(&str, &str)] = &[
    ("pop-dark", shipped!("pop-dark")),
    ("pop-light", shipped!("pop-light")),
    ("pop-high-contrast", shipped!("pop-high-contrast")),
    ("github-light", shipped!("github-light")),
    ("github-dark", shipped!("github-dark")),
    (
        "github-light-high-contrast",
        shipped!("github-light-high-contrast"),
    ),
    (
        "github-dark-high-contrast",
        shipped!("github-dark-high-contrast"),
    ),
    ("catppuccin-latte", shipped!("catppuccin-latte")),
    ("catppuccin-mocha", shipped!("catppuccin-mocha")),
    ("one-light", shipped!("one-light")),
    ("one-dark-pro", shipped!("one-dark-pro")),
    ("solarized-light", shipped!("solarized-light")),
    ("solarized-dark", shipped!("solarized-dark")),
    ("gruvbox-light", shipped!("gruvbox-light")),
    ("gruvbox-dark", shipped!("gruvbox-dark")),
];

/// The scale an accent generated from a seed colour is written to
/// ([`ThemeSpec::accent_color`]): `custom.1` to `custom.12` and
/// `custom.contrast`, replacing a scale of that name.
pub const SEED_SCALE: &str = "custom";

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
            Density::Comfortable => shipped!("density-comfortable"),
            Density::Compact => shipped!("density-compact"),
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

/// Which theme, if any, is the other variant (light or dark) of a spec's
/// base theme: installed with it, so that egui's light or dark mode
/// (the system's, or the user's choice) switches between them without
/// installing anything again.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Counterpart {
    /// The one the token files name ([`VARIANT_OF_EXTENSION`]); none
    /// when they name none.
    #[default]
    Auto,
    /// This theme, whatever the files say.
    Theme(String),
    /// None: the base theme alone, in its own mode whatever is asked.
    None,
}

/// What to resolve: a base theme (mode and contrast), a density, an accent,
/// reduced motion and the user's own overrides, composed in that order.
/// The base theme's counterpart ([`Counterpart`]), when it has one, is
/// resolved with the same density, accent, reduced motion and overrides
/// ([`Themes::resolve_counterpart`]), or with overrides of its own
/// ([`ThemeSpec::counterpart_overrides`]).
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
    /// The other variant of the base theme, light or dark.
    pub counterpart: Counterpart,
    /// The counterpart's own overrides, in place of `overrides`: a pair
    /// whose two variants are edited apart (the theme editor's). `None`:
    /// `overrides` apply to both.
    pub counterpart_overrides: Option<BTreeMap<String, Json>>,
    /// The user's text size: a factor of the size scale (`font.size.*`),
    /// one of [`TEXT_SIZES`]; 1 keeps the theme's sizes
    /// ([`ThemeSpec::text_size`]).
    pub text_size: f32,
    /// A family the user puts first in `font.family.ui`, before the
    /// theme's own (which stay its fallbacks).
    pub font_ui: Option<String>,
    /// A family the user puts first in `font.family.mono`.
    pub font_mono: Option<String>,
}

/// The text sizes a user chooses from, as factors of a theme's size
/// scale: few steps, so the font atlas holds few sizes.
pub const TEXT_SIZES: [f32; 7] = [0.85, 1.0, 1.15, 1.3, 1.5, 1.75, 2.0];

/// The step of [`TEXT_SIZES`] nearest to a factor.
pub fn text_size_step(factor: f32) -> f32 {
    TEXT_SIZES
        .iter()
        .copied()
        .min_by(|a, b| (a - factor).abs().total_cmp(&(b - factor).abs()))
        .unwrap_or(1.0)
}

/// The tokens of the size scale a text size multiplies.
const SIZE_SCALE: [&str; 5] = [
    "font.size.xs",
    "font.size.s",
    "font.size.m",
    "font.size.l",
    "font.size.xl",
];

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
            counterpart: Counterpart::Auto,
            counterpart_overrides: None,
            text_size: 1.0,
            font_ui: None,
            font_mono: None,
        }
    }

    /// A pair of themes, a dark one and a light one: `dark` is the base
    /// (shown when nothing asks for light), `light` its counterpart.
    ///
    /// ```
    /// use egui_pop_tokens::{ThemeSpec, Themes};
    ///
    /// let spec = ThemeSpec::pair("github-dark", "pop-light");
    /// let light = Themes::default().resolve_counterpart(&spec).unwrap();
    /// assert_eq!(light.spec().base, "pop-light");
    /// assert!(!light.is_dark());
    /// ```
    pub fn pair(dark: impl Into<String>, light: impl Into<String>) -> ThemeSpec {
        ThemeSpec::new(dark).counterpart(light)
    }

    /// Names the base theme's counterpart, whatever the token files say.
    pub fn counterpart(mut self, theme: impl Into<String>) -> Self {
        self.counterpart = Counterpart::Theme(theme.into());
        self
    }

    /// The base theme alone, without its counterpart: its mode is forced.
    pub fn alone(mut self) -> Self {
        self.counterpart = Counterpart::None;
        self
    }

    /// The same spec over another base theme, alone, with the same
    /// overrides.
    pub fn variant(&self, base: impl Into<String>) -> ThemeSpec {
        let mut spec = self.clone();
        spec.base = base.into();
        spec.counterpart = Counterpart::None;
        spec.counterpart_overrides = None;
        spec
    }

    /// What the counterpart `other` is resolved from: the same spec over
    /// it, alone, with the counterpart's own overrides when the spec
    /// gives them ([`ThemeSpec::counterpart_overrides`]).
    ///
    /// ```
    /// use egui_pop_tokens::{Color, ThemeSpec, Themes};
    /// use serde_json::json;
    ///
    /// let spec = ThemeSpec::new("pop-dark")
    ///     .override_token("danger", json!("#ff0000"))
    ///     .counterpart_override("danger", json!("#00ff00"));
    /// let light = Themes::default().resolve_counterpart(&spec).unwrap();
    /// assert_eq!(light.color("danger"), Color::from_hex("#00ff00"));
    /// let dark = Themes::default().resolve(&spec);
    /// assert_eq!(dark.color("danger"), Color::from_hex("#ff0000"));
    /// ```
    pub fn counterpart_variant(&self, other: impl Into<String>) -> ThemeSpec {
        let mut spec = self.variant(other);
        if let Some(own) = &self.counterpart_overrides {
            spec.overrides = own.clone();
        }
        spec
    }

    pub fn density(mut self, density: Density) -> Self {
        self.density = density;
        self
    }

    /// The accent: a colour scale of the theme by name (`"blue"`), or a
    /// seed colour (`"#3a7bd5"`) the scale is generated from
    /// ([`ThemeSpec::accent_color`]).
    pub fn accent(mut self, scale: impl Into<String>) -> Self {
        self.accent = Some(scale.into());
        self
    }

    /// An accent generated from a seed colour: the [`SEED_SCALE`] scale
    /// ([`palette::Ramp`](crate::palette::Ramp)), for the theme's light
    /// or dark scheme, the seed clamped to what gives a usable ramp. A
    /// counterpart gets the ramp of its own scheme from the same seed.
    ///
    /// ```
    /// use egui_pop_tokens::{Color, ThemeSpec, Themes, contrast};
    ///
    /// let spec = ThemeSpec::new("pop-light").accent_color(Color::from_hex("#c0392b").unwrap());
    /// assert_eq!(spec.accent.as_deref(), Some("#c0392b"));
    /// let tokens = Themes::default().resolve(&spec);
    /// assert_eq!(tokens.get("accent.default").unwrap().alias.as_deref(), Some("custom.9"));
    /// assert!(contrast::check(&tokens, contrast::Level::Normal).is_empty());
    /// ```
    pub fn accent_color(mut self, seed: Color) -> Self {
        self.accent = Some(Color { a: 1.0, ..seed }.to_hex());
        self
    }

    /// The accent's seed colour, when the accent is one (`#rrggbb`).
    pub fn accent_seed(&self) -> Option<Color> {
        self.accent
            .as_deref()
            .filter(|a| a.starts_with('#'))
            .and_then(Color::from_hex)
    }

    pub fn reduced_motion(mut self, reduced: bool) -> Self {
        self.reduced_motion = reduced;
        self
    }

    /// The user's text size, a factor of the theme's size scale, rounded
    /// to the nearest of [`TEXT_SIZES`]: each step of `font.size.*` is
    /// multiplied and rounded to half a point, and every text style and
    /// component token aliasing a step follows. Only the text grows:
    /// spacing does not (the interface zoom scales everything), and
    /// `size.control.height` grows to at least 1.6 times the body text,
    /// so a larger label is not clipped.
    ///
    /// ```
    /// use egui_pop_tokens::{ThemeSpec, Themes};
    ///
    /// let themes = Themes::default();
    /// let base = themes.resolve(&ThemeSpec::new("pop-dark"));
    /// let large = themes.resolve(&ThemeSpec::new("pop-dark").text_size(1.3));
    /// assert_eq!(base.dimension("font.size.m"), Some(13.0));
    /// assert_eq!(large.dimension("font.size.m"), Some(17.0));
    /// assert_eq!(large.dimension("font.text-style.body"), Some(17.0));
    /// assert_eq!(large.dimension("space.m"), base.dimension("space.m"));
    /// ```
    pub fn text_size(mut self, factor: f32) -> Self {
        self.text_size = text_size_step(factor);
        self
    }

    /// A family drawn first for interface text (`font.family.ui`), the
    /// theme's own families after it; `None` keeps the theme's.
    pub fn font_ui(mut self, family: Option<String>) -> Self {
        self.font_ui = family.filter(|f| !f.trim().is_empty());
        self
    }

    /// A family drawn first for numbers and code (`font.family.mono`).
    pub fn font_mono(mut self, family: Option<String>) -> Self {
        self.font_mono = family.filter(|f| !f.trim().is_empty());
        self
    }

    /// Overrides one token with a raw DTCG value (`json!("#3080ff")`,
    /// `json!("{blue.9}")`).
    pub fn override_token(mut self, path: impl Into<String>, value: Json) -> Self {
        self.overrides.insert(path.into(), value);
        self
    }

    /// Overrides one token of the counterpart only: from the first one,
    /// the counterpart has overrides of its own
    /// ([`ThemeSpec::counterpart_overrides`]), starting empty.
    pub fn counterpart_override(mut self, path: impl Into<String>, value: Json) -> Self {
        self.counterpart_overrides
            .get_or_insert_with(BTreeMap::new)
            .insert(path.into(), value);
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
        if (self.text_size - 1.0).abs() > f32::EPSILON {
            parts.push(format!("text {}%", (self.text_size * 100.0).round()));
        }
        for f in [&self.font_ui, &self.font_mono].into_iter().flatten() {
            parts.push(f.clone());
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

    /// Whether the theme is a dark one: its panels are dark.
    pub fn is_dark(&self) -> bool {
        self.color("surface.panel").is_some_and(Color::is_dark)
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
            // The alpha modifier is in the value already: kept, it would
            // apply twice when the file is read back.
            if let Some(e) = without_alpha(token.extensions.as_ref()) {
                entry.insert("$extensions".into(), e);
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
                // The default kit, read as any kit folder is; its tokens
                // are `pop-dark`'s own.
                let kit = crate::kit::built_in(KIT_FILES);
                for p in kit.problems() {
                    if matches!(
                        p,
                        crate::kit::Problem::Tokens(_) | crate::kit::Problem::Repeated { .. }
                    ) {
                        panic!("the default kit: {p}");
                    }
                }
                for (path, token) in kit.theme.tokens {
                    let clash = file.tokens.insert(path.clone(), token);
                    assert!(clash.is_none(), "the default kit repeats {path}");
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

    /// Reads a theme file (YAML or JSON, `TokenFile::parse`) and adds it
    /// under `name`, loaded over its base theme ([`Themes::check_file`]).
    /// Only a file that cannot be read at all is an error; what was read
    /// otherwise than written comes back as warnings.
    ///
    /// A file carrying its counterpart ([`COUNTERPART_EXTENSION`]) adds
    /// it too, under its own name and through the same rule; a
    /// counterpart that cannot be read is a warning, the file's own
    /// variant added alone.
    pub fn add_text(&mut self, name: &str, text: &str) -> Result<Vec<Warning>, Error> {
        let (file, mut warnings) = self.check_file(TokenFile::parse(name, text)?);
        warnings.extend(self.add_counterpart_of(&file));
        self.add(file);
        Ok(warnings)
    }

    /// Adds the counterpart a file carries, if any.
    fn add_counterpart_of(&mut self, file: &TokenFile) -> Vec<Warning> {
        match file.counterpart_file() {
            None => Vec::new(),
            Some(Ok(other)) => {
                let (other, warnings) = self.check_file(other);
                self.add(other);
                warnings
            }
            Some(Err(e)) => vec![Warning::Invalid {
                path: format!("$extensions.{COUNTERPART_EXTENSION}"),
                reason: e.to_string(),
            }],
        }
    }

    /// Reads a theme file from disk (a user's theme) and adds it under
    /// `name`, as [`Themes::add_text`] does. Its YAML goes through the
    /// default cache ([`crate::cache`]): read again unchanged, it is not
    /// parsed again, and the loading rule still runs, so its warnings
    /// are the same on a hit.
    pub fn add_path(&mut self, name: &str, path: &Path) -> Result<Vec<Warning>, Error> {
        self.add_path_with(name, path, &crate::cache::default())
    }

    /// [`Themes::add_path`] through a given cache. An entry that cannot
    /// be written is a [`Warning::NotCached`]: the file is read all the
    /// same.
    pub fn add_path_with(
        &mut self,
        name: &str,
        path: &Path,
        cache: &Cache,
    ) -> Result<Vec<Warning>, Error> {
        let (file, unwritten) = TokenFile::read(name, path, cache);
        let (file, mut warnings) = self.check_file(file?);
        warnings.extend(self.add_counterpart_of(&file));
        self.add(file);
        if let Some(e) = unwritten {
            warnings.push(Warning::NotCached {
                file: path.display().to_string(),
                reason: e.to_string(),
            });
        }
        Ok(warnings)
    }

    /// Loads a theme file over its base theme (its `$extends`, else
    /// [`FALLBACK_THEME`]), the rule every theme file goes through: the
    /// file is brought to the current contract ([`TokenFile::upgrade`]),
    /// a seed that cannot be read ([`crate::palette::SEEDS_EXTENSION`])
    /// is left out, then a token whose value does not load (an unreadable value, an
    /// alias to nothing or in a cycle, another type than the base's or
    /// the contract's) is left out, so the base theme's value stays. A
    /// token the base does not have is kept as the file's own (a
    /// palette, an extension's component); under a group of the
    /// contract, with a term no token of the base uses, it is reported
    /// as a likely typo. Nothing here fails: each such thing is a
    /// [`Warning`]. A base not known yet leaves the tokens unchecked.
    pub fn check_file(&self, mut file: TokenFile) -> (TokenFile, Vec<Warning>) {
        let mut warnings = file.upgrade();
        warnings.extend(file.seeds().1);
        let base = file
            .extends
            .clone()
            .unwrap_or_else(|| FALLBACK_THEME.to_string());
        if base != file.name && !self.themes.contains_key(&base) {
            warnings.push(Warning::Unchecked { base });
            return (file, warnings);
        }
        let base_raw = self.flatten(&base, &mut Vec::new());
        let groups: BTreeSet<&str> = contract()
            .filter_map(|r| r.path.split('.').next())
            .collect();
        let terms: BTreeSet<&str> = base_raw
            .keys()
            .filter_map(|p| p.rsplit('.').next())
            .collect();
        for path in file.tokens.keys() {
            if base_raw.contains_key(path) || required(path).is_some() {
                continue;
            }
            let group = path.split('.').next().unwrap_or_default();
            let term = path.rsplit('.').next().unwrap_or_default();
            if groups.contains(group) && !terms.contains(term) {
                warnings.push(Warning::Unknown {
                    path: path.clone(),
                    base: base.clone(),
                    kept: true,
                });
            }
        }
        // The file resolved as it would be, without a density overlay
        // hiding its own space and size tokens: what breaks is left out,
        // until nothing does (leaving a token out can break another one
        // aliasing it).
        let mut trial = self.clone();
        trial.densities.clear();
        let base_types: BTreeMap<String, String> = trial
            .resolve_without_fallback(&ThemeSpec::new(base.clone()))
            .tokens
            .into_iter()
            .map(|(path, t)| (path, t.ty))
            .collect();
        loop {
            trial.add(file.clone());
            let r = trial.resolve_without_fallback(&ThemeSpec::new(file.name.clone()));
            let mut bad: BTreeMap<String, String> = BTreeMap::new();
            for p in &r.problems {
                if matches!(p, Error::Missing { .. } | Error::UnknownOverride { .. }) {
                    continue;
                }
                let own: Vec<&str> = match p {
                    Error::AliasCycle { chain } => chain.iter().map(String::as_str).collect(),
                    _ => p.path().into_iter().collect(),
                };
                for path in own {
                    if file.tokens.contains_key(path) {
                        bad.entry(path.to_string()).or_insert_with(|| p.to_string());
                    }
                }
            }
            for (path, t) in &r.tokens {
                if let Some(ty) = base_types.get(path)
                    && *ty != t.ty
                    && file.tokens.contains_key(path)
                {
                    bad.entry(path.clone())
                        .or_insert_with(|| format!("a {}, where `{base}` has a {ty}", t.ty));
                }
            }
            if bad.is_empty() {
                break;
            }
            for (path, reason) in bad {
                file.tokens.remove(&path);
                warnings.push(Warning::Invalid { path, reason });
            }
        }
        (file, warnings)
    }

    /// Replaces a density's overlay.
    pub fn set_density(&mut self, density: Density, overlay: TokenFile) {
        self.densities.insert(density, overlay);
    }

    pub fn get(&self, name: &str) -> Option<&TokenFile> {
        self.themes.get(name)
    }

    /// The other variant of a theme, as the token files say
    /// ([`VARIANT_OF_EXTENSION`]): the theme its own file names, else the
    /// first theme (by name) naming it. Only a known theme other than
    /// itself counts.
    pub fn counterpart_of(&self, name: &str) -> Option<&str> {
        let named = |file: &TokenFile| {
            file.extensions
                .as_ref()
                .and_then(|e| e.get(VARIANT_OF_EXTENSION))
                .and_then(Json::as_str)
                .map(String::from)
        };
        if let Some(other) = self.themes.get(name).and_then(named)
            && other != name
            && let Some((key, _)) = self.themes.get_key_value(&other)
        {
            return Some(key);
        }
        self.themes
            .iter()
            .find(|(n, file)| n.as_str() != name && named(file).as_deref() == Some(name))
            .map(|(n, _)| n.as_str())
    }

    /// The counterpart a spec asks for ([`ThemeSpec::counterpart`]): a
    /// known theme other than the base, or none.
    pub fn counterpart(&self, spec: &ThemeSpec) -> Option<String> {
        match &spec.counterpart {
            Counterpart::Auto => self.counterpart_of(&spec.base).map(String::from),
            Counterpart::Theme(t) => {
                (*t != spec.base && self.themes.contains_key(t)).then(|| t.clone())
            }
            Counterpart::None => None,
        }
    }

    /// Resolves a spec's counterpart, if it has one
    /// ([`Themes::counterpart`]), with the spec's density, accent,
    /// reduced motion and overrides, or the counterpart's own overrides
    /// ([`ThemeSpec::counterpart_variant`]).
    pub fn resolve_counterpart(&self, spec: &ThemeSpec) -> Option<ResolvedTokens> {
        self.counterpart(spec)
            .map(|other| self.resolve(&spec.counterpart_variant(other)))
    }

    /// The names of the themes known, sorted.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.themes.keys().map(String::as_str)
    }

    /// The theme's tokens with everything it extends, the theme's own
    /// winning. An unknown theme falls back to [`FALLBACK_THEME`].
    pub(crate) fn flatten(
        &self,
        name: &str,
        problems: &mut Vec<Error>,
    ) -> BTreeMap<String, RawToken> {
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
                // The scales the file gives by their seed, under its own
                // tokens: a step it also gives keeps its value.
                let (seeds, _) = theme.seeds();
                if !seeds.is_empty() {
                    let generated = seeded_scales(&tokens, &theme.tokens, &seeds);
                    layer(&mut tokens, &generated);
                }
                layer(&mut tokens, &theme.tokens);
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
        if let Some(asked) = &spec.accent {
            let scale = match spec.accent_seed() {
                Some(seed) => {
                    let color = |path: &str| match resolve_one(&raw, path) {
                        Ok((_, Value::Color(c))) => Some(c),
                        _ => None,
                    };
                    let scheme = color("surface.panel").map_or(Scheme::Dark, Scheme::of_background);
                    let backdrop = Backdrop::from_tokens(color);
                    for (path, value) in Ramp::fitted(seed, scheme, &backdrop).tokens(SEED_SCALE) {
                        raw.insert(
                            path,
                            RawToken {
                                ty: Some("color".into()),
                                value,
                                description: Some(format!("Generated from {}.", seed.to_hex())),
                                extensions: None,
                            },
                        );
                    }
                    SEED_SCALE
                }
                None => asked.as_str(),
            };
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
                    name: asked.clone(),
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
        inherit(&mut raw, &mut problems);
        for (path, value) in &spec.overrides {
            match raw.get_mut(path) {
                // An override is the value as wanted: the alpha modifier
                // of the value it replaces goes with it.
                Some(token) => {
                    token.value = value.clone();
                    token.extensions = without_alpha(token.extensions.as_ref());
                }
                None => problems.push(Error::UnknownOverride { path: path.clone() }),
            }
        }
        user_fonts(&mut raw, spec);
        text_size(&mut raw, spec.text_size);

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

/// Puts the user's families (`ThemeSpec::font_ui`, `font_mono`) first
/// in the theme's lists.
fn user_fonts(raw: &mut BTreeMap<String, RawToken>, spec: &ThemeSpec) {
    for (path, family) in [
        ("font.family.ui", &spec.font_ui),
        ("font.family.mono", &spec.font_mono),
    ] {
        let Some(family) = family else { continue };
        let theirs = match resolve_one(raw, path) {
            Ok((_, Value::FontFamily(list))) => list,
            _ => Vec::new(),
        };
        let mut list = vec![family.clone()];
        list.extend(theirs.into_iter().filter(|f| f != family));
        let entry = raw.entry(path.to_string()).or_insert_with(|| RawToken {
            ty: Some("fontFamily".into()),
            value: Json::Null,
            description: None,
            extensions: None,
        });
        entry.value = json!(list);
    }
}

/// Multiplies the size scale by the user's text size (each step rounded
/// to half a point), and grows the control height with the body text.
fn text_size(raw: &mut BTreeMap<String, RawToken>, factor: f32) {
    if (factor - 1.0).abs() <= f32::EPSILON || !factor.is_finite() || factor <= 0.0 {
        return;
    }
    for path in SIZE_SCALE {
        let Ok((_, Value::Dimension(size))) = resolve_one(raw, path) else {
            continue;
        };
        let scaled = (size * factor * 2.0).round() / 2.0;
        if let Some(token) = raw.get_mut(path) {
            token.value = json!({ "value": scaled, "unit": "px" });
        }
    }
    let body = match resolve_one(raw, "font.text-style.body") {
        Ok((_, Value::Dimension(b))) => b,
        _ => return,
    };
    if let Ok((_, Value::Dimension(height))) = resolve_one(raw, "size.control.height") {
        let wanted = (body * 1.6).ceil();
        if wanted > height
            && let Some(token) = raw.get_mut("size.control.height")
        {
            token.value = json!({ "value": wanted, "unit": "px" });
        }
    }
}

/// The component term naming the component another is based on
/// (`command-button.based-on: "button"`, `docs/kit.md`).
pub const BASED_ON: &str = "based-on";

/// The component a component is based on, as its tokens say
/// (`<component>.based-on`); `None` when it gives none, or no name.
pub(crate) fn base_of<'a>(raw: &'a BTreeMap<String, RawToken>, component: &str) -> Option<&'a str> {
    let token = raw.get(&format!("{component}.{BASED_ON}"))?;
    token
        .value
        .as_str()
        .filter(|_| alias_target(&token.value).is_none())
}

/// Components based on another (`<component>.based-on`) receive every
/// token of their base they do not give themselves, as an alias of it
/// (`command-button.hover.fill: "{button.hover.fill}"`): states, tones,
/// component terms and the `shape`. Chains are followed, each base
/// completed before the components based on it. A cycle is a problem,
/// and its members keep their own tokens only; so is a base no token
/// names. Done once per resolution, before the overrides, so that an
/// override of the base reaches the components based on it.
fn inherit(raw: &mut BTreeMap<String, RawToken>, problems: &mut Vec<Error>) {
    let suffix = format!(".{BASED_ON}");
    let mut bases: BTreeMap<String, String> = BTreeMap::new();
    for (path, token) in raw.iter() {
        let Some(component) = path.strip_suffix(&suffix) else {
            continue;
        };
        if component.contains('.') {
            continue;
        }
        match base_of(raw, component) {
            Some(base) if !base.is_empty() => {
                bases.insert(component.to_string(), base.to_string());
            }
            _ => problems.push(Error::InvalidValue {
                path: path.clone(),
                ty: "string".into(),
                reason: format!("`{}` does not name a component", token.value),
            }),
        }
    }
    if bases.is_empty() {
        return;
    }
    // The members of cycles keep their own tokens.
    let mut cyclic: BTreeSet<String> = BTreeSet::new();
    for start in bases.keys() {
        let mut chain = vec![start.clone()];
        let mut at = start;
        while let Some(next) = bases.get(at) {
            if let Some(i) = chain.iter().position(|c| c == next) {
                let members = &chain[i..];
                if members.iter().all(|m| !cyclic.contains(m)) {
                    let mut shown = members.to_vec();
                    shown.push(next.clone());
                    problems.push(Error::BasedOnCycle { chain: shown });
                    cyclic.extend(members.iter().cloned());
                }
                break;
            }
            chain.push(next.clone());
            at = next;
        }
    }
    let mut done: BTreeSet<String> = BTreeSet::new();
    let components: Vec<String> = bases.keys().cloned().collect();
    for component in components {
        complete(raw, &bases, &cyclic, &mut done, &component, problems);
    }
}

/// Gives `component` its base's tokens, its base completed first.
fn complete(
    raw: &mut BTreeMap<String, RawToken>,
    bases: &BTreeMap<String, String>,
    cyclic: &BTreeSet<String>,
    done: &mut BTreeSet<String>,
    component: &str,
    problems: &mut Vec<Error>,
) {
    if cyclic.contains(component) || !done.insert(component.to_string()) {
        return;
    }
    let Some(base) = bases.get(component) else {
        return;
    };
    complete(raw, bases, cyclic, done, base, problems);
    let prefix = format!("{base}.");
    let inherited: Vec<(String, RawToken)> = raw
        .range(prefix.clone()..)
        .take_while(|(path, _)| path.starts_with(&prefix))
        .filter_map(|(path, token)| {
            let term = &path[prefix.len()..];
            if term == BASED_ON {
                return None;
            }
            let own = format!("{component}.{term}");
            (!raw.contains_key(&own)).then(|| {
                (
                    own,
                    RawToken {
                        ty: token.ty.clone(),
                        value: json!(format!("{{{path}}}")),
                        description: None,
                        extensions: None,
                    },
                )
            })
        })
        .collect();
    let has_base = raw
        .range(prefix.clone()..)
        .next()
        .is_some_and(|(path, _)| path.starts_with(&prefix));
    if !has_base {
        problems.push(Error::UnknownBase {
            path: format!("{component}.{BASED_ON}"),
            base: base.clone(),
        });
        return;
    }
    raw.extend(inherited);
}

/// Lays `over` on `under`: values replace, a token without a type keeps the
/// type it had.
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

/// Whether `path` is a token of one of `scales` (`blue.3` of `blue`).
pub(crate) fn in_scales<'a>(mut scales: impl Iterator<Item = &'a String>, path: &str) -> bool {
    scales.any(|s| {
        path.strip_prefix(s.as_str())
            .is_some_and(|rest| rest.starts_with('.'))
    })
}

/// What a seeded scale is fitted to, and the scheme it is generated for
/// when its seed does not say: the theme as `under` and `own` give it,
/// the seeded scales' own tokens left out (so a scale never fits itself).
pub(crate) fn seed_backdrop(
    under: &BTreeMap<String, RawToken>,
    own: &BTreeMap<String, RawToken>,
    seeds: &BTreeMap<String, Seed>,
) -> (Backdrop, Scheme) {
    let mut raw = under.clone();
    let others: BTreeMap<String, RawToken> = own
        .iter()
        .filter(|(p, _)| !in_scales(seeds.keys(), p))
        .map(|(p, t)| (p.clone(), t.clone()))
        .collect();
    layer(&mut raw, &others);
    let color = |path: &str| match resolve_one(&raw, path) {
        Ok((_, Value::Color(c))) => Some(c),
        _ => None,
    };
    let scheme = color("surface.panel").map_or(Scheme::Dark, Scheme::of_background);
    (Backdrop::from_tokens(color), scheme)
}

/// The tokens of the scales a file gives by their seed
/// ([`crate::palette::SEEDS_EXTENSION`]), generated over `under` (the
/// themes the file extends) and fitted to the theme with the file's own
/// tokens ([`seed_backdrop`]). A scale the themes under it already have
/// gets the steps they have (a neutral scale has no `contrast`); a new
/// one gets its twelve steps and `contrast`.
pub(crate) fn seeded_scales(
    under: &BTreeMap<String, RawToken>,
    own: &BTreeMap<String, RawToken>,
    seeds: &BTreeMap<String, Seed>,
) -> BTreeMap<String, RawToken> {
    let (backdrop, scheme) = seed_backdrop(under, own, seeds);
    let mut out = BTreeMap::new();
    for (scale, seed) in seeds {
        let ramp = Ramp::fitted(seed.color, seed.scheme.unwrap_or(scheme), &backdrop);
        let known = under.contains_key(&format!("{scale}.1"));
        for (path, value) in ramp.tokens(scale) {
            if known && !under.contains_key(&path) && !own.contains_key(&path) {
                continue;
            }
            out.insert(
                path,
                RawToken {
                    ty: Some("color".into()),
                    value,
                    description: Some(format!("Generated from {}.", seed.color.to_hex())),
                    extensions: None,
                },
            );
        }
    }
    out
}

fn is_accent_scale(raw: &BTreeMap<String, RawToken>, scale: &str) -> bool {
    ACCENT_ROLES
        .iter()
        .all(|(_, step)| raw.contains_key(&format!("{scale}.{step}")))
}

/// `$extensions` without the alpha modifier, `None` when nothing else
/// is left.
fn without_alpha(extensions: Option<&Json>) -> Option<Json> {
    let mut e = extensions?.clone();
    if let Some(map) = e.as_object_mut() {
        map.remove(ALPHA_EXTENSION);
        if map.is_empty() {
            return None;
        }
    }
    Some(e)
}

/// A token's alpha modifier ([`ALPHA_EXTENSION`]), if it carries one.
fn alpha_of(path: &str, token: &RawToken) -> Result<Option<f32>, Error> {
    let Some(a) = token
        .extensions
        .as_ref()
        .and_then(|e| e.get(ALPHA_EXTENSION))
    else {
        return Ok(None);
    };
    match a.as_f64() {
        Some(a) if (0.0..=1.0).contains(&a) => Ok(Some(a as f32)),
        _ => Err(Error::InvalidModifier {
            path: path.to_string(),
            reason: format!("`{ALPHA_EXTENSION}` is {a}, not a number from 0 to 1"),
        }),
    }
}

/// A token's type and value, aliases followed, alpha modifiers applied.
fn resolve_one(raw: &BTreeMap<String, RawToken>, path: &str) -> Result<(String, Value), Error> {
    let mut chain = vec![path.to_string()];
    let mut declared = raw.get(path).and_then(|t| t.ty.clone());
    let mut at = raw.get(path).ok_or_else(|| Error::Missing {
        path: path.to_string(),
    })?;
    // The alpha to multiply by, and the first token asking for it.
    let mut alpha = alpha_of(path, at)?.map(|a| (a, path.to_string()));
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
        if let Some(a) = alpha_of(target, next)? {
            alpha = Some(match alpha {
                Some((b, carrier)) => (a * b, carrier),
                None => (a, target.to_string()),
            });
        }
        chain.push(target.to_string());
        at = next;
    }
    let ty = declared.ok_or_else(|| Error::MissingType {
        path: path.to_string(),
    })?;
    let mut value = Value::parse(&ty, &at.value).map_err(|reason| Error::InvalidValue {
        path: path.to_string(),
        ty: ty.clone(),
        reason,
    })?;
    if let Some((a, carrier)) = alpha {
        match &mut value {
            Value::Color(c) => c.a *= a,
            _ => {
                return Err(Error::InvalidModifier {
                    path: carrier,
                    reason: format!("`{ALPHA_EXTENSION}` is for colours, this is a {ty}"),
                });
            }
        }
    }
    Ok((ty, value))
}
