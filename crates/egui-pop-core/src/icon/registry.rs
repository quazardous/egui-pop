//! The icon vocabulary: every name an icon can be declared by, resolved
//! once, and the names that did not resolve.
//!
//! A name is one of three kinds:
//!
//! - **egui-pop's**, bare and in kebab case (`settings`, `chevron-right`):
//!   [`Icon::name`] of every icon in [`Icon::ALL`];
//! - **a Phosphor glyph's**, prefixed (`phosphor:export`,
//!   `phosphor:grid-four`): every glyph of the regular Phosphor font, with
//!   the `phosphor` feature; without it these names do not resolve;
//! - **an application's or a plugin's**, registered as an SVG
//!   (`crate::icon_set::register`); a plugin's are prefixed by its id
//!   (`my-plugin:mask-circle`, `crate::icon_set::register_for`).
//!
//! [`lookup`] resolves a name, once: the answer is kept, and forgotten
//! only when a registration changes what the name means. [`resolve`] does
//! the same and reports a name that does not resolve: a warning in the
//! log the first time, and a line in [`unknown`] that the debug bridge's
//! `icons` collector shows (ADR 0003: never a silent no-op). A surface
//! that cannot draw the icon of an unknown name draws a neutral
//! placeholder ([`PLACEHOLDER`], or the title's first letter where there
//! is one).
//!
//! What a surface drew by name can be recorded each pass ([`record`]),
//! so agents read which icon a row or a field shows without pixels
//! ([`drawn`]).

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{LazyLock, Mutex, RwLock};

use egui::Context;
use serde::Serialize;

use super::Icon;
use crate::state::{lock, pass, shared};

/// The prefix of the Phosphor glyphs' names (`phosphor:export`).
pub const PHOSPHOR_PREFIX: &str = "phosphor:";

/// What a surface without a title draws for a name that does not
/// resolve: a hollow ring, so the place is visibly taken.
pub const PLACEHOLDER: Icon = Icon::Circle;

/// A glyph of the Phosphor font, by its index in the font's list (with
/// the `phosphor` feature; without it, none exists).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PhosphorIcon(u16);

impl PhosphorIcon {
    /// Its name, prefixed (`phosphor:grid-four`).
    pub fn name(self) -> &'static str {
        phosphor::names()
            .get(usize::from(self.0))
            .map_or(PHOSPHOR_PREFIX, String::as_str)
    }

    /// Its glyph in the icon font.
    pub fn glyph(self) -> Option<&'static str> {
        phosphor::glyph(self.0)
    }
}

#[cfg(feature = "phosphor")]
mod phosphor {
    use std::sync::LazyLock;

    use egui_phosphor::regular::ICONS;

    static NAMES: LazyLock<Vec<String>> = LazyLock::new(|| {
        ICONS
            .iter()
            .map(|(n, _)| {
                format!(
                    "{}{}",
                    super::PHOSPHOR_PREFIX,
                    n.to_ascii_lowercase().replace('_', "-")
                )
            })
            .collect()
    });

    pub(super) fn names() -> &'static [String] {
        &NAMES
    }

    pub(super) fn find(name: &str) -> Option<u16> {
        names()
            .iter()
            .position(|n| n == name)
            .and_then(|i| u16::try_from(i).ok())
    }

    pub(super) fn glyph(i: u16) -> Option<&'static str> {
        ICONS.get(usize::from(i)).map(|(_, g)| *g)
    }
}

#[cfg(not(feature = "phosphor"))]
mod phosphor {
    pub(super) fn names() -> &'static [String] {
        &[]
    }

    pub(super) fn find(_name: &str) -> Option<u16> {
        None
    }

    pub(super) fn glyph(_i: u16) -> Option<&'static str> {
        None
    }
}

/// Whether the Phosphor names resolve (the `phosphor` feature).
pub fn phosphor_available() -> bool {
    cfg!(feature = "phosphor")
}

/// Every Phosphor glyph's name, prefixed, in the font's order; empty
/// without the `phosphor` feature.
pub fn phosphor_names() -> &'static [String] {
    phosphor::names()
}

/// egui-pop's own names, in [`Icon::ALL`]'s order.
pub fn builtin_names() -> impl Iterator<Item = &'static str> {
    Icon::ALL.iter().map(|i| i.name())
}

/// The names resolved so far, and what each gave.
static CACHE: LazyLock<RwLock<HashMap<String, Option<Icon>>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// The names that did not resolve, with the surfaces that asked.
static UNKNOWN: LazyLock<Mutex<BTreeMap<String, BTreeSet<String>>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));

