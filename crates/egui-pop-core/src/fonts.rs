//! Text fonts from the theme: the families named by `font.family.ui` and
//! `font.family.mono`, at several weights.
//!
//! Each name of a token's list is looked for in order:
//!
//! 1. faces the application registered ([`register`]): bytes it embeds or
//!    reads from disk;
//! 2. egui's bundled faces by name (`Ubuntu` is egui's Ubuntu-Light,
//!    `Hack` its Hack): the built-in themes name them;
//! 3. the system's fonts by family name (feature `system-fonts`);
//!
//! The first found is the primary, the others its fallbacks, and egui's
//! bundled fonts always come last so text never vanishes. A generic name
//! (`sans-serif`, `serif`, `monospace`, `system-ui`) stands for those
//! bundled fonts. A name found nowhere is a problem in the [`Report`]
//! (ADR 0003), never replaced in silence.
//!
//! egui's `FontId` has no weight: each weight of [`WEIGHTS`] is an egui
//! family of its own, [`family`]`(role, weight)`, and [`font_id`] gives
//! the `FontId` to draw with ([`font_at`] at a [`Step`] of the theme's
//! size scale). A variable face (a `wght` axis) serves
//! every weight its axis covers, set on the face (`FontTweak::coords`);
//! a static face serves its own weight, and a weight a family lacks takes
//! its nearest one, said in the report. egui's `Proportional` and `Monospace` families are
//! the regular weight of `ui` and `mono`, so egui's own widgets follow the
//! theme too.
//!
//! [`install`] runs with the theme (`theme::install_tokens`); it replaces
//! egui's font definitions only when the families or the registered faces
//! changed (one slow frame: egui rebuilds its atlas).
//!
//! Any family can also be drawn by name, outside the theme's roles, as a
//! [`specimen`] (a picker drawing each name in its own face, a page
//! comparing fonts); [`families`] lists the families there are, with
//! their [`Origin`].

use std::sync::Arc;

use egui::text::{LayoutJob, LayoutSection, TextFormat};
use egui::{Color32, Context, FontData, FontDefinitions, FontFamily, FontId, Galley};

use crate::state::Slot;

/// Which text a family is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    /// Interface text (`font.family.ui`).
    Ui,
    /// Values, code, logs (`font.family.mono`).
    Mono,
}

impl Role {
    pub const ALL: [Role; 2] = [Role::Ui, Role::Mono];

    fn key(self) -> &'static str {
        match self {
            Role::Ui => "ui",
            Role::Mono => "mono",
        }
    }

    fn token(self) -> &'static str {
        match self {
            Role::Ui => "font.family.ui",
            Role::Mono => "font.family.mono",
        }
    }
}

/// A font weight, as CSS and OpenType number them (100 to 900).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct Weight(pub u16);

impl Weight {
    pub const REGULAR: Weight = Weight(400);
    pub const MEDIUM: Weight = Weight(500);
    pub const SEMIBOLD: Weight = Weight(600);
    pub const BOLD: Weight = Weight(700);

    /// The weight of [`WEIGHTS`] nearest to this one.
    pub fn nearest_installed(self) -> Weight {
        nearest(self, WEIGHTS.iter().copied()).unwrap_or(Weight::REGULAR)
    }
}

/// The weights each family is installed at.
pub const WEIGHTS: [Weight; 4] = [
    Weight::REGULAR,
    Weight::MEDIUM,
    Weight::SEMIBOLD,
    Weight::BOLD,
];

/// The egui family of a role at a weight (`pop.ui.700`); a weight not in
/// [`WEIGHTS`] gives the nearest installed one.
pub fn family(role: Role, weight: Weight) -> FontFamily {
    let w = weight.nearest_installed();
    FontFamily::Name(format!("pop.{}.{}", role.key(), w.0).into())
}

/// The font to draw a role's text at a weight and a size (points).
pub fn font_id(role: Role, weight: Weight, size: f32) -> FontId {
    FontId::new(size, family(role, weight))
}

/// A step of the theme's size scale (`font.size.xs` to `font.size.xl`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Step {
    Xs,
    S,
    M,
    L,
    Xl,
}

impl Step {
    pub const ALL: [Step; 5] = [Step::Xs, Step::S, Step::M, Step::L, Step::Xl];

    /// Its name in the tokens (`xs`, `s`, `m`, `l`, `xl`).
    pub fn name(self) -> &'static str {
        match self {
            Step::Xs => "xs",
            Step::S => "s",
            Step::M => "m",
            Step::L => "l",
            Step::Xl => "xl",
        }
    }

    /// Its token (`font.size.m`).
    pub fn token(self) -> &'static str {
        match self {
            Step::Xs => "font.size.xs",
            Step::S => "font.size.s",
            Step::M => "font.size.m",
            Step::L => "font.size.l",
            Step::Xl => "font.size.xl",
        }
    }
}

/// The font to draw a role's text at a weight and a step of the theme's
/// size scale ([`font_id`] at the step's size).
pub fn font_at(tokens: &crate::theme::Tokens, role: Role, weight: Weight, step: Step) -> FontId {
    font_id(role, weight, tokens.font.scale.get(step))
}

/// Where a face comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    /// egui's own fonts (`Ubuntu`, `Hack`).
    Bundled,
    /// A face the application registered ([`register`]).
    Registered,
    /// A font file the user added ([`register_files`]).
    File,
    /// The system's fonts (feature `system-fonts`).
    System,
}

impl Origin {
    /// Its name in reports: `bundled`, `registered`, `file`, `system`.
    pub fn name(self) -> &'static str {
        match self {
            Origin::Bundled => "bundled",
            Origin::Registered => "registered",
            Origin::File => "file",
            Origin::System => "system",
        }
    }
}

/// How values line their digits up (`font.numeric`): egui applies no
/// OpenType feature, so tabular figures (`tnum`) cannot be asked of a
/// face whose default digits are proportional.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Numeric {
    /// In the monospace family (`font.family.mono`), its size matched to
    /// the interface text's: the default.
    #[default]
    Mono,
    /// In the interface family: its own digits when they are tabular,
    /// else each digit laid in a cell as wide as the widest digit.
    Ui,
}

