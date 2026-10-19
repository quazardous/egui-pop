//! Kits as packages (`docs/kit-packages.md`): a folder holding a kit's
//! tokens, its fonts and its licence, read at run time or embedded in the
//! binary.
//!
//! ```text
//! underline-tabs/
//!   kit.yaml            a DTCG token file, like a theme (`$extends`, tokens);
//!                       `kit.json` is read too
//!   *.tokens.yaml       optional: more tokens, one file per component
//!                       (`.tokens.json` is read too)
//!   fonts/*.ttf|otf     optional: faces for the families the tokens name
//!   LICENSE             optional: the kit's licence (and its fonts')
//!   icons/              reserved: not read yet
//! ```
//!
//! `kit.yaml` is a theme: its name is the folder's, unless its manifest
//! (`$extensions."org.egui-pop.kit"`) gives one, and it extends `pop-dark`
//! unless it says otherwise. A face's family and weight come from its
//! file name (`Inter-SemiBold.ttf` is `Inter` at 600), or from the
//! manifest when given:
//!
//! ```yaml
//! $extends: "pop-dark"
//! $extensions:
//!   org.egui-pop.kit:
//!     name: "brand"
//!     label: "Brand"
//!     fonts:
//!       - {file: "JetBrainsMono-Regular.ttf", family: "JetBrains Mono", weight: 400}
//! ```
//!
//! Reading a kit never fails: what is wrong (bad YAML or JSON, a term outside the
//! vocabulary, a font file missing) is in [`KitData::problems`], and the
//! kit is still usable with fallbacks (a kit whose `kit.yaml` cannot be
//! read is `pop-dark` under the kit's name).
//!
//! ```
//! let kit = egui_pop_tokens::kit::KitData::from_text(
//!     "underline",
//!     r#"
//!     tab:
//!       shape: {$type: "string", $value: "underline"}
//!     "#,
//! );
//! assert!(egui_pop_tokens::kit::check(&kit).is_empty());
//! let mut themes = egui_pop_tokens::Themes::default();
//! themes.add_kit(&kit);
//! assert!(themes.get("underline").is_some());
//! ```

use std::borrow::Cow;
use std::collections::BTreeSet;
use std::fmt;
use std::path::{Path, PathBuf};

use serde_json::Value as Json;

use crate::contract::{kit_contract, required};
use crate::theme::{BUILT_IN_THEMES, FALLBACK_THEME, ThemeSpec, Themes};
use crate::{Error, TokenFile};

/// The file every kit folder has.
pub const KIT_FILE: &str = "kit.yaml";

/// The names a kit folder's [`KIT_FILE`] may have, the first found read.
pub const KIT_FILES: &[&str] = &["kit.yaml", "kit.yml", "kit.json"];

/// The endings of a kit's other token files.
pub const TOKEN_FILE_ENDINGS: &[&str] = &[".tokens.yaml", ".tokens.yml", ".tokens.json"];

/// Where a kit's manifest lives in `kit.yaml`'s root `$extensions`.
pub const KIT_EXTENSION: &str = "org.egui-pop.kit";

/// The folder under an application's configuration directory where the
/// user's kits are found ([`discover`]).
pub const USER_KITS_DIR: &str = "kits";

/// The six states (`docs/kit.md`), in the order their boxes apply.
pub const STATES: [&str; 6] = ["rest", "selected", "hover", "focus", "active", "disabled"];

/// The terms a state gives (`component.state.term`), with their type.
pub const STATE_TERMS: &[(&str, &str)] = &[
    ("fill", "color"),
    ("border-color", "color"),
    ("border-width", "dimension"),
    ("radius", "dimension"),
    ("text", "color"),
    ("indicator-color", "color"),
    ("indicator-size", "dimension"),
    ("ring-color", "color"),
    ("ring-width", "dimension"),
    ("ring-offset", "dimension"),
];

/// The terms that do not change with the state (`component.term`), with
/// their type.
pub const COMPONENT_TERMS: &[(&str, &str)] = &[
    ("shape", "string"),
    ("padding-x", "dimension"),
    ("padding-y", "dimension"),
    ("height", "dimension"),
    ("width", "dimension"),
    ("font-size", "dimension"),
    ("offset", "dimension"),
    ("indicator-side", "string"),
    ("align", "string"),
    ("opacity", "number"),
    ("buttons-side", "string"),
    ("buttons-hover", "string"),
    ("corners", "string"),
    ("shadow-offset-x", "dimension"),
    ("shadow-offset-y", "dimension"),
    ("shadow-blur", "dimension"),
    ("shadow-spread", "dimension"),
    ("shadow-color", "color"),
];