fn find(name: &str) -> Option<Icon> {
    if name.starts_with(PHOSPHOR_PREFIX) {
        return phosphor::find(name).map(|i| Icon::Phosphor(PhosphorIcon(i)));
    }
    Icon::ALL
        .iter()
        .copied()
        .find(|i| i.name() == name)
        .or_else(|| crate::icon_set::by_name(name))
}

/// The icon of a name, silently: egui-pop's names, then the Phosphor
/// glyphs' (`phosphor:<name>`), then the registered ones. Each name is
/// looked up once; the answer is kept.
pub fn lookup(name: &str) -> Option<Icon> {
    if let Some(hit) = CACHE.read().unwrap_or_else(|e| e.into_inner()).get(name) {
        return *hit;
    }
    let found = find(name);
    CACHE
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .insert(name.to_string(), found);
    found
}

/// Whether a name resolves.
pub fn is_known(name: &str) -> bool {
    lookup(name).is_some()
}

/// The icon of a name, as [`lookup`], reporting a name that does not
/// resolve: logged once, and listed by [`unknown`] with `surface` (what
/// declared or drew it: `indicator`, `menu list item`, `panel
/// egui_pop.jobs`).
pub fn resolve(name: &str, surface: &str) -> Option<Icon> {
    let found = lookup(name);
    if found.is_none() {
        note_unknown(name, surface);
    }
    found
}

/// Why a name may not resolve, for the warning.
pub fn hint(name: &str) -> &'static str {
    if name.starts_with(PHOSPHOR_PREFIX) && !phosphor_available() {
        "egui-pop was built without the `phosphor` feature"
    } else if name.starts_with(PHOSPHOR_PREFIX) {
        "the Phosphor set has no glyph of that name"
    } else if name.contains(':') {
        "no icon is registered under that name"
    } else {
        "not one of egui-pop's names (a Phosphor glyph is `phosphor:<name>`, a plugin's `<plugin-id>:<name>`)"
    }
}

fn note_unknown(name: &str, surface: &str) {
    let mut u = lock(&UNKNOWN);
    match u.get_mut(name) {
        Some(s) if s.contains(surface) => {}
        Some(s) => {
            s.insert(surface.to_string());
        }
        None => {
            log::warn!(
                target: "egui_pop",
                "unknown icon `{name}` ({surface}): {}; a placeholder is drawn",
                hint(name)
            );
            u.insert(name.to_string(), BTreeSet::from([surface.to_string()]));
        }
    }
}

/// Forgets what a name resolved to, and that it was unknown: a
/// registration changed it.
pub(crate) fn forget(name: &str) {
    CACHE
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .remove(name);
    lock(&UNKNOWN).remove(name);
}

/// A name that did not resolve, and who asked for it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Unknown {
    pub name: String,
    /// The surfaces that declared or drew it.
    pub surfaces: Vec<String>,
    /// Why it may not resolve.
    pub hint: &'static str,
}

/// The names asked of [`resolve`] that did not resolve, by name.
pub fn unknown() -> Vec<Unknown> {
    lock(&UNKNOWN)
        .iter()
        .map(|(name, s)| Unknown {
            name: name.clone(),
            surfaces: s.iter().cloned().collect(),
            hint: hint(name),
        })
        .collect()
}

/// Names of the vocabulary containing `query` (egui-pop's, the
/// registered ones, the Phosphor glyphs'), at most `limit`.
pub fn search(query: &str, limit: usize) -> Vec<String> {
    let q = query.to_ascii_lowercase();
    builtin_names()
        .map(str::to_string)
        .chain(crate::icon_set::names().into_iter().map(str::to_string))
        .chain(phosphor_names().iter().cloned())
        .filter(|n| n.contains(&q))
        .take(limit)
        .collect()
}

/// An icon given by value or by name: what a widget taking an icon
/// accepts (`Icon::Search`, `"search"`, `"phosphor:export"`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum IconRef {
    Icon(Icon),
    /// A name of the vocabulary, resolved when used.
    Name(String),
}

impl IconRef {
    /// The icon, or `None` for a name that does not resolve (reported,
    /// see [`resolve`]).
    pub fn resolve(&self, surface: &str) -> Option<Icon> {
        match self {
            IconRef::Icon(i) => Some(*i),
            IconRef::Name(n) => resolve(n, surface),
        }
    }

    /// The icon, or [`PLACEHOLDER`] for a name that does not resolve
    /// (reported).
    pub fn or_placeholder(&self, surface: &str) -> Icon {
        self.resolve(surface).unwrap_or(PLACEHOLDER)
    }

    /// Its name, as declared.
    pub fn name(&self) -> Cow<'_, str> {
        match self {
            IconRef::Icon(i) => Cow::Borrowed(i.name()),
            IconRef::Name(n) => Cow::Borrowed(n),
        }
    }
}