impl Numeric {
    /// From the token's value: `mono` or `ui`.
    pub fn parse(s: &str) -> Option<Numeric> {
        match s.trim() {
            "mono" => Some(Numeric::Mono),
            "ui" => Some(Numeric::Ui),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Numeric::Mono => "mono",
            Numeric::Ui => "ui",
        }
    }
}

/// How a value is drawn at a size: the font, and whether its digits are
/// laid in cells (a proportional face asked for numbers).
#[derive(Clone, Debug, PartialEq)]
pub struct NumberFont {
    pub font: FontId,
    /// Each digit in a cell as wide as the widest, right-aligned in it.
    pub cells: bool,
}

/// How values are drawn at `size` (the size of the text around them),
/// from `font.numeric` and the installed faces: the monospace family at
/// the size matching the interface text's x-height, or the interface
/// family (its digits in cells when they are not tabular).
pub fn number_font(ctx: &Context, size: f32) -> NumberFont {
    let t = crate::theme::Tokens::of(ctx);
    match t.font.numeric {
        Numeric::Mono => {
            let ratio = if t.font.body > 0.0 {
                t.font.mono / t.font.body
            } else {
                1.0
            };
            NumberFont {
                font: FontId::new((size * ratio * 2.0).round() / 2.0, FontFamily::Monospace),
                cells: false,
            }
        }
        Numeric::Ui => NumberFont {
            font: FontId::new(size, FontFamily::Proportional),
            cells: !report(ctx).is_some_and(|r| r.tabular_digits(Role::Ui)),
        },
    }
}

/// A value's text laid out so its digits line up with the values above
/// and below it ([`number_font`]): what sliders, readouts and tables of
/// numbers draw. In cells, each digit is its own section, right-aligned
/// in a cell as wide as the face's widest digit.
pub fn number_job(ctx: &Context, text: &str, size: f32, color: Color32) -> LayoutJob {
    let NumberFont { font, cells } = number_font(ctx, size);
    if !cells {
        return LayoutJob::simple_singleline(text.to_string(), font, color);
    }
    let widths: Vec<f32> =
        ctx.fonts_mut(|f| ('0'..='9').map(|c| f.glyph_width(&font, c)).collect());
    let cell = widths.iter().copied().fold(0.0_f32, f32::max);
    digit_cells(text, &font, color, |c| {
        c.to_digit(10).map(|d| cell - widths[d as usize])
    })
}

/// The job of [`number_job`] in cells: `pad` gives the space before a
/// digit (`None` for any other char).
fn digit_cells(
    text: &str,
    font: &FontId,
    color: Color32,
    pad: impl Fn(char) -> Option<f32>,
) -> LayoutJob {
    let mut job = LayoutJob {
        text: text.to_string(),
        break_on_newline: false,
        ..LayoutJob::default()
    };
    let format = TextFormat::simple(font.clone(), color);
    let mut run_start: Option<usize> = None;
    for (at, c) in text.char_indices() {
        match pad(c) {
            Some(leading) => {
                if let Some(s) = run_start.take() {
                    job.sections.push(LayoutSection {
                        leading_space: 0.0,
                        byte_range: span(s, at),
                        format: format.clone(),
                    });
                }
                job.sections.push(LayoutSection {
                    leading_space: leading.max(0.0),
                    byte_range: span(at, at + c.len_utf8()),
                    format: format.clone(),
                });
            }
            None => {
                run_start.get_or_insert(at);
            }
        }
    }
    if let Some(s) = run_start {
        job.sections.push(LayoutSection {
            leading_space: 0.0,
            byte_range: span(s, text.len()),
            format,
        });
    }
    job
}

fn span(from: usize, to: usize) -> std::ops::Range<egui::text::ByteIndex> {
    egui::text::ByteIndex(from)..egui::text::ByteIndex(to)
}

/// [`number_job`] laid out.
pub fn number_galley(ctx: &Context, text: &str, size: f32, color: Color32) -> Arc<Galley> {
    let job = number_job(ctx, text, size, color);
    ctx.fonts_mut(|f| f.layout_job(job))
}

/// A font face the application gives: its family name (as a theme names
/// it), its weight and the bytes of a `.ttf` or `.otf` file.
#[derive(Clone)]
pub struct Face {
    pub family: String,
    pub weight: Weight,
    data: Arc<FontData>,
    origin: Origin,
}

impl Face {
    /// A face from bytes the binary embeds.
    pub fn from_static(family: &str, weight: Weight, bytes: &'static [u8]) -> Self {
        Self {
            family: family.into(),
            weight,
            data: Arc::new(FontData::from_static(bytes)),
            origin: Origin::Registered,
        }
    }

    /// A face from bytes read at run time.
    pub fn from_owned(family: &str, weight: Weight, bytes: Vec<u8>) -> Self {
        Self {
            family: family.into(),
            weight,
            data: Arc::new(FontData::from_owned(bytes)),
            origin: Origin::Registered,
        }
    }

    /// Where it comes from: [`Origin::Registered`], or [`Origin::File`]
    /// for a user's font file.
    pub fn origin(&self) -> Origin {
        self.origin
    }

    /// Its file's size, in bytes.
    pub fn len(&self) -> usize {
        self.data.font.len()
    }

    /// Whether its file is empty (it never draws).
    pub fn is_empty(&self) -> bool {
        self.data.font.is_empty()
    }

    /// Whether its digits `0` to `9` share one advance (tabular figures
    /// by default); `false` when it cannot be read.
    pub fn tabular_digits(&self) -> bool {
        tabular_digits(&self.data)
    }

    /// Whether every glyph has the same advance (a monospace face, as
    /// far as `i`, `W`, `m` and `0` tell).
    pub fn monospace(&self) -> bool {
        monospace(&self.data)
    }
}

/// The advances of chars in a face, in font units; `None` when one is
/// missing or the face cannot be read.
fn advances(data: &FontData, chars: &[char]) -> Option<Vec<f32>> {
    use skrifa::MetadataProvider as _;
    let font = skrifa::FontRef::from_index(data.font.as_ref(), data.index).ok()?;
    let charmap = font.charmap();
    let metrics = font.glyph_metrics(
        skrifa::instance::Size::unscaled(),
        skrifa::instance::LocationRef::default(),
    );
    chars
        .iter()
        .map(|c| charmap.map(*c).and_then(|g| metrics.advance_width(g)))
        .collect()
}