/// The tones a component may give (`component.tone.<tone>.<term>` and
/// `component.tone.<tone>.<state>.<term>`).
pub const TONES: [&str; 7] = [
    "accent", "info", "success", "warning", "danger", "muted", "record",
];

/// Glyph roles that must keep shapes of their own (`glyph.<role>`), so
/// that colour never carries a state alone: no two roles of a pair may
/// name the same icon. The LEDs off, on, warning and error differ from
/// each other, a busy LED from all but the on one (it pulses, or shows a
/// still ring under reduced motion), and the four named flags from each
/// other. [`check`] reports a kit breaking it, and egui-pop draws its own
/// shapes for those roles.
pub const DISTINCT_GLYPHS: &[(&str, &str)] = &[
    ("led-off", "led-on"),
    ("led-off", "led-warning"),
    ("led-off", "led-error"),
    ("led-on", "led-warning"),
    ("led-on", "led-error"),
    ("led-warning", "led-error"),
    ("led-busy", "led-off"),
    ("led-busy", "led-warning"),
    ("led-busy", "led-error"),
    ("flag-modified", "flag-pinned"),
    ("flag-modified", "flag-required"),
    ("flag-modified", "flag-invalid"),
    ("flag-pinned", "flag-required"),
    ("flag-pinned", "flag-invalid"),
    ("flag-required", "flag-invalid"),
];

/// The pairs of [`DISTINCT_GLYPHS`] naming the same icon in a resolved
/// theme, with that icon's name.
pub fn same_glyphs(theme: &crate::ResolvedTokens) -> Vec<(&'static str, &'static str, String)> {
    let name = |role: &str| match theme.value(&format!("glyph.{role}")) {
        Some(crate::Value::Other(serde_json::Value::String(s))) => Some(s.clone()),
        _ => None,
    };
    DISTINCT_GLYPHS
        .iter()
        .filter_map(|(a, b)| match (name(a), name(b)) {
            (Some(x), Some(y)) if x == y => Some((*a, *b, x)),
            _ => None,
        })
        .collect()
}

/// A font file's bytes: embedded in the binary or read at run time.
pub type Bytes = Cow<'static, [u8]>;

/// A face a kit ships, for the families its tokens name.
#[derive(Clone, PartialEq)]
pub struct KitFont {
    /// Its file name under `fonts/`.
    pub file: String,
    /// The family a theme names it by (`font.family.ui`).
    pub family: String,
    /// 100 to 900, as CSS numbers weights.
    pub weight: u16,
    pub bytes: Bytes,
}

impl fmt::Debug for KitFont {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KitFont")
            .field("file", &self.file)
            .field("family", &self.family)
            .field("weight", &self.weight)
            .field("bytes", &self.bytes.len())
            .finish()
    }
}

/// Where a kit was read from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    /// Embedded in the binary ([`kit!`](crate::kit!)); the folder it was
    /// embedded from, as the macro named it.
    Embedded(&'static str),
    /// Read from a folder at run time.
    Dir(PathBuf),
}