impl From<Icon> for IconRef {
    fn from(icon: Icon) -> Self {
        IconRef::Icon(icon)
    }
}

impl From<&str> for IconRef {
    fn from(name: &str) -> Self {
        IconRef::Name(name.to_string())
    }
}

impl From<String> for IconRef {
    fn from(name: String) -> Self {
        IconRef::Name(name)
    }
}

impl From<&String> for IconRef {
    fn from(name: &String) -> Self {
        IconRef::Name(name.clone())
    }
}

/// An icon a surface drew, as agents read it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Drawn {
    /// What drew it: `tree`, `input`, `breadcrumb`...
    pub surface: String,
    /// The text it stands beside (a row's label, a field's).
    pub label: String,
    /// The icon's name.
    pub icon: String,
}

/// The most icons recorded in one pass.
const DRAWN_MAX: usize = 1024;

#[derive(Debug, Default)]
struct DrawnState {
    pass: u64,
    building: Vec<Drawn>,
    last: Vec<Drawn>,
}

fn rotate(ctx: &Context, s: &mut DrawnState) {
    let now = pass(ctx);
    if s.pass != now {
        s.last = if s.pass + 1 == now {
            std::mem::take(&mut s.building)
        } else {
            s.building.clear();
            Vec::new()
        };
        s.pass = now;
    }
}

/// Records an icon drawn this pass beside `label` on `surface`, so agents
/// see it in [`drawn`] from the next pass.
pub fn record(ctx: &Context, surface: &str, label: &str, icon: &str) {
    let st = shared::<DrawnState>(ctx, "core.icons.drawn");
    let mut s = lock(&st);
    rotate(ctx, &mut s);
    if s.building.len() < DRAWN_MAX {
        s.building.push(Drawn {
            surface: surface.into(),
            label: label.into(),
            icon: icon.into(),
        });
    }
}

/// The icons recorded in the last complete pass.
pub fn drawn(ctx: &Context) -> Vec<Drawn> {
    let st = shared::<DrawnState>(ctx, "core.icons.drawn");
    let mut s = lock(&st);
    rotate(ctx, &mut s);
    s.last.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_built_in_name_resolves_and_is_kept() {
        assert_eq!(lookup("settings"), Some(Icon::Settings));
        assert_eq!(lookup("settings"), Some(Icon::Settings));
        assert!(is_known("chevron-right"));
    }

    #[test]
    fn an_unknown_name_is_reported_once_with_every_surface() {
        assert_eq!(resolve("test-no-such-icon", "indicator"), None);
        assert_eq!(resolve("test-no-such-icon", "indicator"), None);
        assert_eq!(resolve("test-no-such-icon", "panel demo.p"), None);
        let u = unknown();
        let found = u.iter().find(|u| u.name == "test-no-such-icon").unwrap();
        assert_eq!(found.surfaces, ["indicator", "panel demo.p"]);
    }

    #[test]
    fn a_name_given_as_text_resolves_through_the_registry() {
        let r = IconRef::from("search");
        assert_eq!(r.resolve("test"), Some(Icon::Search));
        assert_eq!(IconRef::from(Icon::Close).name(), "close");
        assert_eq!(
            IconRef::from("test-missing-ref").or_placeholder("test"),
            PLACEHOLDER
        );
    }

    #[cfg(feature = "phosphor")]
    #[test]
    fn phosphor_glyphs_are_named_with_their_prefix() {
        let grid = lookup("phosphor:grid-four").expect("a Phosphor glyph");
        assert_eq!(grid.name(), "phosphor:grid-four");
        assert_eq!(grid.glyph(), Some(egui_phosphor::regular::GRID_FOUR));
        assert_eq!(lookup("phosphor:no-such-glyph"), None);
        // Bare, a Phosphor name is not egui-pop's.
        assert_eq!(lookup("grid-four"), None);
        assert_eq!(phosphor_names().len(), egui_phosphor::regular::ICONS.len());
        assert!(search("grid-f", 10).contains(&"phosphor:grid-four".to_string()));
    }

    #[cfg(not(feature = "phosphor"))]
    #[test]
    fn without_the_feature_a_phosphor_name_is_unknown() {
        assert_eq!(resolve("phosphor:export", "test"), None);
        assert!(hint("phosphor:export").contains("feature"));
    }

    #[test]
    fn drawn_icons_are_read_from_the_pass_before() {
        let ctx = Context::default();
        let _ = ctx.run_ui(Default::default(), |ui| {
            record(ui.ctx(), "tree", "Photos", "folder");
        });
        let _ = ctx.run_ui(Default::default(), |_| {});
        assert_eq!(
            drawn(&ctx),
            [Drawn {
                surface: "tree".into(),
                label: "Photos".into(),
                icon: "folder".into(),
            }]
        );
    }
}