fn same_advance(data: &FontData, chars: &[char]) -> bool {
    advances(data, chars).is_some_and(|a| a.windows(2).all(|w| (w[0] - w[1]).abs() < 0.5))
}

/// Whether a face's default digits are tabular (one advance for `0` to
/// `9`): egui applies no OpenType feature, so `tnum` cannot be asked.
fn tabular_digits(data: &FontData) -> bool {
    same_advance(data, &['0', '1', '2', '3', '4', '5', '6', '7', '8', '9'])
}

fn monospace(data: &FontData) -> bool {
    same_advance(data, &['i', 'W', 'm', '0', '.'])
}

impl std::fmt::Debug for Face {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Face")
            .field("family", &self.family)
            .field("weight", &self.weight)
            .finish_non_exhaustive()
    }
}

/// A font file a user added: where it is, and the family and weight
/// its tables give. What settings keep (paths, not bytes): the file is
/// read again when registered ([`register_files`]).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FontFile {
    pub path: std::path::PathBuf,
    /// The family a theme names it by (the file's typographic family,
    /// else its family name).
    pub family: String,
    pub weight: Weight,
}

impl FontFile {
    /// Reads a font file (`.ttf`, `.otf`, the first face of a `.ttc`):
    /// checks it parses, and reads its family and weight. The bytes are
    /// not kept.
    pub fn read(path: &std::path::Path) -> Result<FontFile, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let (family, weight) = inspect(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(FontFile {
            path: path.to_path_buf(),
            family,
            weight,
        })
    }
}

/// A font's family and weight, from its `name` and `OS/2` tables; an
/// error when it does not parse as a font.
fn inspect(bytes: &[u8]) -> Result<(String, Weight), String> {
    use skrifa::MetadataProvider as _;
    use skrifa::string::StringId;
    let font = skrifa::FontRef::from_index(bytes, 0).map_err(|e| format!("not a font ({e})"))?;
    let name = |id| {
        font.localized_strings(id)
            .english_or_first()
            .map(|s| s.chars().collect::<String>())
            .filter(|s| !s.trim().is_empty())
    };
    let family = name(StringId::TYPOGRAPHIC_FAMILY_NAME)
        .or_else(|| name(StringId::FAMILY_NAME))
        .ok_or("a font without a family name")?;
    let weight = font.attributes().weight.value().round().clamp(1.0, 1000.0) as u16;
    Ok((family, Weight(weight)))
}

/// Registers the faces of font files a user added (paths, as settings
/// keep them), installed with the next theme install or [`install`]:
/// each file is read again, checked and registered under its family
/// and weight. A file that cannot be read or parsed is reported, and
/// the face registered before for it, if any, stays.
pub fn register_files(ctx: &Context, files: &[FontFile]) -> Vec<String> {
    let mut problems = Vec::new();
    for f in files {
        match std::fs::read(&f.path) {
            Ok(bytes) => match inspect(&bytes) {
                Ok(_) => register(
                    ctx,
                    Face {
                        origin: Origin::File,
                        ..Face::from_owned(&f.family, f.weight, bytes)
                    },
                ),
                Err(e) => problems.push(format!("{}: {e}", f.path.display())),
            },
            Err(e) => problems.push(format!("{}: {e}", f.path.display())),
        }
    }
    for p in &problems {
        log::warn!(target: "egui_pop", "fonts: {p}");
    }
    problems
}

/// Installs the families again with the faces registered since (after
/// [`register_files`]), keeping the families installed; the atlas is
/// rebuilt once, from the next pass. Nothing happens when nothing
/// changed.
pub fn reinstall(ctx: &Context) -> Option<Arc<Report>> {
    let (key, _) = INSTALLED.get(ctx)?;
    Some(install(ctx, &key.ui, &key.mono))
}

/// The egui family drawing a family by its name at a weight, outside
/// the theme's roles: a face registered, egui's or the system's, egui's
/// bundled fonts behind it. `None` until it is installed: the first ask
/// installs the fonts again (one slow frame, from the next pass) and
/// asks for a repaint; draw with another font meanwhile. A name found
/// nowhere draws egui's bundled fonts, and is said in the [`Report`].
pub fn specimen(ctx: &Context, name: &str, weight: Weight) -> Option<FontFamily> {
    let installed = INSTALLED
        .get(ctx)
        .is_some_and(|(k, _)| k.specimens.iter().any(|s| s.eq_ignore_ascii_case(name)));
    if installed {
        return Some(specimen_family(name, weight));
    }
    SPECIMENS.update(ctx, |s| {
        if !s.iter().any(|x| x.eq_ignore_ascii_case(name)) {
            s.push(name.to_string());
        }
    });
    reinstall(ctx);
    ctx.request_repaint();
    None
}

fn specimen_family(name: &str, weight: Weight) -> FontFamily {
    FontFamily::Name(
        format!(
            "pop.specimen.{}.{}",
            name.to_lowercase(),
            weight.nearest_installed().0
        )
        .into(),
    )
}

/// A family there is to draw with: its name (as a theme names it),
/// where its faces come from and whether it is monospace.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct FamilyInfo {
    pub name: String,
    pub origin: Origin,
    pub monospace: bool,
}

/// The families a theme or a user may name: egui's bundled ones, the
/// faces registered (the application's, then the user's files), then
/// the system's (feature `system-fonts`), each once, the first origin
/// found kept.
pub fn families(ctx: &Context) -> Vec<FamilyInfo> {
    let mut out: Vec<FamilyInfo> = Vec::new();
    let mut push = |info: FamilyInfo| {
        if !out.iter().any(|f| f.name.eq_ignore_ascii_case(&info.name)) {
            out.push(info);
        }
    };
    for (name, monospace) in [("Ubuntu", false), ("Hack", true)] {
        push(FamilyInfo {
            name: name.into(),
            origin: Origin::Bundled,
            monospace,
        });
    }
    let registered = REGISTERED.get_or_default(ctx);
    for origin in [Origin::Registered, Origin::File] {
        for f in registered.faces.iter().filter(|f| f.origin == origin) {
            push(FamilyInfo {
                name: f.family.clone(),
                origin,
                monospace: f.monospace(),
            });
        }
    }
    for (name, monospace) in System::get().families() {
        push(FamilyInfo {
            name: name.clone(),
            origin: Origin::System,
            monospace: *monospace,
        });
    }
    out
}