/// What is wrong with a kit. The kit is usable anyway: each problem says
/// what is done instead.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Problem {
    /// A file could not be read.
    Io { file: String, message: String },
    /// A token file is not valid YAML or JSON, or not a token file: its
    /// tokens are left out (all of them for `kit.yaml`: the kit is its
    /// base theme).
    Tokens(Error),
    /// Two of the kit's files give the same token: the first one read
    /// (`kit.yaml`, then the others by name) is kept.
    Repeated { path: String, file: String },
    /// A token file other than `kit.yaml` says what it extends: only
    /// `kit.yaml` does.
    Extends { file: String },
    /// A token of a kit component with a term outside the vocabulary
    /// (`docs/kit.md`): no component reads it.
    UnknownTerm { path: String, reason: String },
    /// A token shaped `component.state.term` for a component no kit has.
    UnknownComponent { path: String, component: String },
    /// A term given with another type than the vocabulary's: the
    /// component reads nothing there.
    TermType {
        path: String,
        expected: String,
        found: String,
    },
    /// The manifest (`$extensions."org.egui-pop.kit"`) is not as expected:
    /// that part of it is ignored.
    Manifest { reason: String },
    /// A face could not be used: it is left out, and the families naming
    /// it fall back (`egui_pop_core::fonts`).
    Font { file: String, reason: String },
    /// The kit's name is a built-in theme's: it is used with `-kit`
    /// added, so the built-in theme is not replaced.
    Name { asked: String, used: String },
    /// A part of the folder egui-pop does not read yet.
    NotRead { what: String },
    /// A problem of the theme the kit resolves to ([`check`]).
    Theme(Error),
    /// Two glyph roles of [`DISTINCT_GLYPHS`] name the same icon: two
    /// states would differ by colour alone, so egui-pop draws its own
    /// shapes for them.
    SameGlyph {
        roles: (String, String),
        icon: String,
    },
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Problem::Io { file, message } => write!(f, "`{file}`: cannot be read: {message}"),
            Problem::Tokens(e) => write!(f, "{e}: its tokens are left out"),
            Problem::Repeated { path, file } => {
                write!(f, "`{file}` gives `{path}` again: the first value is kept")
            }
            Problem::Extends { file } => write!(
                f,
                "`{file}` has `$extends`: only the kit file says what the kit extends, ignored"
            ),
            Problem::UnknownTerm { path, reason } => {
                write!(f, "`{path}`: {reason}; nothing reads it")
            }
            Problem::UnknownComponent { path, component } => write!(
                f,
                "`{path}`: no kit component is named `{component}`; nothing reads it"
            ),
            Problem::TermType {
                path,
                expected,
                found,
            } => write!(
                f,
                "`{path}` is a {found}, the vocabulary says {expected}; nothing reads it"
            ),
            Problem::Manifest { reason } => write!(f, "manifest: {reason}"),
            Problem::Font { file, reason } => write!(f, "font `{file}`: {reason}; left out"),
            Problem::Name { asked, used } => write!(
                f,
                "the kit's name `{asked}` is a built-in theme's: it is `{used}`"
            ),
            Problem::NotRead { what } => write!(f, "{what} is not read yet"),
            Problem::Theme(e) => write!(f, "theme: {e}"),
            Problem::SameGlyph { roles, icon } => write!(
                f,
                "`glyph.{}` and `glyph.{}` both name `{icon}`: their states would differ by colour alone; egui-pop's own shapes are drawn",
                roles.0, roles.1
            ),
        }
    }
}

impl std::error::Error for Problem {}

/// What [`kit!`](crate::kit!) embeds: the folder's files, named as in the
/// folder.
#[derive(Clone, Debug)]
pub struct Embedded {
    /// The folder, as the macro named it.
    pub dir: &'static str,
    /// `kit.yaml` (or `kit.json`).
    pub kit: &'static str,
    /// Other token files: their name and text.
    pub tokens: Vec<(String, &'static str)>,
    /// Faces: their file name under `fonts/` and their bytes.
    pub fonts: Vec<(&'static str, &'static [u8])>,
    pub license: Option<&'static str>,
}

/// A kit's data: its tokens (a theme), its faces and its licence, with
/// what was wrong reading them. A kit with code adds its handlers
/// around it (`egui_pop_core::kit::KitSource`).
#[derive(Clone, Debug, PartialEq)]
pub struct KitData {
    /// Its theme's name; the theme picker lists it under it.
    pub name: String,
    /// A name for people, from the manifest.
    pub label: Option<String>,
    /// The kit's tokens: a theme named [`KitData::name`].
    pub theme: TokenFile,
    pub fonts: Vec<KitFont>,
    pub license: Option<String>,
    pub origin: Origin,
    /// Fonts the manifest names, to check embedded kits against.
    manifest_fonts: Vec<String>,
    problems: Vec<Problem>,
}

impl KitData {
    /// Reads a kit folder (see the module). Never fails: what could not
    /// be read is in [`KitData::problems`].
    pub fn load(dir: impl AsRef<Path>) -> KitData {
        let dir = dir.as_ref();
        let mut problems = Vec::new();
        let read = |file: &Path| {
            std::fs::read_to_string(file).map_err(|e| Problem::Io {
                file: file.display().to_string(),
                message: e.to_string(),
            })
        };
        let kit = read(&kit_file(dir).unwrap_or_else(|| dir.join(KIT_FILE)));
        let mut tokens = Vec::new();
        let mut fonts = Vec::new();
        let mut license = None;
        let mut icons = false;
        match std::fs::read_dir(dir) {
            Ok(entries) => {
                let mut names: Vec<(String, PathBuf)> = entries
                    .filter_map(Result::ok)
                    .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
                    .collect();
                names.sort();
                for (name, path) in names {
                    if TOKEN_FILE_ENDINGS.iter().any(|e| name.ends_with(e)) {
                        tokens.push((name, read(&path)));
                    } else if license.is_none()
                        && matches!(name.as_str(), "LICENSE" | "LICENSE.txt" | "LICENSE.md")
                    {
                        match read(&path) {
                            Ok(text) => license = Some(text),
                            Err(p) => problems.push(p),
                        }
                    } else if name == "icons" {
                        icons = true;
                    }
                }
            }
            Err(e) => problems.push(Problem::Io {
                file: dir.display().to_string(),
                message: e.to_string(),
            }),
        }
        let fonts_dir = dir.join("fonts");
        if let Ok(entries) = std::fs::read_dir(&fonts_dir) {
            let mut files: Vec<(String, PathBuf)> = entries
                .filter_map(Result::ok)
                .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
                .filter(|(n, _)| is_font_file(n))
                .collect();
            files.sort();
            for (name, path) in files {
                match std::fs::read(&path) {
                    Ok(bytes) => fonts.push((name, Bytes::Owned(bytes))),
                    Err(e) => problems.push(Problem::Font {
                        file: name,
                        reason: e.to_string(),
                    }),
                }
            }
        }
        if icons {
            problems.push(Problem::NotRead {
                what: "icons/".into(),
            });
        }
        let folder = dir
            .file_name()
            .map_or_else(|| "kit".into(), |n| n.to_string_lossy().into_owned());
        assemble(
            &folder,
            Origin::Dir(dir.to_path_buf()),
            kit,
            tokens,
            fonts,
            license,
            problems,
        )
    }