/// The faces registered, in order: the application's and the user's
/// files.
pub fn registered(ctx: &Context) -> Vec<Face> {
    REGISTERED.get_or_default(ctx).faces
}

/// The families installed last: `font.family.ui` and `font.family.mono`
/// as asked.
pub fn installed_families(ctx: &Context) -> Option<(Vec<String>, Vec<String>)> {
    INSTALLED.get(ctx).map(|(k, _)| (k.ui, k.mono))
}

/// What [`install`] did: the face each family starts with, and what it
/// could not do.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    /// Per egui family (`pop.ui.700`): the face drawn first, as
    /// "Family weight (source)", or `None` for egui's bundled fonts.
    pub primary: Vec<(String, Option<String>)>,
    /// Per egui family, the faces it falls back through, in order (keys
    /// of egui's font definitions).
    pub chains: Vec<(String, Vec<String>)>,
    /// Per role (`ui`, `mono`): whether the face drawing its regular
    /// weight has tabular digits by default.
    pub tabular: Vec<(String, bool)>,
    /// The names drawn as specimens ([`specimen`]).
    pub specimens: Vec<String>,
    pub problems: Vec<String>,
}

impl Report {
    /// Whether the face drawing a role's regular text has tabular
    /// digits by default (one advance for `0` to `9`).
    pub fn tabular_digits(&self, role: Role) -> bool {
        self.tabular.iter().any(|(r, t)| r == role.key() && *t)
    }
}

#[derive(Clone, Default)]
struct Registered {
    faces: Vec<Face>,
    generation: u64,
}

/// The faces registered for the next install.
const REGISTERED: Slot<Registered> = Slot::opaque("core.fonts.registered");
/// The names asked as specimens.
const SPECIMENS: Slot<Vec<String>> = Slot::opaque("core.fonts.specimens");
/// What the last install was asked, and its report.
const INSTALLED: Slot<(Key, Arc<Report>)> = Slot::opaque("core.fonts.installed");

/// Registers a face for the families a theme names; installed with the
/// next theme install. A face of the same family and weight is replaced.
pub fn register(ctx: &Context, face: Face) {
    REGISTERED.update(ctx, |r| {
        r.faces
            .retain(|f| !(f.family.eq_ignore_ascii_case(&face.family) && f.weight == face.weight));
        r.faces.push(face);
        r.generation += 1;
    });
}

/// The last install's report.
pub fn report(ctx: &Context) -> Option<Arc<Report>> {
    INSTALLED.get(ctx).map(|(_, r)| r)
}

#[derive(Clone, PartialEq)]
struct Key {
    ui: Vec<String>,
    mono: Vec<String>,
    specimens: Vec<String>,
    generation: u64,
}

const GENERIC: [&str; 5] = [
    "sans-serif",
    "serif",
    "monospace",
    "system-ui",
    "ui-monospace",
];

fn is_generic(name: &str) -> bool {
    GENERIC.iter().any(|g| g.eq_ignore_ascii_case(name))
}

/// Installs the families named by the theme's tokens, with the
/// registered faces, unless they are already installed. In use from the
/// next pass. The icon font (`icon::install`) is kept.
pub fn install(ctx: &Context, ui: &[String], mono: &[String]) -> Arc<Report> {
    let registered = REGISTERED.get_or_default(ctx);
    let specimens = SPECIMENS.get_or_default(ctx);
    let key = Key {
        ui: ui.to_vec(),
        mono: mono.to_vec(),
        specimens: specimens.clone(),
        generation: registered.generation,
    };
    if let Some((k, report)) = INSTALLED.get(ctx)
        && k == key
    {
        return report;
    }
    let (mut defs, mut report) = definitions(&registered.faces, ui, mono, &System::get());
    add_specimens(
        &mut defs,
        &mut report,
        &registered.faces,
        &specimens,
        &System::get(),
    );
    for p in &report.problems {
        log::warn!(target: "egui_pop", "fonts: {p}");
    }
    ctx.set_fonts(defs);
    let report = Arc::new(report);
    INSTALLED.set(ctx, (key, report.clone()));
    report
}

/// A face found for a name at a weight: its key in the definitions, its
/// data, the weight it has and where it came from.
struct Found {
    key: String,
    data: Arc<FontData>,
    weight: Weight,
    source: &'static str,
}

fn find(
    name: &str,
    weight: Weight,
    faces: &[Face],
    bundled: &std::collections::BTreeMap<String, Arc<FontData>>,
    system: &System,
) -> Option<Found> {
    let own: Vec<&Face> = faces
        .iter()
        .filter(|f| f.family.eq_ignore_ascii_case(name))
        .collect();
    if let Some(w) = nearest(weight, own.iter().map(|f| f.weight)) {
        let face = own.iter().find(|f| f.weight == w)?;
        return Some(on_axis(
            Found {
                key: format!("pop:{}:{}", face.family, w.0),
                data: face.data.clone(),
                weight: w,
                source: face.origin.name(),
            },
            weight,
        ));
    }
    // egui's faces have one weight each: the others draw it.
    let lower = name.to_ascii_lowercase();
    if let Some((key, data)) = bundled.iter().find(|(k, _)| {
        let k = k.to_ascii_lowercase();
        k == lower
            || k.strip_prefix(&lower)
                .is_some_and(|rest| rest.starts_with('-'))
    }) {
        return Some(Found {
            key: key.clone(),
            data: data.clone(),
            weight,
            source: "bundled",
        });
    }
    system.find(name, weight).map(|f| on_axis(f, weight))
}

/// A face drawn at `weight` through its `wght` axis when it has one that
/// covers it and is not already at that weight: the same bytes, with the
/// axis set on the face, under a key of their own. Owned bytes are
/// copied per weight (`FontData` holds them in a `Cow`), at install only.
fn on_axis(found: Found, weight: Weight) -> Found {
    if found.weight == weight {
        return found;
    }
    let target = f32::from(weight.0);
    let covers = found
        .data
        .variation_axes()
        .iter()
        .any(|a| a.tag.to_be_bytes() == *b"wght" && a.range.contains(target));
    if !covers {
        return found;
    }
    let mut data = (*found.data).clone();
    data.tweak.coords.push(b"wght", target);
    Found {
        key: format!("{}@wght{}", found.key, weight.0),
        data: Arc::new(data),
        weight,
        source: found.source,
    }
}

fn nearest(weight: Weight, among: impl Iterator<Item = Weight>) -> Option<Weight> {
    // CSS's rule, simplified: the closest, the heavier one on a tie.
    among.min_by_key(|w| (w.0.abs_diff(weight.0), std::cmp::Reverse(w.0)))
}

fn definitions(
    faces: &[Face],
    ui: &[String],
    mono: &[String],
    system: &System,
) -> (FontDefinitions, Report) {
    let mut defs = FontDefinitions::default();
    let bundled_faces = defs.font_data.clone();
    crate::icon::insert(&mut defs);
    let bundled = |f: &FontFamily| defs.families.get(f).cloned().unwrap_or_default();
    let bundled_ui = bundled(&FontFamily::Proportional);
    let bundled_mono = bundled(&FontFamily::Monospace);
    let mut report = Report::default();
    for role in Role::ALL {
        let (names, tail) = match role {
            Role::Ui => (ui, &bundled_ui),
            Role::Mono => (mono, &bundled_mono),
        };
        let mut missing: Vec<&str> = Vec::new();
        for weight in WEIGHTS {
            let mut chain: Vec<String> = Vec::new();
            let mut primary = None;
            for name in names.iter().filter(|n| !is_generic(n)) {
                let Some(found) = find(name, weight, faces, &bundled_faces, system) else {
                    if weight == Weight::REGULAR {
                        missing.push(name);
                    }
                    continue;
                };
                if primary.is_none() {
                    primary = Some(format!("{name} {} ({})", found.weight.0, found.source));
                    if found.weight != weight {
                        report.problems.push(format!(
                            "{}: {name} has no weight {}, {} is drawn",
                            role.token(),
                            weight.0,
                            found.weight.0
                        ));
                    }
                }
                defs.font_data
                    .entry(found.key.clone())
                    .or_insert(found.data);
                if !chain.contains(&found.key) {
                    chain.push(found.key);
                }
            }
            if weight == Weight::REGULAR {
                // The face drawing the digits: the first of the chain.
                let first = chain.first().or(tail.first());
                let tabular = first
                    .and_then(|k| defs.font_data.get(k))
                    .is_some_and(|d| tabular_digits(d));
                report.tabular.push((role.key().to_string(), tabular));
            }
            chain.extend(tail.iter().cloned());
            let fam = family(role, weight);
            if let FontFamily::Name(n) = &fam {
                report.primary.push((n.to_string(), primary));
                report.chains.push((n.to_string(), chain.clone()));
            }
            if weight == Weight::REGULAR {
                let egui_family = match role {
                    Role::Ui => FontFamily::Proportional,
                    Role::Mono => FontFamily::Monospace,
                };
                defs.families.insert(egui_family, chain.clone());
            }
            defs.families.insert(fam, chain);
        }
        for name in missing {
            report.problems.push(format!(
                "{}: no font named {name} (registered{})",
                role.token(),
                if cfg!(feature = "system-fonts") {
                    " or on the system"
                } else {
                    "; the system's fonts need the system-fonts feature"
                }
            ));
        }
    }
    (defs, report)
}

/// Adds a family per specimen name and weight: the face found for the
/// name, egui's bundled proportional fonts behind it.
fn add_specimens(
    defs: &mut FontDefinitions,
    report: &mut Report,
    faces: &[Face],
    names: &[String],
    system: &System,
) {
    let bundled_faces = FontDefinitions::default().font_data;
    let tail = defs
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    // egui's own tail, not the theme's faces in front of it.
    let egui_tail: Vec<String> = FontDefinitions::default()
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let tail: Vec<String> = tail.into_iter().filter(|k| egui_tail.contains(k)).collect();
    for name in names {
        let mut found_any = false;
        for weight in WEIGHTS {
            let mut chain = Vec::new();
            if !is_generic(name)
                && let Some(found) = find(name, weight, faces, &bundled_faces, system)
            {
                found_any = true;
                defs.font_data
                    .entry(found.key.clone())
                    .or_insert(found.data);
                chain.push(found.key);
            }
            chain.extend(tail.iter().cloned());
            let fam = specimen_family(name, weight);
            if let FontFamily::Name(n) = &fam {
                report.chains.push((n.to_string(), chain.clone()));
            }
            defs.families.insert(fam, chain);
        }
        if !found_any && !is_generic(name) {
            report
                .problems
                .push(format!("specimen: no font named {name}"));
        }
        report.specimens.push(name.clone());
    }
}

/// The system's fonts, read once per process.
struct System {
    #[cfg(feature = "system-fonts")]
    db: Option<&'static fontdb::Database>,
}

impl System {
    #[cfg(feature = "system-fonts")]
    fn get() -> Self {
        static DB: std::sync::OnceLock<fontdb::Database> = std::sync::OnceLock::new();
        let db = DB.get_or_init(|| {
            let mut db = fontdb::Database::new();
            db.load_system_fonts();
            db
        });
        Self { db: Some(db) }
    }

    #[cfg(not(feature = "system-fonts"))]
    fn get() -> Self {
        Self {}
    }

    /// No system fonts (tests: what is drawn never depends on the
    /// machine).
    #[cfg(test)]
    fn none() -> Self {
        Self {
            #[cfg(feature = "system-fonts")]
            db: None,
        }
    }