    /// A kit embedded in the binary (what [`kit!`](crate::kit!) gives).
    pub fn embedded(e: Embedded) -> KitData {
        let folder = e
            .dir
            .trim_end_matches(['/', '\\'])
            .rsplit(['/', '\\'])
            .next()
            .filter(|n| !n.is_empty() && *n != "." && *n != "..")
            .unwrap_or("kit")
            .to_string();
        assemble(
            &folder,
            Origin::Embedded(e.dir),
            Ok(e.kit.to_string()),
            e.tokens
                .into_iter()
                .map(|(n, t)| (n, Ok(t.to_string())))
                .collect(),
            e.fonts
                .into_iter()
                .map(|(n, b)| (n.to_string(), Bytes::Borrowed(b)))
                .collect(),
            e.license.map(String::from),
            Vec::new(),
        )
    }

    /// A data-only kit from the text of its `kit.yaml` (or `kit.json`),
    /// under a name (the manifest's wins).
    pub fn from_text(name: &str, kit_text: &str) -> KitData {
        assemble(
            name,
            Origin::Embedded(""),
            Ok(kit_text.to_string()),
            Vec::new(),
            Vec::new(),
            None,
            Vec::new(),
        )
    }

    /// What was wrong reading the kit, in the order met; empty for a
    /// clean kit. [`check`] adds the problems of the theme it resolves to.
    pub fn problems(&self) -> &[Problem] {
        &self.problems
    }

    /// The kit's name for people: its label, else its name.
    pub fn display_name(&self) -> &str {
        self.label.as_deref().unwrap_or(&self.name)
    }

    /// The kit as a theme picker lists it.
    pub fn info(&self) -> KitInfo {
        KitInfo {
            name: self.name.clone(),
            label: self.display_name().to_string(),
            description: self.theme.description.clone(),
            origin: self.origin.clone(),
            problems: self.problems.len(),
        }
    }
}

/// A kit as a theme picker shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KitInfo {
    /// The theme's name, for `ThemeSpec::new`.
    pub name: String,
    pub label: String,
    /// `kit.yaml`'s `$description`.
    pub description: Option<String>,
    pub origin: Origin,
    /// How many problems reading it met.
    pub problems: usize,
}

impl Themes {
    /// Adds a kit's theme (replacing a theme of the same name, returned).
    /// Its faces and handlers are installed by `egui_pop_core::kit::KitSource`
    /// or `AppBuilder::kit`.
    pub fn add_kit(&mut self, kit: &KitData) -> Option<TokenFile> {
        self.add(kit.theme.clone())
    }
}

/// Every problem of a kit: reading it, and resolving its theme over the
/// built-in ones. An application's tests call it on the kits it embeds,
/// since [`kit!`](crate::kit!) checks nothing when compiling:
///
/// ```
/// let kit = egui_pop_tokens::kit::KitData::from_text(
///     "k",
///     r##"
///     tab:
///       rest:
///         fil: {$type: "color", $value: "#000000"}
///     "##,
/// );
/// let problems = egui_pop_tokens::kit::check(&kit);
/// assert_eq!(problems.len(), 1, "{problems:?}");
/// ```
pub fn check(kit: &KitData) -> Vec<Problem> {
    let mut problems = kit.problems.clone();
    if let Origin::Embedded(_) = kit.origin {
        for file in &kit.manifest_fonts {
            if !kit.fonts.iter().any(|f| &f.file == file) {
                problems.push(Problem::Font {
                    file: file.clone(),
                    reason: "named by the manifest but not embedded (add it to kit!'s fonts)"
                        .into(),
                });
            }
        }
    }
    let mut themes = Themes::default();
    themes.add_kit(kit);
    let resolved = themes.resolve(&ThemeSpec::new(kit.name.clone()));
    problems.extend(resolved.problems().iter().cloned().map(Problem::Theme));
    problems.extend(
        same_glyphs(&resolved)
            .into_iter()
            .map(|(a, b, icon)| Problem::SameGlyph {
                roles: (a.into(), b.into()),
                icon,
            }),
    );
    problems
}

/// The kits found in a folder: each sub-folder holding a `kit.yaml`, by
/// name (anything else is not a kit and is skipped). An application's
/// user kits are in `<config>/kits` ([`USER_KITS_DIR`]); a folder that
/// does not exist has none.
pub fn discover(dir: impl AsRef<Path>) -> Vec<KitData> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| kit_file(p).is_some())
        .collect();
    dirs.sort();
    dirs.iter().map(KitData::load).collect()
}

/// The default kit: `themes/kit/`, read by the same code as any kit
/// folder. `Themes::default` layers its tokens into `pop-dark`.
pub(crate) fn built_in(files: &[(&str, &'static str)]) -> KitData {
    KitData::embedded(Embedded {
        dir: "themes/kit",
        // Compiled to JSON by `build.rs`, as the token files are.
        kit: include_str!(concat!(env!("OUT_DIR"), "/themes/kit/kit.json")),
        tokens: files
            .iter()
            .map(|(component, text)| (format!("{component}.tokens.yaml"), *text))
            .collect(),
        fonts: Vec::new(),
        license: None,
    })
}

/// A kit folder's [`KIT_FILE`], under the first of [`KIT_FILES`] found.
pub fn kit_file(dir: impl AsRef<Path>) -> Option<PathBuf> {
    let dir = dir.as_ref();
    KIT_FILES
        .iter()
        .map(|name| dir.join(name))
        .find(|p| p.is_file())
}

fn is_font_file(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [".ttf", ".otf", ".ttc"].iter().any(|e| lower.ends_with(e))
}

fn assemble(
    folder: &str,
    origin: Origin,
    kit: Result<String, Problem>,
    tokens: Vec<(String, Result<String, Problem>)>,
    fonts: Vec<(String, Bytes)>,
    license: Option<String>,
    mut problems: Vec<Problem>,
) -> KitData {
    let mut theme = match kit.map(|text| TokenFile::parse(KIT_FILE, &text)) {
        Ok(Ok(file)) => file,
        Ok(Err(e)) => {
            problems.push(Problem::Tokens(e));
            TokenFile::new(KIT_FILE)
        }
        Err(p) => {
            problems.push(p);
            TokenFile::new(KIT_FILE)
        }
    };
    for (file, text) in tokens {
        let part = match text.map(|t| TokenFile::parse(&file, &t)) {
            Ok(Ok(part)) => part,
            Ok(Err(e)) => {
                problems.push(Problem::Tokens(e));
                continue;
            }
            Err(p) => {
                problems.push(p);
                continue;
            }
        };
        if part.extends.is_some() {
            problems.push(Problem::Extends { file: file.clone() });
        }
        for (path, token) in part.tokens {
            match theme.tokens.entry(path) {
                std::collections::btree_map::Entry::Occupied(e) => {
                    problems.push(Problem::Repeated {
                        path: e.key().clone(),
                        file: file.clone(),
                    });
                }
                std::collections::btree_map::Entry::Vacant(e) => {
                    e.insert(token);
                }
            }
        }
    }
    let manifest = Manifest::read(&theme, &mut problems);
    let asked = manifest.name.clone().unwrap_or_else(|| folder.to_string());
    let name = if BUILT_IN_THEMES.contains(&asked.as_str()) {
        let used = format!("{asked}-kit");
        problems.push(Problem::Name {
            asked,
            used: used.clone(),
        });
        used
    } else {
        asked
    };
    theme.name = name.clone();
    if theme.extends.is_none() {
        theme.extends = Some(FALLBACK_THEME.to_string());
    }
    problems.extend(vocabulary(&theme));
    let mut faces = Vec::new();
    for entry in &manifest.fonts {
        if !fonts.iter().any(|(f, _)| f == &entry.file) && matches!(origin, Origin::Dir(_)) {
            problems.push(Problem::Font {
                file: entry.file.clone(),
                reason: "named by the manifest, not in fonts/".into(),
            });
        }
    }
    for (file, bytes) in fonts {
        let (family, weight) = match manifest.fonts.iter().find(|e| e.file == file) {
            Some(e) => (e.family.clone(), e.weight),
            None => match face_from_file_name(&file) {
                Ok(fw) => fw,
                Err((fw, reason)) => {
                    problems.push(Problem::Font {
                        file: file.clone(),
                        reason: format!(
                            "{reason}: kept as `{}` at {} (name it in the manifest)",
                            fw.0, fw.1
                        ),
                    });
                    fw
                }
            },
        };
        if bytes.len() < 12 || !looks_like_font(&bytes) {
            problems.push(Problem::Font {
                file,
                reason: "not a TrueType or OpenType file".into(),
            });
            continue;
        }
        faces.push(KitFont {
            file,
            family,
            weight,
            bytes,
        });
    }
    KitData {
        name,
        label: manifest.label,
        theme,
        fonts: faces,
        license,
        origin,
        manifest_fonts: manifest.fonts.into_iter().map(|e| e.file).collect(),
        problems,
    }
}

/// The first bytes of a TrueType, OpenType or collection file.
fn looks_like_font(bytes: &[u8]) -> bool {
    matches!(
        &bytes[..4],
        [0, 1, 0, 0] | b"OTTO" | b"true" | b"ttcf" | b"typ1"
    )
}

#[derive(Default)]
struct Manifest {
    name: Option<String>,
    label: Option<String>,
    fonts: Vec<ManifestFont>,
}

struct ManifestFont {
    file: String,
    family: String,
    weight: u16,
}

impl Manifest {
    fn read(theme: &TokenFile, problems: &mut Vec<Problem>) -> Manifest {
        let mut m = Manifest::default();
        let Some(raw) = theme.extensions.as_ref().and_then(|x| x.get(KIT_EXTENSION)) else {
            return m;
        };
        let Some(obj) = raw.as_object() else {
            problems.push(Problem::Manifest {
                reason: "is not an object".into(),
            });
            return m;
        };
        let mut text = |key: &str| match obj.get(key) {
            None => None,
            Some(Json::String(s)) if !s.trim().is_empty() => Some(s.clone()),
            Some(other) => {
                problems.push(Problem::Manifest {
                    reason: format!("`{key}`: `{other}` is not a name"),
                });
                None
            }
        };
        m.name = text("name");
        m.label = text("label");
        if let Some(name) = &m.name
            && name.contains(['/', '\\', '{', '}'])
        {
            problems.push(Problem::Manifest {
                reason: format!("`name`: `{name}` may not contain `/`, `\\`, `{{` or `}}`"),
            });
            m.name = None;
        }
        match obj.get("fonts") {
            None => {}
            Some(Json::Array(entries)) => {
                for (i, entry) in entries.iter().enumerate() {
                    let file = entry.get("file").and_then(Json::as_str);
                    let family = entry.get("family").and_then(Json::as_str);
                    let weight = match entry.get("weight") {
                        None => Some(400),
                        Some(w) => w
                            .as_u64()
                            .filter(|w| (1..=1000).contains(w))
                            .and_then(|w| u16::try_from(w).ok()),
                    };
                    match (file, family, weight) {
                        (Some(file), Some(family), Some(weight)) => m.fonts.push(ManifestFont {
                            file: file.into(),
                            family: family.into(),
                            weight,
                        }),
                        _ => problems.push(Problem::Manifest {
                            reason: format!(
                                "`fonts[{i}]` needs a `file`, a `family` and a `weight` from 1 to 1000"
                            ),
                        }),
                    }
                }
            }
            Some(_) => problems.push(Problem::Manifest {
                reason: "`fonts` is not a list".into(),
            }),
        }
        m
    }
}

/// The weights a file name can end with, as CSS numbers them.
const WEIGHT_NAMES: &[(&str, u16)] = &[
    ("thin", 100),
    ("hairline", 100),
    ("extralight", 200),
    ("ultralight", 200),
    ("light", 300),
    ("regular", 400),
    ("normal", 400),
    ("book", 400),
    ("variable", 400),
    ("medium", 500),
    ("semibold", 600),
    ("demibold", 600),
    ("bold", 700),
    ("extrabold", 800),
    ("ultrabold", 800),
    ("black", 900),
    ("heavy", 900),
];

type FamilyWeight = (String, u16);

/// A face's family and weight from its file name: `Family-Weight.ext`
/// (`Inter-SemiBold.ttf`), `Family.ext` at 400, a variable font's axes in
/// brackets dropped (`Inter[wght].ttf`). A suffix that is not a weight is
/// an error, the whole name then taken as the family.
fn face_from_file_name(file: &str) -> Result<FamilyWeight, (FamilyWeight, String)> {
    let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
    let stem = stem.split_once('[').map_or(stem, |(s, _)| s).trim();
    let Some((family, suffix)) = stem.rsplit_once('-') else {
        return Ok((stem.to_string(), 400));
    };
    let key: String = suffix
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    if let Ok(n) = key.parse::<u16>()
        && (1..=1000).contains(&n)
    {
        return Ok((family.to_string(), n));
    }
    match WEIGHT_NAMES.iter().find(|(name, _)| *name == key) {
        Some((_, w)) => Ok((family.to_string(), *w)),
        None => Err((
            (stem.to_string(), 400),
            format!("`{suffix}` is not a weight"),
        )),
    }
}

/// The tokens of kit components that are outside the vocabulary.
fn vocabulary(theme: &TokenFile) -> Vec<Problem> {
    let components: BTreeSet<&str> = kit_contract()
        .filter_map(|r| r.path.split('.').next())
        .collect();
    let mut problems = Vec::new();
    let find = |list: &[(&str, &'static str)], term: &str| {
        list.iter().find(|(t, _)| *t == term).map(|(_, ty)| *ty)
    };
    for (path, token) in &theme.tokens {
        if required(path).is_some() {
            continue;
        }
        let segs: Vec<&str> = path.split('.').collect();
        let unknown = |reason: String| Problem::UnknownTerm {
            path: path.clone(),
            reason,
        };
        let expected = if components.contains(segs[0]) {
            match segs[..] {
                [_, term] => match find(COMPONENT_TERMS, term) {
                    Some(ty) => Some(ty),
                    None if find(STATE_TERMS, term).is_some() => {
                        problems.push(unknown(format!(
                            "`{term}` is given per state (`{}.rest.{term}`)",
                            segs[0]
                        )));
                        None
                    }
                    None => {
                        problems.push(unknown(format!("`{term}` is not a term of the vocabulary")));
                        None
                    }
                },
                [_, state, term] if STATES.contains(&state) => match find(STATE_TERMS, term) {
                    Some(ty) => Some(ty),
                    None => {
                        problems.push(unknown(format!("`{term}` is not a term a state gives")));
                        None
                    }
                },
                [_, "tone", tone, ..] if !TONES.contains(&tone) => {
                    problems.push(unknown(format!(
                        "`{tone}` is not a tone ({})",
                        TONES.join(", ")
                    )));
                    None
                }
                [_, "tone", _, term] => match find(STATE_TERMS, term) {
                    Some(ty) => Some(ty),
                    None => {
                        problems.push(unknown(format!("`{term}` is not a term a tone gives")));
                        None
                    }
                },
                [_, "tone", _, state, term] if STATES.contains(&state) => {
                    match find(STATE_TERMS, term) {
                        Some(ty) => Some(ty),
                        None => {
                            problems.push(unknown(format!("`{term}` is not a term a state gives")));
                            None
                        }
                    }
                }
                [_, state, _] => {
                    problems.push(unknown(format!(
                        "`{state}` is not a state ({})",
                        STATES.join(", ")
                    )));
                    None
                }
                _ => {
                    problems.push(unknown(
                        "a kit token is `component.term` or `component.state.term`".into(),
                    ));
                    None
                }
            }
        } else {
            if let [component, state, term] = segs[..]
                && STATES.contains(&state)
                && find(STATE_TERMS, term).is_some()
            {
                problems.push(Problem::UnknownComponent {
                    path: path.clone(),
                    component: component.into(),
                });
            }
            None
        };
        if let (Some(expected), Some(found)) = (expected, token.ty.as_deref())
            && expected != found
        {
            problems.push(Problem::TermType {
                path: path.clone(),
                expected: expected.into(),
                found: found.into(),
            });
        }
    }
    problems
}

/// Embeds a kit folder in the binary: `kit.yaml` (or the kit file
/// `kit = "kit.json"` names), and the token files,
/// faces (under `fonts/`) and licence it is given. The path is relative
/// to the file calling the macro, as `include_str!`'s. Nothing is checked
/// when compiling: call [`kit::check`](crate::kit::check) from a test.
///
/// ```ignore
/// let kit = egui_pop_tokens::kit!("../kits/brand",
///     tokens = ["tab.tokens.yaml"],
///     fonts = ["Inter-Regular.ttf", "Inter-Bold.ttf"],
///     license = "LICENSE");
/// ```
#[macro_export]
macro_rules! kit {
    ($dir:literal $(, tokens = [$($part:literal),* $(,)?])?
        $(, fonts = [$($font:literal),* $(,)?])?
        $(, license = $license:literal)?
        $(,)?) => {
        $crate::kit!($dir, kit = "kit.yaml"
            $(, tokens = [$($part),*])?
            $(, fonts = [$($font),*])?
            $(, license = $license)?)
    };
    ($dir:literal, kit = $kit:literal
        $(, tokens = [$($part:literal),* $(,)?])?
        $(, fonts = [$($font:literal),* $(,)?])?
        $(, license = $license:literal)?
        $(,)?) => {
        $crate::kit::KitData::embedded($crate::kit::Embedded {
            dir: $dir,
            kit: include_str!(concat!($dir, "/", $kit)),
            tokens: vec![$($((
                ::std::string::String::from($part),
                include_str!(concat!($dir, "/", $part)),
            ),)*)?],
            fonts: vec![$($((
                $font,
                &include_bytes!(concat!($dir, "/fonts/", $font))[..],
            ),)*)?],
            license: {
                #[allow(unused_mut)]
                let mut license: ::std::option::Option<&'static str> = None;
                $(license = Some(include_str!(concat!($dir, "/", $license)));)?
                license
            },
        })
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_face_is_named_by_its_file() {
        assert_eq!(
            face_from_file_name("Inter-SemiBold.ttf"),
            Ok(("Inter".into(), 600))
        );
        assert_eq!(
            face_from_file_name("Inter[wght].ttf"),
            Ok(("Inter".into(), 400))
        );
        assert_eq!(face_from_file_name("Hack.otf"), Ok(("Hack".into(), 400)));
        assert_eq!(
            face_from_file_name("Brand-700.otf"),
            Ok(("Brand".into(), 700))
        );
        assert!(face_from_file_name("Inter-Italic.ttf").is_err());
    }

    #[test]
    fn terms_outside_the_vocabulary_are_reported() {
        let kit = KitData::from_text(
            "k",
            r##"
            tab:
              rest:
                fil: {$type: "color", $value: "#000000"}
              pressed:
                fill: {$type: "color", $value: "#000000"}
              fill: {$type: "color", $value: "#000000"}
              hover:
                radius: {$type: "color", $value: "#000000"}
            tabz:
              hover:
                fill: {$type: "color", $value: "#000000"}
            gray:
              "1": {$type: "color", $value: "#000000"}
            "##,
        );
        let p = kit.problems();
        assert_eq!(p.len(), 5, "{p:?}");
        assert!(p.iter().any(
            |p| matches!(p, Problem::UnknownComponent { component, .. } if component == "tabz")
        ));
        assert!(
            p.iter()
                .any(|p| matches!(p, Problem::TermType { path, .. } if path == "tab.hover.radius"))
        );
    }

    #[test]
    fn tones_are_in_the_vocabulary() {
        let kit = KitData::from_text(
            "k",
            r##"
            window-button:
              tone:
                warning:
                  fill: {$type: "color", $value: "#ffbb00"}
                  active:
                    fill: {$type: "color", $value: "#cc9900"}
                amber:
                  fill: {$type: "color", $value: "#ffbb00"}
                danger:
                  glow: {$type: "color", $value: "#ff0000"}
            title-bar:
              buttons-side: {$type: "string", $value: "start"}
            "##,
        );
        let p = kit.problems();
        assert_eq!(p.len(), 2, "{p:?}");
        assert!(
            p.iter()
                .any(|p| matches!(p, Problem::UnknownTerm { path, .. }
            if path == "window-button.tone.amber.fill"))
        );
        assert!(
            p.iter()
                .any(|p| matches!(p, Problem::UnknownTerm { path, .. }
            if path == "window-button.tone.danger.glow"))
        );
    }

    #[test]
    fn a_kit_extends_pop_dark_and_keeps_built_in_names() {
        let kit = KitData::from_text("pop-light", "{}");
        assert_eq!(kit.name, "pop-light-kit");
        assert_eq!(kit.theme.extends.as_deref(), Some(FALLBACK_THEME));
        assert!(matches!(kit.problems()[0], Problem::Name { .. }));
    }

    #[test]
    fn bad_json_gives_the_base_theme_under_the_kit_name() {
        let kit = KitData::from_text("broken", "{ nope");
        assert!(matches!(
            kit.problems()[0],
            Problem::Tokens(Error::Json { .. })
        ));
        let problems = check(&kit);
        assert_eq!(problems.len(), 1, "{problems:?}");
        let mut themes = Themes::default();
        themes.add_kit(&kit);
        let r = themes.resolve(&ThemeSpec::new("broken"));
        assert!(r.problems().is_empty());
    }
}