    #[cfg(feature = "system-fonts")]
    fn find(&self, name: &str, weight: Weight) -> Option<Found> {
        let db = self.db?;
        let id = db.query(&fontdb::Query {
            families: &[fontdb::Family::Name(name)],
            weight: fontdb::Weight(weight.0),
            ..fontdb::Query::default()
        })?;
        let info = db.face(id)?;
        let w = Weight(info.weight.0);
        let data = db.with_face_data(id, |bytes, index| {
            let mut data = FontData::from_owned(bytes.to_vec());
            data.index = index;
            data
        })?;
        Some(Found {
            key: format!("pop:{name}:{}:system", w.0),
            data: Arc::new(data),
            weight: w,
            source: "system",
        })
    }

    #[cfg(not(feature = "system-fonts"))]
    fn find(&self, _name: &str, _weight: Weight) -> Option<Found> {
        None
    }

    /// The system's families, each once, sorted, with whether it is
    /// monospace.
    #[cfg(feature = "system-fonts")]
    fn families(&self) -> &'static [(String, bool)] {
        static LIST: std::sync::OnceLock<Vec<(String, bool)>> = std::sync::OnceLock::new();
        let Some(db) = self.db else {
            return &[];
        };
        LIST.get_or_init(|| {
            let mut out: Vec<(String, bool)> = Vec::new();
            for face in db.faces() {
                let Some((name, _)) = face.families.first() else {
                    continue;
                };
                if !out.iter().any(|(n, _)| n == name) {
                    out.push((name.clone(), face.monospaced));
                }
            }
            out.sort_by_key(|(n, _)| n.to_lowercase());
            out
        })
    }

    #[cfg(not(feature = "system-fonts"))]
    fn families(&self) -> &'static [(String, bool)] {
        &[]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static FAKE: &[u8] = b"not a real font, never rasterised here";

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_step_of_the_scale_gives_its_size_and_egui_styles_sit_on_steps() {
        let t = crate::theme::Tokens::default();
        let sizes: Vec<f32> = Step::ALL.iter().map(|s| t.font.scale.get(*s)).collect();
        assert!(sizes.windows(2).all(|w| w[0] < w[1]), "{sizes:?}");
        let f = font_at(&t, Role::Mono, Weight::BOLD, Step::L);
        assert_eq!(f.size, t.font.scale.l);
        assert_eq!(f.family, family(Role::Mono, Weight::BOLD));
        assert_eq!(Step::Xl.token(), format!("font.size.{}", Step::Xl.name()));
        // egui's text styles are steps of the scale.
        assert_eq!(t.font.small, t.font.scale.xs);
        assert_eq!(t.font.body, t.font.scale.m);
        assert_eq!(t.font.button, t.font.scale.m);
        assert_eq!(t.font.heading, t.font.scale.xl);
        assert_eq!(t.font.mono, t.font.scale.s);
        let style = crate::theme::style(&t, &egui::Style::default());
        assert_eq!(
            style.text_styles[&egui::TextStyle::Button].size,
            t.font.button
        );
    }

    #[test]
    fn registered_faces_lead_their_families_at_each_weight() {
        let faces = vec![
            Face::from_static("Inter", Weight::REGULAR, FAKE),
            Face::from_static("Inter", Weight::BOLD, FAKE),
        ];
        let (defs, report) = definitions(
            &faces,
            &names(&["Inter", "sans-serif"]),
            &names(&["monospace"]),
            &System::none(),
        );
        let bold = &defs.families[&family(Role::Ui, Weight::BOLD)];
        assert_eq!(bold[0], "pop:Inter:700");
        let regular = &defs.families[&FontFamily::Proportional];
        assert_eq!(regular[0], "pop:Inter:400");
        // Semibold takes the nearest, the heavier on a tie: bold.
        let semi = &defs.families[&family(Role::Ui, Weight::SEMIBOLD)];
        assert_eq!(semi[0], "pop:Inter:700");
        assert!(
            report
                .problems
                .iter()
                .any(|p| p.contains("no weight 600, 700")),
            "{report:?}"
        );
        // A generic name is the bundled fonts: no problem said.
        assert!(!report.problems.iter().any(|p| p.contains("monospace")));
    }

    static INTER_VAR: &[u8] = include_bytes!("../tests/fonts/InterVariable-subset.ttf");

    #[test]
    fn a_variable_face_serves_every_weight_through_its_axis() {
        let faces = vec![Face::from_static("Inter", Weight::REGULAR, INTER_VAR)];
        let (defs, report) = definitions(&faces, &names(&["Inter"]), &[], &System::none());
        assert!(report.problems.is_empty(), "{report:?}");
        assert_eq!(defs.families[&FontFamily::Proportional][0], "pop:Inter:400");
        let bold = &defs.families[&family(Role::Ui, Weight::BOLD)][0];
        assert_eq!(bold, "pop:Inter:400@wght700");
        assert!(format!("{:?}", defs.font_data[bold].tweak.coords).contains("700"));
    }

    #[test]
    fn a_variable_face_draws_heavier_at_a_heavier_weight() {
        let ctx = Context::default();
        register(&ctx, Face::from_static("Inter", Weight::REGULAR, INTER_VAR));
        install(&ctx, &names(&["Inter"]), &[]);
        // Fonts change at the next pass.
        ctx.run_ui(egui::RawInput::default(), |_| {})
            .textures_delta
            .clear();
        let width = |w: Weight| {
            ctx.fonts_mut(|f| {
                f.layout_no_wrap(
                    "Hamburgefonstiv".into(),
                    font_id(Role::Ui, w, 20.0),
                    egui::Color32::WHITE,
                )
                .size()
                .x
            })
        };
        let (regular, bold) = (width(Weight::REGULAR), width(Weight::BOLD));
        assert!(bold > regular + 1.0, "regular {regular}, bold {bold}");
    }

    #[test]
    fn a_static_face_keeps_its_weight() {
        // Hack has no axis: asked at 700, it stays itself (and says so).
        let hack = FontDefinitions::default().font_data["Hack"].clone();
        let found = on_axis(
            Found {
                key: "Hack".into(),
                data: hack,
                weight: Weight::REGULAR,
                source: "bundled",
            },
            Weight::BOLD,
        );
        assert_eq!(found.key, "Hack");
        assert_eq!(found.weight, Weight::REGULAR);
    }

    #[test]
    fn a_family_found_nowhere_is_reported_and_text_still_draws() {
        let (defs, report) = definitions(
            &[],
            &names(&["Nowhere Sans"]),
            &names(&["Hack"]),
            &System::none(),
        );
        assert!(
            report
                .problems
                .iter()
                .any(|p| p.starts_with("font.family.ui: no font named Nowhere Sans")),
            "{report:?}"
        );
        // egui's bundled fonts stay at the end of every chain.
        let bundled = FontDefinitions::default();
        assert_eq!(
            defs.families[&FontFamily::Proportional],
            bundled.families[&FontFamily::Proportional]
        );
    }

    #[test]
    fn the_built_in_themes_names_are_egui_s_bundled_faces() {
        let (defs, report) = definitions(
            &[],
            &names(&["Ubuntu", "sans-serif"]),
            &names(&["Hack", "monospace"]),
            &System::none(),
        );
        assert!(report.problems.is_empty(), "{report:?}");
        let bundled = FontDefinitions::default();
        if let Some(first) = bundled.families[&FontFamily::Proportional].first() {
            assert_eq!(&defs.families[&FontFamily::Proportional][0], first);
            assert_eq!(&defs.families[&family(Role::Ui, Weight::BOLD)][0], first);
        }
    }

    #[test]
    fn the_icon_font_is_kept() {
        let (defs, _) = definitions(&[], &[], &[], &System::none());
        assert_eq!(
            defs.families
                .contains_key(&FontFamily::Name(crate::icon::FAMILY.into())),
            cfg!(feature = "phosphor")
        );
    }

    #[test]
    fn installing_twice_the_same_families_keeps_the_report() {
        let ctx = Context::default();
        let a = install(&ctx, &names(&["Nowhere"]), &[]);
        let b = install(&ctx, &names(&["Nowhere"]), &[]);
        assert!(Arc::ptr_eq(&a, &b));
        register(&ctx, Face::from_static("Nowhere", Weight::REGULAR, FAKE));
        let c = install(&ctx, &names(&["Nowhere"]), &[]);
        assert!(!Arc::ptr_eq(&a, &c));
        assert!(c.problems.is_empty() || c.problems.iter().all(|p| !p.contains("no font named")));
    }

    #[test]
    fn a_family_is_drawn_by_name_as_a_specimen_once_installed() {
        let ctx = Context::default();
        install(&ctx, &names(&["Ubuntu"]), &names(&["Hack"]));
        register(&ctx, Face::from_static("Inter", Weight::REGULAR, INTER_VAR));
        // Asked once: installed from the next pass.
        assert_eq!(specimen(&ctx, "Inter", Weight::REGULAR), None);
        ctx.run_ui(egui::RawInput::default(), |_| {})
            .textures_delta
            .clear();
        let fam = specimen(&ctx, "inter", Weight::BOLD).expect("installed");
        assert_eq!(fam, specimen_family("Inter", Weight::BOLD));
        let said = report(&ctx).unwrap();
        assert_eq!(said.specimens, vec!["Inter".to_string()]);
        let chain = &said
            .chains
            .iter()
            .find(|(f, _)| fam == FontFamily::Name(f.as_str().into()))
            .unwrap()
            .1;
        assert_eq!(chain[0], "pop:Inter:400@wght700");
        // The theme's families are untouched.
        assert_eq!(installed_families(&ctx).unwrap().0, names(&["Ubuntu"]));
        let width = |f: FontFamily| {
            ctx.fonts_mut(|fonts| {
                fonts
                    .layout_no_wrap(
                        "Hamburgefonstiv".into(),
                        FontId::new(20.0, f),
                        egui::Color32::WHITE,
                    )
                    .size()
                    .x
            })
        };
        let regular = specimen(&ctx, "Inter", Weight::REGULAR).unwrap();
        assert!(width(fam) > width(regular) + 1.0);
        // A name found nowhere still draws, and is said.
        assert_eq!(specimen(&ctx, "Nowhere Sans", Weight::REGULAR), None);
        ctx.run_ui(egui::RawInput::default(), |_| {})
            .textures_delta
            .clear();
        assert!(specimen(&ctx, "Nowhere Sans", Weight::REGULAR).is_some());
        assert!(
            report(&ctx)
                .unwrap()
                .problems
                .iter()
                .any(|p| p == "specimen: no font named Nowhere Sans")
        );
    }

    #[test]
    fn families_list_egui_s_the_registered_and_the_user_s_with_their_origin() {
        let ctx = Context::default();
        register(&ctx, Face::from_static("Inter", Weight::REGULAR, INTER_VAR));
        let hack = FontDefinitions::default().font_data["Hack"].font.to_vec();
        register(
            &ctx,
            Face {
                origin: Origin::File,
                ..Face::from_owned("My Mono", Weight::REGULAR, hack)
            },
        );
        let list = families(&ctx);
        let find = |n: &str| list.iter().find(|f| f.name == n).cloned().unwrap();
        assert_eq!(find("Ubuntu").origin, Origin::Bundled);
        assert!(find("Hack").monospace);
        assert_eq!(find("Inter").origin, Origin::Registered);
        assert!(!find("Inter").monospace);
        assert_eq!(find("My Mono").origin, Origin::File);
        assert!(find("My Mono").monospace);
        // egui's first, each name once.
        assert_eq!(list[0].name, "Ubuntu");
        let mut names: Vec<_> = list.iter().map(|f| f.name.to_lowercase()).collect();
        names.dedup();
        assert_eq!(names.len(), list.len());
        assert_eq!(registered(&ctx).len(), 2);
    }

    #[test]
    fn the_report_says_whether_each_role_draws_tabular_digits() {
        let (_, report) = definitions(&[], &names(&["Ubuntu"]), &names(&["Hack"]), &System::none());
        // A monospace face's digits are tabular.
        assert!(report.tabular_digits(Role::Mono));
        let hack = FontDefinitions::default().font_data["Hack"].clone();
        assert!(tabular_digits(&hack));
        assert!(monospace(&hack));
        let ubuntu = FontDefinitions::default().font_data["Ubuntu-Light"].clone();
        assert!(!monospace(&ubuntu));
        assert_eq!(report.tabular_digits(Role::Ui), tabular_digits(&ubuntu));
        assert_eq!(report.chains.len(), 2 * WEIGHTS.len());
    }

    /// The x of each glyph from the right edge, per galley.
    fn from_right(g: &Galley) -> Vec<f32> {
        let row = &g.rows[0];
        let right = row.glyphs.last().map_or(0.0, |l| l.pos.x + l.advance_width);
        row.glyphs.iter().map(|gl| right - gl.pos.x).collect()
    }

    fn pass(ctx: &Context) {
        ctx.run_ui(egui::RawInput::default(), |_| {})
            .textures_delta
            .clear();
    }

    #[test]
    fn values_are_drawn_in_the_monospace_family_by_default_and_line_up() {
        let ctx = Context::default();
        let t = crate::theme::install(&ctx, &crate::theme::ThemeSpec::new("pop-dark"));
        assert_eq!(t.font.numeric, Numeric::Mono);
        assert_eq!(Numeric::parse("ui"), Some(Numeric::Ui));
        assert_eq!(Numeric::parse("tabular"), None);
        pass(&ctx);
        let f = number_font(&ctx, t.font.body);
        assert_eq!(f.font.family, FontFamily::Monospace);
        // The monospace size that matches the body's x-height.
        assert_eq!(f.font.size, t.font.mono);
        assert!(!f.cells);
        // A column of numbers, right-aligned: each digit's x from the
        // right is the same on every row.
        let rows: Vec<Vec<f32>> = ["-1234.56", "1111.11", "8888.88", "+0000.00"]
            .iter()
            .map(|n| from_right(&number_galley(&ctx, n, 13.0, Color32::WHITE)))
            .collect();
        for r in &rows[1..] {
            for (a, b) in r.iter().rev().zip(rows[0].iter().rev()) {
                assert!((a - b).abs() < 0.01, "{rows:?}");
            }
        }
    }

    #[test]
    fn a_proportional_face_s_digits_are_laid_in_cells() {
        let ctx = Context::default();
        register(&ctx, Face::from_static("Inter", Weight::REGULAR, INTER_VAR));
        let t = crate::theme::install(
            &ctx,
            &crate::theme::ThemeSpec::new("pop-dark")
                .override_token("font.numeric", serde_json::json!("ui"))
                .override_token("font.family.ui", serde_json::json!(["Inter"])),
        );
        assert_eq!(t.font.numeric, Numeric::Ui);
        pass(&ctx);
        // Inter's default digits are proportional.
        assert!(!report(&ctx).unwrap().tabular_digits(Role::Ui));
        let f = number_font(&ctx, 20.0);
        assert_eq!(f.font, FontId::proportional(20.0));
        assert!(f.cells);
        let width = |s: &str| number_galley(&ctx, s, 20.0, Color32::WHITE).size().x;
        let plain = |s: &str| {
            ctx.fonts_mut(|fonts| {
                fonts
                    .layout_no_wrap(s.into(), FontId::proportional(20.0), Color32::WHITE)
                    .size()
                    .x
            })
        };
        // Drawn plainly, ones are narrower than zeros; in cells, not.
        assert!(plain("1111") + 1.0 < plain("0000"));
        assert!((width("1111") - width("0000")).abs() < 0.01);
        assert!((width("-11.1") - width("-00.0")).abs() < 0.01);
    }

    #[test]
    fn cells_are_sections_one_per_digit() {
        let job = digit_cells(
            "-12.5 EV",
            &FontId::proportional(10.0),
            Color32::WHITE,
            |c| c.is_ascii_digit().then_some(1.0),
        );
        let parts: Vec<(&str, f32)> = job
            .sections
            .iter()
            .map(|s| {
                (
                    &job.text[s.byte_range.start.0..s.byte_range.end.0],
                    s.leading_space,
                )
            })
            .collect();
        assert_eq!(
            parts,
            vec![
                ("-", 0.0),
                ("1", 1.0),
                ("2", 1.0),
                (".", 0.0),
                ("5", 1.0),
                (" EV", 0.0)
            ]
        );
    }

    #[test]
    fn a_weight_maps_to_the_nearest_installed_family() {
        assert_eq!(
            family(Role::Ui, Weight(800)),
            family(Role::Ui, Weight::BOLD)
        );
        assert_eq!(
            family(Role::Mono, Weight(450)),
            family(Role::Mono, Weight::MEDIUM)
        );
    }

    #[test]
    fn a_font_file_is_read_by_path_and_a_broken_one_is_reported() {
        let dir = std::env::temp_dir().join(format!("egui-pop-fonts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let good = dir.join("inter.ttf");
        std::fs::write(&good, INTER_VAR).unwrap();
        let bad = dir.join("broken.ttf");
        std::fs::write(&bad, b"not a font at all").unwrap();
        let file = FontFile::read(&good).unwrap();
        assert!(file.family.starts_with("Inter"), "{file:?}");
        assert_eq!(file.weight, Weight::REGULAR);
        assert!(FontFile::read(&bad).unwrap_err().contains("not a font"));
        assert!(FontFile::read(&dir.join("missing.ttf")).is_err());

        let ctx = Context::default();
        let broken = FontFile {
            path: bad.clone(),
            ..file.clone()
        };
        let problems = register_files(&ctx, &[file.clone(), broken]);
        assert_eq!(problems.len(), 1, "{problems:?}");
        // The good one is registered: the family is found by its name.
        let report = install(&ctx, &names(&[file.family.as_str()]), &[]);
        assert!(
            report.problems.iter().all(|p| !p.contains("no font named")),
            "{:?}",
            report.problems
        );
        assert_eq!(
            installed_families(&ctx).unwrap().0,
            vec![file.family.clone()]
        );
        // A path is what is kept.
        let json = serde_json::to_value(&file).unwrap();
        assert_eq!(json["weight"], 400);
        assert!(json["path"].as_str().unwrap().ends_with("inter.ttf"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
