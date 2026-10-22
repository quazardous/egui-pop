//! Text fonts from the theme: the families named by `font.family.ui` and
//! `font.family.mono`, at several weights.
//!
//! Each name of a token's list is looked for in order:
//!
//! 1. faces the application registered ([`register`]): bytes it embeds or
//!    reads from disk;
//! 2. the system's fonts by family name (feature `system-fonts`, or the
//!    faces given by [`set_system_faces`]);
//! 3. egui's bundled faces by name (`Ubuntu` is egui's Ubuntu-Light,
//!    `Hack` its Hack): the built-in themes name them. A bundled face is
//!    reached by its name only when the system has no family of that
//!    name, as platforms resolve a font stack.
//!
//! The first found is the primary, the others its fallbacks, and egui's
//! bundled fonts always come last so text never vanishes. A generic name
//! (`sans-serif`, `serif`, `monospace`, `system-ui`) stands for those
//! bundled fonts. A name found nowhere is a problem in the [`Report`]
//! (ADR 0003), never replaced in silence; the report says where each
//! name was found ([`Report::resolved`]: `Ubuntu → bundled Ubuntu-Light`).
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
//!
//! A face is previewed before it is applied through the preview
//! families ([`set_preview`]): every install defines them, so they never
//! vanish while a preview draws (egui panics on an unknown family). They
//! draw the role's families as installed, or the list a preview gives;
//! [`with_preview`] draws a `Ui` in them, the rest of the application
//! keeping the theme's fonts.
//!
//! The system's fonts come from fontdb (feature `system-fonts`), unless
//! faces are given in their place ([`set_system_faces`]: tests, or an
//! application finding fonts its own way).

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

/// The `fonts` collector of the debug bridge (among the core ones):
/// what the theme asked, the faces registered with their origin, each
/// egui family's chain, the sizes, the atlas; and `check`, the chars of
/// a text no face of a family covers. Read after the first frame (egui
/// has no fonts before).
pub fn collector() -> crate::debug::FnCollector {
    use serde_json::{Value as Json, json};
    crate::debug::FnCollector::new(
        "fonts",
        "The fonts: the families the theme asks for and where each name was found (registered, system or bundled), the faces registered (family, weight, origin, size, tabular digits, monospace), each egui family's faces in fallback order, the specimens, the sizes of the scale and of egui's text styles, the text size, how values line up, the atlas and the problems.",
    )
    .snapshot(|ctx| {
        let t = crate::theme::Tokens::of(ctx);
        let (ui, mono) = installed_families(ctx).unwrap_or_default();
        let report = report(ctx).unwrap_or_default();
        let faces: Vec<Json> = registered(ctx)
            .iter()
            .map(|f| {
                json!({
                    "family": f.family,
                    "weight": f.weight.0,
                    "origin": f.origin,
                    "bytes": f.len(),
                    "tabular_digits": f.tabular_digits(),
                    "monospace": f.monospace(),
                })
            })
            .collect();
        let primary: std::collections::BTreeMap<&str, &Option<String>> = report
            .primary
            .iter()
            .map(|(f, p)| (f.as_str(), p))
            .collect();
        let families: Vec<Json> = report
            .chains
            .iter()
            .map(|(f, chain)| {
                json!({
                    "family": f,
                    "primary": primary.get(f.as_str()).copied().cloned().flatten(),
                    "chain": chain,
                })
            })
            .collect();
        let s = &t.font.scale;
        let ppp = ctx.pixels_per_point();
        // egui has no fonts before its first pass.
        let (atlas, fill) = if ctx.cumulative_pass_nr() == 0 {
            (None, None)
        } else {
            ctx.fonts(|f| (Some(f.font_image_size()), Some(f.font_atlas_fill_ratio())))
        };
        json!({
            "requested": { "ui": ui, "mono": mono },
            "faces": faces,
            "families": families,
            "specimens": report.specimens,
            "resolved": report
                .resolved
                .iter()
                .map(|r| json!({
                    "list": r.list,
                    "name": r.name,
                    "origin": r.origin,
                    "face": r.face,
                    "says": r.to_string(),
                }))
                .collect::<Vec<_>>(),
            "tabular_digits": {
                "ui": report.tabular_digits(Role::Ui),
                "mono": report.tabular_digits(Role::Mono),
            },
            "numeric": t.font.numeric,
            "text_size": t.resolved().spec().text_size,
            "sizes": {
                "scale": { "xs": s.xs, "s": s.s, "m": s.m, "l": s.l, "xl": s.xl },
                "text_styles": {
                    "small": t.font.small,
                    "body": t.font.body,
                    "button": t.font.button,
                    "heading": t.font.heading,
                    "monospace": t.font.mono,
                },
            },
            "pixels_per_point": ppp,
            "atlas": { "size": atlas, "fill_ratio": fill },
            "problems": report.problems,
        })
    })
    .action(
        "check",
        "The chars of `text` no face of a family covers (`family`: `ui` or `mono`, the default `ui`, or a family's name drawn as a specimen); drawn as a box.",
        false,
        |ctx, args| {
            let text = args
                .get("text")
                .and_then(Json::as_str)
                .ok_or("`text` is missing")?;
            let asked = args.get("family").and_then(Json::as_str).unwrap_or("ui");
            let family = match asked {
                "ui" => FontFamily::Proportional,
                "mono" => FontFamily::Monospace,
                name => specimen(ctx, name, Weight::REGULAR).ok_or_else(|| {
                    format!("`{name}` is being installed as a specimen: ask again after a frame")
                })?,
            };
            if ctx.cumulative_pass_nr() == 0 {
                return Err("egui has no fonts before its first frame".into());
            }
            let id = FontId::new(13.0, family);
            let mut missing: Vec<String> = Vec::new();
            ctx.fonts_mut(|f| {
                for c in text.chars().filter(|c| !c.is_whitespace() && !c.is_control()) {
                    if !f.has_glyph(&id, c) && !missing.contains(&c.to_string()) {
                        missing.push(c.to_string());
                    }
                }
            });
            Ok(json!({ "family": asked, "missing": missing }))
        },
    )
    .args(json!({ "text": "Ελληνικά 日本語", "family": "ui" }))
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
                font: FontId::new(
                    whole_pixel_advance(ctx, size * ratio),
                    FontFamily::Monospace,
                ),
                cells: false,
            }
        }
        Numeric::Ui => NumberFont {
            font: FontId::new(size, FontFamily::Proportional),
            cells: !report(ctx).is_some_and(|r| r.tabular_digits(Role::Ui)),
        },
    }
}

/// The largest size, not above `size`, at which the monospace family's
/// advance is a whole number of physical pixels at the context's
/// `pixels_per_point`: with a fractional advance, glyphs snap to pixels
/// differently from one row to the next and a column drifts by a pixel.
/// Before egui's first pass (no fonts yet), `size` to half a point.
fn whole_pixel_advance(ctx: &Context, size: f32) -> f32 {
    if ctx.cumulative_pass_nr() == 0 || size <= 0.0 {
        return (size * 2.0).round() / 2.0;
    }
    let ppp = ctx.pixels_per_point();
    let advance =
        ctx.fonts_mut(|f| f.glyph_width(&FontId::new(size, FontFamily::Monospace), '0')) * ppp;
    if advance <= 0.0 {
        return size;
    }
    // A hair over, so an advance already whole stays.
    let whole = (advance + 1e-3).floor().max(1.0);
    size * whole / advance
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
        // Installed, but egui takes new fonts at its next pass: asked
        // again in the same frame, it is not drawn yet.
        let family = specimen_family(name, weight);
        if bound(ctx, &family) {
            return Some(family);
        }
        ctx.request_repaint();
        return None;
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

/// Previews a role's families before they are applied: the preview
/// families ([`preview_family`], drawn by [`with_preview`]) draw the
/// faces found for `names` in priority order, the role's families as
/// installed behind them (so a name found nowhere draws the role's
/// families, never nothing), or the role's families alone with `None`. The theme's own families are
/// untouched. Installed again at once when it changes (one slow frame,
/// from the next pass); every install defines the preview families, so
/// a theme switched while a preview is shown keeps it.
pub fn set_preview(ctx: &Context, role: Role, names: Option<Vec<String>>) {
    let at = role_index(role);
    let mut previews = PREVIEW.get_or_default(ctx);
    if previews[at] == names {
        return;
    }
    previews[at] = names;
    PREVIEW.set(ctx, previews);
    reinstall(ctx);
    ctx.request_repaint();
}

/// What a role's preview draws ([`set_preview`]): `None` for the role's
/// families as installed.
pub fn preview(ctx: &Context, role: Role) -> Option<Vec<String>> {
    PREVIEW.get_or_default(ctx)[role_index(role)].clone()
}

fn role_index(role: Role) -> usize {
    match role {
        Role::Ui => 0,
        Role::Mono => 1,
    }
}

/// The egui family drawing a role's preview at a weight
/// ([`set_preview`]); `None` until egui's fonts in use have it (before
/// the first install's pass).
pub fn preview_family(ctx: &Context, role: Role, weight: Weight) -> Option<FontFamily> {
    let family = preview_family_name(role, weight);
    bound(ctx, &family).then_some(family)
}

/// Draws `add` in a child `Ui` in the preview families ([`set_preview`]):
/// its text styles draw the interface preview (the monospace style, the
/// monospace one), and inside it [`shown_font_id`] and the text roles of
/// [`crate::ui`] give the preview families too. What draws with an
/// explicit `FontId` keeps the theme's fonts. Scopes nest; the previous
/// state comes back when `add` returns, even when it panics.
pub fn with_preview<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    struct Restore {
        ctx: Context,
        previous: Option<bool>,
    }
    impl Drop for Restore {
        fn drop(&mut self) {
            match self.previous.take() {
                Some(p) => PREVIEWING.set(&self.ctx, p),
                None => {
                    PREVIEWING.remove(&self.ctx);
                }
            }
        }
    }
    let ctx = ui.ctx().clone();
    let previous = PREVIEWING.get(&ctx);
    PREVIEWING.set(&ctx, true);
    let _restore = Restore {
        ctx: ctx.clone(),
        previous,
    };
    ui.scope(|ui| {
        let ui_family = preview_family(&ctx, Role::Ui, Weight::REGULAR);
        let mono_family = preview_family(&ctx, Role::Mono, Weight::REGULAR);
        let style = ui.style_mut();
        for font in style.text_styles.values_mut() {
            let family = match font.family {
                FontFamily::Monospace => mono_family.clone(),
                FontFamily::Proportional => ui_family.clone(),
                FontFamily::Name(_) => None,
            };
            if let Some(f) = family {
                font.family = f;
            }
        }
        add(ui)
    })
    .inner
}

/// Whether a `Ui` drawing now is in [`with_preview`].
pub fn previewing(ctx: &Context) -> bool {
    PREVIEWING.get(ctx).unwrap_or(false)
}

/// The font a role's text draws at a weight and a size where it is
/// drawn: the preview family inside [`with_preview`] (once egui has
/// it), else [`font_id`].
pub fn shown_font_id(ctx: &Context, role: Role, weight: Weight, size: f32) -> FontId {
    if previewing(ctx)
        && let Some(family) = preview_family(ctx, role, weight)
    {
        return FontId::new(size, family);
    }
    font_id(role, weight, size)
}

/// Whether egui's fonts in use have a family: asking for one they do not
/// have panics. `false` before egui's first pass (no fonts yet).
fn bound(ctx: &Context, family: &FontFamily) -> bool {
    ctx.cumulative_pass_nr() > 0 && ctx.fonts(|f| f.definitions().families.contains_key(family))
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
/// the system's (feature `system-fonts`), each once. A name in several
/// places is listed at its first place with the origin the install
/// finds it at ([`origin_of`]): a registered face before the system's,
/// the system's before egui's.
pub fn families(ctx: &Context) -> Vec<FamilyInfo> {
    // The install's order: registered (and files), system, bundled.
    let rank = |o: Origin| match o {
        Origin::Registered | Origin::File => 0,
        Origin::System => 1,
        Origin::Bundled => 2,
    };
    let mut out: Vec<FamilyInfo> = Vec::new();
    let mut push = |info: FamilyInfo| match out
        .iter_mut()
        .find(|f| f.name.eq_ignore_ascii_case(&info.name))
    {
        Some(f) if rank(info.origin) < rank(f.origin) => {
            f.origin = info.origin;
            f.monospace = info.monospace;
        }
        Some(_) => {}
        None => out.push(info),
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
    for (name, monospace) in System::get(ctx).families().iter() {
        push(FamilyInfo {
            name: name.clone(),
            origin: Origin::System,
            monospace: *monospace,
        });
    }
    out
}

/// Where the family a theme names would be found, in the order the
/// install looks ([`install`]): registered, then the system's, then
/// egui's own faces; `Some(Origin::Bundled)` for a generic name
/// (`sans-serif`), `None` for a name found nowhere on this machine (its
/// list's next name draws).
pub fn origin_of(ctx: &Context, name: &str) -> Option<Origin> {
    if is_generic(name) {
        return Some(Origin::Bundled);
    }
    let registered = REGISTERED.get_or_default(ctx);
    if let Some(f) = registered
        .faces
        .iter()
        .find(|f| f.family.eq_ignore_ascii_case(name))
    {
        return Some(f.origin);
    }
    let on_system = System::get(ctx)
        .families()
        .iter()
        .any(|(n, _)| n.eq_ignore_ascii_case(name));
    if on_system {
        return Some(Origin::System);
    }
    FontDefinitions::default()
        .font_data
        .keys()
        .any(|k| bundled_key_matches(k, name))
        .then_some(Origin::Bundled)
}

/// Whether a name is a generic one (`sans-serif`, `serif`, `monospace`,
/// `system-ui`, `ui-monospace`): it stands for egui's bundled fonts, so
/// it is always found. A list ending with one always draws.
pub fn is_generic(name: &str) -> bool {
    GENERIC.iter().any(|g| g.eq_ignore_ascii_case(name))
}

/// Gives the system's fonts in this context in place of those
/// `system-fonts` finds (tests, so what is drawn never depends on the
/// machine; or an application finding fonts its own way): the faces,
/// their family as a theme names it. `None` goes back to the system's.
/// Installed with the next theme install, or at once with [`reinstall`].
pub fn set_system_faces(ctx: &Context, faces: Option<Vec<Face>>) {
    static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    match faces {
        None => {
            SYSTEM.remove(ctx);
        }
        Some(faces) => {
            let faces: Vec<Face> = faces
                .into_iter()
                .map(|f| Face {
                    origin: Origin::System,
                    ..f
                })
                .collect();
            let mut families: Vec<(String, bool)> = Vec::new();
            for f in &faces {
                if !families
                    .iter()
                    .any(|(n, _)| n.eq_ignore_ascii_case(&f.family))
                {
                    families.push((f.family.clone(), f.monospace()));
                }
            }
            families.sort_by_key(|(n, _)| n.to_lowercase());
            SYSTEM.set(
                ctx,
                SystemList {
                    faces: faces.into(),
                    families: families.into(),
                    generation: GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
                },
            );
        }
    }
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
    /// of egui's font definitions): the roles', the preview families'
    /// (`pop.preview.ui.400`) and the specimens'.
    pub chains: Vec<(String, Vec<String>)>,
    /// Per role (`ui`, `mono`): whether the face drawing its regular
    /// weight has tabular digits by default.
    pub tabular: Vec<(String, bool)>,
    /// The names drawn as specimens ([`specimen`]).
    pub specimens: Vec<String>,
    /// Where each name of the roles' lists and of the specimens was
    /// found, at the regular weight, in list order.
    pub resolved: Vec<Resolved>,
    pub problems: Vec<String>,
}

/// Where a family name was found by an install: so a theme author sees
/// `Ubuntu → bundled Ubuntu-Light` where the system has no Ubuntu.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Resolved {
    /// The list it is in: `ui`, `mono` or `specimen`.
    pub list: String,
    /// The name as the list gives it.
    pub name: String,
    /// Where its face comes from; `None` for a name found nowhere. A
    /// generic name (`sans-serif`) is egui's bundled fonts.
    pub origin: Option<Origin>,
    /// The face drawn: egui's key for a bundled one (`Ubuntu-Light`),
    /// the PostScript name for a system one when it has one, else its
    /// family and weight (`Inter 400`); `None` for a generic name or a
    /// name found nowhere.
    pub face: Option<String>,
}

impl std::fmt::Display for Resolved {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.origin, &self.face) {
            (Some(o), Some(face)) => write!(f, "{} → {} {face}", self.name, o.name()),
            (Some(o), None) => write!(f, "{} → {}", self.name, o.name()),
            (None, _) => write!(f, "{} → missing", self.name),
        }
    }
}

impl Report {
    /// Where a name of a list (`ui`, `mono` or `specimen`) was found.
    pub fn resolved(&self, list: &str, name: &str) -> Option<&Resolved> {
        self.resolved
            .iter()
            .find(|r| r.list == list && r.name.eq_ignore_ascii_case(name))
    }

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
/// The families each role's preview draws ([`set_preview`]), by role.
const PREVIEW: Slot<Previews> = Slot::opaque("core.fonts.preview");
/// The system's fonts given in place of fontdb's ([`set_system_faces`]).
const SYSTEM: Slot<SystemList> = Slot::opaque("core.fonts.system");
/// Whether a `Ui` draws in the preview families ([`with_preview`]).
const PREVIEWING: Slot<bool> = Slot::opaque("core.fonts.previewing");

/// What each role's preview draws: `[ui, mono]`, `None` for the role's
/// families as installed.
type Previews = [Option<Vec<String>>; 2];

/// Faces given as the system's ([`set_system_faces`]).
#[derive(Clone)]
struct SystemList {
    faces: Arc<[Face]>,
    /// Each family once, sorted, with whether it is monospace.
    families: Arc<[(String, bool)]>,
    generation: u64,
}

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

/// The faces drawing each role's regular text, as installed last.
const FACES: Slot<Arc<[Vec<InstalledFace>; 2]>> = Slot::opaque("core.fonts.faces");

/// A face drawing a role's text, as installed: the family the theme
/// names it by (`None` for egui's bundled faces behind the theme's),
/// and its file.
#[derive(Clone, Debug, PartialEq)]
pub struct InstalledFace {
    pub family: Option<String>,
    pub data: Arc<FontData>,
}

/// The faces drawing a role's regular text, as installed last, in
/// order: the theme's families found (registered, egui's or the
/// system's), then egui's bundled faces behind them. For a renderer
/// outside egui (an SVG rasteriser, a PDF writer) to draw the same text
/// as the screen. `None` before the first install.
pub fn installed_faces(ctx: &Context, role: Role) -> Option<Vec<InstalledFace>> {
    FACES.get(ctx).map(|f| f[role as usize].clone())
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
    preview: Previews,
    generation: u64,
    /// The faces given as the system's ([`set_system_faces`]), 0 for
    /// the system's own.
    system: u64,
}

const GENERIC: [&str; 5] = [
    "sans-serif",
    "serif",
    "monospace",
    "system-ui",
    "ui-monospace",
];

/// Installs the families named by the theme's tokens, with the
/// registered faces, unless they are already installed. In use from the
/// next pass. The icon font (`icon::install`) is kept.
pub fn install(ctx: &Context, ui: &[String], mono: &[String]) -> Arc<Report> {
    let registered = REGISTERED.get_or_default(ctx);
    let specimens = SPECIMENS.get_or_default(ctx);
    let preview = PREVIEW.get_or_default(ctx);
    let system = System::get(ctx);
    let key = Key {
        ui: ui.to_vec(),
        mono: mono.to_vec(),
        specimens: specimens.clone(),
        preview: preview.clone(),
        generation: registered.generation,
        system: system.generation(),
    };
    if let Some((k, report)) = INSTALLED.get(ctx)
        && k == key
    {
        return report;
    }
    let (mut defs, mut report, drawing) =
        definitions_and_faces(&registered.faces, ui, mono, &preview, &system);
    add_specimens(
        &mut defs,
        &mut report,
        &registered.faces,
        &specimens,
        &system,
    );
    for p in &report.problems {
        log::warn!(target: "egui_pop", "fonts: {p}");
    }
    ctx.set_fonts(defs);
    let report = Arc::new(report);
    INSTALLED.set(ctx, (key, report.clone()));
    FACES.set(ctx, Arc::new(drawing));
    report
}

/// A face found for a name at a weight: its key in the definitions, its
/// data, the weight it has, where it came from and the face it is, as
/// reports name it (`Ubuntu-Light`, `Inter 400`).
struct Found {
    key: String,
    data: Arc<FontData>,
    weight: Weight,
    origin: Origin,
    face: String,
}

/// Whether a key of egui's bundled faces is a family's: the family itself
/// (`Hack`) or one of its styles (`Ubuntu-Light`).
fn bundled_key_matches(key: &str, name: &str) -> bool {
    let (k, lower) = (key.to_ascii_lowercase(), name.to_ascii_lowercase());
    k == lower
        || k.strip_prefix(&lower)
            .is_some_and(|rest| rest.starts_with('-'))
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
                origin: face.origin,
                face: format!("{} {}", face.family, w.0),
            },
            weight,
        ));
    }
    // The system's before egui's: a theme naming `Ubuntu` gets the
    // system's Ubuntu where it is installed, as the platform would.
    if let Some(found) = system.find(name, weight) {
        return Some(on_axis(found, weight));
    }
    // egui's faces have one weight each: the others draw it.
    let (key, data) = bundled.iter().find(|(k, _)| bundled_key_matches(k, name))?;
    Some(Found {
        key: key.clone(),
        data: data.clone(),
        weight,
        origin: Origin::Bundled,
        face: key.clone(),
    })
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
        origin: found.origin,
        face: found.face,
    }
}

fn nearest(weight: Weight, among: impl Iterator<Item = Weight>) -> Option<Weight> {
    // CSS's rule, simplified: the closest, the heavier one on a tie.
    among.min_by_key(|w| (w.0.abs_diff(weight.0), std::cmp::Reverse(w.0)))
}

/// A chain of faces at a weight: the faces found for `names` (generic
/// ones skipped), in order, each added to `defs`.
struct Chain {
    keys: Vec<String>,
    /// The name each key was found by, in the same order.
    names: Vec<String>,
    /// The first face found: the name it was found by, its weight and
    /// where it came from.
    first: Option<(String, Weight, Origin)>,
    /// The names found nowhere.
    missing: Vec<String>,
    /// Each name of the list, in order, with where it was found and the
    /// face (generic ones too).
    resolved: Vec<(String, Option<Origin>, Option<String>)>,
}

fn chain_at(
    defs: &mut FontDefinitions,
    names: &[String],
    weight: Weight,
    faces: &[Face],
    bundled: &std::collections::BTreeMap<String, Arc<FontData>>,
    system: &System,
) -> Chain {
    let mut chain = Chain {
        keys: Vec::new(),
        names: Vec::new(),
        first: None,
        missing: Vec::new(),
        resolved: Vec::new(),
    };
    for name in names {
        if is_generic(name) {
            chain
                .resolved
                .push((name.clone(), Some(Origin::Bundled), None));
            continue;
        }
        let Some(found) = find(name, weight, faces, bundled, system) else {
            chain.missing.push(name.clone());
            chain.resolved.push((name.clone(), None, None));
            continue;
        };
        chain
            .resolved
            .push((name.clone(), Some(found.origin), Some(found.face.clone())));
        if chain.first.is_none() {
            chain.first = Some((name.clone(), found.weight, found.origin));
        }
        defs.font_data
            .entry(found.key.clone())
            .or_insert(found.data);
        if !chain.keys.contains(&found.key) {
            chain.keys.push(found.key);
            chain.names.push(name.clone());
        }
    }
    chain
}

/// The egui family a role's preview draws at a weight
/// (`pop.preview.ui.400`): defined by every install ([`set_preview`]).
fn preview_family_name(role: Role, weight: Weight) -> FontFamily {
    FontFamily::Name(
        format!(
            "pop.preview.{}.{}",
            role.key(),
            weight.nearest_installed().0
        )
        .into(),
    )
}

/// Adds faces to a chain, each once: a face already in it (egui's
/// Ubuntu named by the theme, then reached again through `sans-serif`)
/// keeps its first place, so egui never gets the same face twice in a
/// family.
fn extend_unique(chain: &mut Vec<String>, more: &[String]) {
    for k in more {
        if !chain.contains(k) {
            chain.push(k.clone());
        }
    }
}

#[cfg(test)]
fn definitions(
    faces: &[Face],
    ui: &[String],
    mono: &[String],
    preview: &Previews,
    system: &System,
) -> (FontDefinitions, Report) {
    let (defs, report, _) = definitions_and_faces(faces, ui, mono, preview, system);
    (defs, report)
}

/// [`definitions`], with the faces drawing each role's regular text
/// ([`installed_faces`]), `ui` then `mono`.
fn definitions_and_faces(
    faces: &[Face],
    ui: &[String],
    mono: &[String],
    preview: &Previews,
    system: &System,
) -> (FontDefinitions, Report, [Vec<InstalledFace>; 2]) {
    let mut drawing: [Vec<InstalledFace>; 2] = Default::default();
    let mut defs = FontDefinitions::default();
    let bundled_faces = defs.font_data.clone();
    crate::icon::insert(&mut defs);
    let bundled = |f: &FontFamily| defs.families.get(f).cloned().unwrap_or_default();
    let bundled_ui = bundled(&FontFamily::Proportional);
    let bundled_mono = bundled(&FontFamily::Monospace);
    let mut report = Report::default();
    for (at, role) in Role::ALL.into_iter().enumerate() {
        let (names, tail) = match role {
            Role::Ui => (ui, &bundled_ui),
            Role::Mono => (mono, &bundled_mono),
        };
        let mut missing: Vec<String> = Vec::new();
        for weight in WEIGHTS {
            let found = chain_at(&mut defs, names, weight, faces, &bundled_faces, system);
            let mut primary = None;
            if let Some((name, w, origin)) = &found.first {
                primary = Some(format!("{name} {} ({})", w.0, origin.name()));
                if *w != weight {
                    report.problems.push(format!(
                        "{}: {name} has no weight {}, {} is drawn",
                        role.token(),
                        weight.0,
                        w.0
                    ));
                }
            }
            let named = found.names;
            let mut chain = found.keys;
            if weight == Weight::REGULAR {
                missing = found.missing;
                report
                    .resolved
                    .extend(
                        found
                            .resolved
                            .into_iter()
                            .map(|(name, origin, face)| Resolved {
                                list: role.key().to_string(),
                                name,
                                origin,
                                face,
                            }),
                    );
                // The face drawing the digits: the first of the chain.
                let first = chain.first().or(tail.first());
                let tabular = first
                    .and_then(|k| defs.font_data.get(k))
                    .is_some_and(|d| tabular_digits(d));
                report.tabular.push((role.key().to_string(), tabular));
            }
            let theirs = chain.len();
            extend_unique(&mut chain, tail);
            if weight == Weight::REGULAR {
                // What egui draws the role's regular text with: the same
                // chain, each face once, the theme's by the name they
                // were found by, then egui's bundled ones.
                for (i, key) in chain.iter().enumerate() {
                    if let Some(data) = defs.font_data.get(key) {
                        drawing[at].push(InstalledFace {
                            family: (i < theirs).then(|| named[i].clone()),
                            data: data.clone(),
                        });
                    }
                }
            }
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
            // The preview family: the preview's faces before the role's
            // chain (its fallbacks), or the role's chain alone when the
            // preview finds no face (names unknown here, generic ones):
            // never empty, never less than what the role draws.
            let previewed = match &preview[at] {
                Some(list) => {
                    let mut c =
                        chain_at(&mut defs, list, weight, faces, &bundled_faces, system).keys;
                    extend_unique(&mut c, &chain);
                    c
                }
                None => chain.clone(),
            };
            let pf = preview_family_name(role, weight);
            if let FontFamily::Name(n) = &pf {
                report.chains.push((n.to_string(), previewed.clone()));
            }
            defs.families.insert(pf, previewed);
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
    (defs, report, drawing)
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
            let found = (!is_generic(name))
                .then(|| find(name, weight, faces, &bundled_faces, system))
                .flatten();
            if weight == Weight::REGULAR {
                report.resolved.push(Resolved {
                    list: "specimen".into(),
                    name: name.clone(),
                    origin: match &found {
                        Some(f) => Some(f.origin),
                        None if is_generic(name) => Some(Origin::Bundled),
                        None => None,
                    },
                    face: found.as_ref().map(|f| f.face.clone()),
                });
            }
            if let Some(found) = found {
                found_any = true;
                defs.font_data
                    .entry(found.key.clone())
                    .or_insert(found.data);
                chain.push(found.key);
            }
            extend_unique(&mut chain, &tail);
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

/// The system's fonts: fontdb's, read once per process (feature
/// `system-fonts`), or the faces given in their place in a context
/// ([`set_system_faces`]).
enum System {
    #[cfg(feature = "system-fonts")]
    Db(&'static fontdb::Database),
    Given(SystemList),
    /// No system fonts: without `system-fonts`, and in unit tests.
    #[cfg_attr(feature = "system-fonts", allow(dead_code))]
    None,
}

impl System {
    fn get(ctx: &Context) -> Self {
        if let Some(given) = SYSTEM.get(ctx) {
            return System::Given(given);
        }
        Self::own()
    }

    #[cfg(feature = "system-fonts")]
    fn own() -> Self {
        static DB: std::sync::OnceLock<fontdb::Database> = std::sync::OnceLock::new();
        System::Db(DB.get_or_init(|| {
            let mut db = fontdb::Database::new();
            db.load_system_fonts();
            db
        }))
    }

    #[cfg(not(feature = "system-fonts"))]
    fn own() -> Self {
        System::None
    }

    /// No system fonts (tests: what is drawn never depends on the
    /// machine).
    #[cfg(test)]
    fn none() -> Self {
        System::None
    }

    /// What an install compares: the faces given, or 0 for the system's.
    fn generation(&self) -> u64 {
        match self {
            System::Given(l) => l.generation,
            _ => 0,
        }
    }

    fn find(&self, name: &str, weight: Weight) -> Option<Found> {
        match self {
            #[cfg(feature = "system-fonts")]
            System::Db(db) => {
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
                    origin: Origin::System,
                    face: if info.post_script_name.is_empty() {
                        format!("{name} {}", w.0)
                    } else {
                        info.post_script_name.clone()
                    },
                })
            }
            System::Given(list) => {
                let own: Vec<&Face> = list
                    .faces
                    .iter()
                    .filter(|f| f.family.eq_ignore_ascii_case(name))
                    .collect();
                let w = nearest(weight, own.iter().map(|f| f.weight))?;
                let face = own.iter().find(|f| f.weight == w)?;
                Some(Found {
                    key: format!("pop:{}:{}:system", face.family, w.0),
                    data: face.data.clone(),
                    weight: w,
                    origin: Origin::System,
                    face: format!("{} {}", face.family, w.0),
                })
            }
            System::None => {
                let _ = (name, weight);
                None
            }
        }
    }

    /// The system's families, each once, sorted, with whether it is
    /// monospace.
    fn families(&self) -> Arc<[(String, bool)]> {
        match self {
            #[cfg(feature = "system-fonts")]
            System::Db(db) => {
                static LIST: std::sync::OnceLock<Arc<[(String, bool)]>> =
                    std::sync::OnceLock::new();
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
                    out.into()
                })
                .clone()
            }
            System::Given(list) => list.families.clone(),
            System::None => Arc::from(Vec::new()),
        }
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
            &Previews::default(),
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
        let (defs, report) = definitions(
            &faces,
            &names(&["Inter"]),
            &[],
            &Previews::default(),
            &System::none(),
        );
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
                origin: Origin::Bundled,
                face: "Hack".into(),
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
            &Previews::default(),
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
            &Previews::default(),
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
        let (defs, _) = definitions(&[], &[], &[], &Previews::default(), &System::none());
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
    fn a_specimen_asked_twice_in_a_frame_waits_for_egui_to_have_it() {
        let ctx = Context::default();
        install(&ctx, &names(&["Ubuntu"]), &names(&["Hack"]));
        pass(&ctx);
        // Installed by the first ask, but egui takes it at its next pass:
        // the second ask in the same frame must not hand it out.
        assert_eq!(specimen(&ctx, "Hack", Weight::REGULAR), None);
        assert_eq!(specimen(&ctx, "Hack", Weight::REGULAR), None);
        pass(&ctx);
        assert!(specimen(&ctx, "Hack", Weight::REGULAR).is_some());
    }

    /// The chain of an egui family in the last install's report.
    fn chain_of(ctx: &Context, family: &FontFamily) -> Vec<String> {
        report(ctx)
            .unwrap()
            .chains
            .iter()
            .find(|(f, _)| *family == FontFamily::Name(f.as_str().into()))
            .map(|(_, c)| c.clone())
            .unwrap_or_default()
    }

    #[test]
    fn a_preview_family_is_always_defined_and_survives_a_theme_install() {
        let ctx = Context::default();
        install(&ctx, &names(&["Ubuntu"]), &names(&["Hack"]));
        // Before egui's first pass: not bound yet, so not handed out.
        assert_eq!(preview_family(&ctx, Role::Ui, Weight::REGULAR), None);
        pass(&ctx);
        let family = preview_family(&ctx, Role::Ui, Weight::REGULAR).expect("always defined");
        // Without a preview, it draws the role's families.
        assert_eq!(
            chain_of(&ctx, &family),
            chain_of(&ctx, &FontFamily::Name("pop.ui.400".into()))
        );
        register(&ctx, Face::from_static("Inter", Weight::REGULAR, INTER_VAR));
        set_preview(&ctx, Role::Ui, Some(names(&["Inter", "sans-serif"])));
        assert_eq!(
            preview(&ctx, Role::Ui),
            Some(names(&["Inter", "sans-serif"]))
        );
        assert_eq!(chain_of(&ctx, &family)[0], "pop:Inter:400");
        // The theme's own families are untouched (the report names
        // egui's `Proportional` as the role's regular, `pop.ui.400`), and
        // follow the previewed face in the preview, as its fallbacks.
        let ui_chain = chain_of(&ctx, &FontFamily::Name("pop.ui.400".into()));
        assert_eq!(ui_chain[0], "Ubuntu-Light");
        assert_eq!(chain_of(&ctx, &family)[1..], ui_chain[..]);
        // The faces reported for renderers outside egui are that chain,
        // each face once.
        let drawn = installed_faces(&ctx, Role::Ui).unwrap();
        assert_eq!(drawn.len(), ui_chain.len());
        assert_eq!(drawn[0].family.as_deref(), Some("Ubuntu"));
        // No installed family has the same face twice (the theme's
        // `Ubuntu` and the bundled fonts behind `sans-serif` are one).
        for (f, chain) in &report(&ctx).unwrap().chains {
            let mut seen = std::collections::BTreeSet::new();
            assert!(chain.iter().all(|k| seen.insert(k)), "{f}: {chain:?}");
        }
        // A preview naming nothing found here falls back to the role's
        // families: a preview family is never empty.
        set_preview(&ctx, Role::Ui, Some(names(&["Nowhere Sans"])));
        assert_eq!(chain_of(&ctx, &family), ui_chain);
        set_preview(&ctx, Role::Ui, Some(Vec::new()));
        assert_eq!(chain_of(&ctx, &family), ui_chain);
        set_preview(&ctx, Role::Ui, Some(names(&["Inter", "sans-serif"])));
        assert_eq!(chain_of(&ctx, &family)[0], "pop:Inter:400");
        // A theme switched while the preview draws: the preview is
        // defined again by the new install, and drawing with it in the
        // same frame and the next never panics.
        install(&ctx, &names(&["Hack"]), &names(&["Hack"]));
        assert_eq!(chain_of(&ctx, &family)[0], "pop:Inter:400");
        let draw = |ctx: &Context| {
            ctx.run_ui(egui::RawInput::default(), |ui| {
                with_preview(ui, |ui| {
                    assert!(previewing(ui.ctx()));
                    ui.label("Hamburgefonstiv");
                    let font = shown_font_id(ui.ctx(), Role::Ui, Weight::BOLD, 13.0);
                    assert_eq!(font.family, preview_family_name(Role::Ui, Weight::BOLD));
                    ui.label(egui::RichText::new("Bold").font(font));
                    ui.monospace("0123");
                });
                assert!(!previewing(ui.ctx()));
            })
            .textures_delta
            .clear();
        };
        draw(&ctx);
        draw(&ctx);
        // The preview's bold weight is the variable face on its axis.
        assert_eq!(
            chain_of(&ctx, &preview_family_name(Role::Ui, Weight::BOLD))[0],
            "pop:Inter:400@wght700"
        );
        // Back to the role's families.
        set_preview(&ctx, Role::Ui, None);
        assert_eq!(
            chain_of(&ctx, &family),
            chain_of(&ctx, &FontFamily::Name("pop.ui.400".into()))
        );
    }

    #[test]
    fn faces_given_as_the_system_s_are_listed_found_and_reported() {
        let ctx = Context::default();
        let bundled = FontDefinitions::default().font_data;
        let hack = bundled["Hack"].font.to_vec();
        let ubuntu = bundled["Ubuntu-Light"].font.to_vec();
        set_system_faces(
            &ctx,
            Some(vec![
                Face::from_owned("Fake Sans", Weight::REGULAR, ubuntu),
                Face::from_owned("Fake Mono", Weight::REGULAR, hack),
            ]),
        );
        let list = families(&ctx);
        let find = |n: &str| list.iter().find(|f| f.name == n).cloned();
        assert_eq!(find("Fake Sans").unwrap().origin, Origin::System);
        assert!(find("Fake Mono").unwrap().monospace);
        assert!(!find("Fake Sans").unwrap().monospace);
        assert_eq!(origin_of(&ctx, "fake sans"), Some(Origin::System));
        assert_eq!(origin_of(&ctx, "Ubuntu"), Some(Origin::Bundled));
        assert_eq!(origin_of(&ctx, "sans-serif"), Some(Origin::Bundled));
        assert_eq!(origin_of(&ctx, "Nowhere Sans"), None);
        let report = install(&ctx, &names(&["Fake Sans", "sans-serif"]), &[]);
        assert!(report.problems.iter().all(|p| !p.contains("no font named")));
        let primary = report
            .primary
            .iter()
            .find(|(f, _)| f == "pop.ui.400")
            .and_then(|(_, p)| p.clone())
            .unwrap();
        assert_eq!(primary, "Fake Sans 400 (system)");
        // The faces a renderer outside egui draws with are the chain egui
        // got, the given system face first, by its name.
        let drawn = installed_faces(&ctx, Role::Ui).unwrap();
        let chain = &report
            .chains
            .iter()
            .find(|(f, _)| f == "pop.ui.400")
            .unwrap()
            .1;
        assert_eq!(drawn.len(), chain.len());
        assert_eq!(drawn[0].family.as_deref(), Some("Fake Sans"));
        assert!(drawn[1..].iter().all(|f| f.family.is_none()));
        // Another machine, without it: the next name draws, and the
        // report says what is missing.
        set_system_faces(&ctx, Some(Vec::new()));
        let report = reinstall(&ctx).unwrap();
        assert!(
            report
                .problems
                .iter()
                .any(|p| p.starts_with("font.family.ui: no font named Fake Sans")),
            "{report:?}"
        );
        assert_eq!(origin_of(&ctx, "Fake Sans"), None);
    }

    /// A context whose system fonts are the faces given (never the
    /// machine's), each from one of egui's own files.
    fn fake_system(faces: &[(&str, &str)]) -> Context {
        let ctx = Context::default();
        let bundled = FontDefinitions::default().font_data;
        set_system_faces(
            &ctx,
            Some(
                faces
                    .iter()
                    .map(|(family, file)| {
                        Face::from_owned(family, Weight::REGULAR, bundled[*file].font.to_vec())
                    })
                    .collect(),
            ),
        );
        ctx
    }

    #[test]
    fn a_name_on_the_system_and_bundled_resolves_to_the_system_s() {
        // The system has an Ubuntu (Hack's file, to tell them apart).
        let ctx = fake_system(&[("Ubuntu", "Hack")]);
        let (defs, report) = definitions(
            &[],
            &names(&["Ubuntu", "sans-serif"]),
            &names(&["monospace"]),
            &Previews::default(),
            &System::get(&ctx),
        );
        // The fake face has weight 400 only: the other weights draw it,
        // and the report says so; nothing else is a problem.
        assert!(
            report
                .problems
                .iter()
                .all(|p| p.starts_with("font.family.ui: Ubuntu has no weight ")
                    && p.ends_with(", 400 is drawn")),
            "{report:?}"
        );
        let regular = &defs.families[&FontFamily::Proportional];
        assert_eq!(regular[0], "pop:Ubuntu:400:system");
        let bold = &defs.families[&family(Role::Ui, Weight::BOLD)];
        assert_eq!(bold[0], "pop:Ubuntu:400:system");
        // egui's bundled faces stay behind it, each once.
        assert!(regular.iter().any(|k| k == "Ubuntu-Light"));
        let mut seen = std::collections::BTreeSet::new();
        assert!(regular.iter().all(|k| seen.insert(k)), "{regular:?}");
        let ubuntu = report.resolved("ui", "Ubuntu").unwrap();
        assert_eq!(ubuntu.origin, Some(Origin::System));
        assert_eq!(ubuntu.to_string(), "Ubuntu → system Ubuntu 400");
        assert_eq!(
            report.resolved("ui", "sans-serif").unwrap().origin,
            Some(Origin::Bundled)
        );
        // Asked by name, and listed, it is the system's too.
        assert_eq!(origin_of(&ctx, "ubuntu"), Some(Origin::System));
        let listed = families(&ctx);
        let ubuntu = listed.iter().find(|f| f.name == "Ubuntu").unwrap();
        assert_eq!(ubuntu.origin, Origin::System);
        assert!(ubuntu.monospace);
        assert_eq!(listed.iter().filter(|f| f.name == "Ubuntu").count(), 1);
        // A registered face still comes before the system's.
        let faces = vec![Face::from_static("Ubuntu", Weight::REGULAR, INTER_VAR)];
        let (defs, report) = definitions(
            &faces,
            &names(&["Ubuntu"]),
            &[],
            &Previews::default(),
            &System::get(&ctx),
        );
        assert_eq!(
            defs.families[&FontFamily::Proportional][0],
            "pop:Ubuntu:400"
        );
        assert_eq!(
            report.resolved("ui", "Ubuntu").unwrap().origin,
            Some(Origin::Registered)
        );
    }

    #[test]
    fn a_name_missing_on_the_system_falls_back_to_the_bundled_face() {
        let ctx = fake_system(&[("Fake Sans", "Ubuntu-Light")]);
        let (defs, report) = definitions(
            &[],
            &names(&["Ubuntu"]),
            &names(&["Hack"]),
            &Previews::default(),
            &System::get(&ctx),
        );
        assert!(report.problems.is_empty(), "{report:?}");
        assert_eq!(defs.families[&FontFamily::Proportional][0], "Ubuntu-Light");
        let ubuntu = report.resolved("ui", "Ubuntu").unwrap();
        assert_eq!(ubuntu.origin, Some(Origin::Bundled));
        assert_eq!(ubuntu.face.as_deref(), Some("Ubuntu-Light"));
        assert_eq!(ubuntu.to_string(), "Ubuntu → bundled Ubuntu-Light");
        assert_eq!(
            report.resolved("mono", "Hack").unwrap().to_string(),
            "Hack → bundled Hack"
        );
        assert_eq!(origin_of(&ctx, "Ubuntu"), Some(Origin::Bundled));
    }

    #[test]
    fn a_stack_whose_first_family_is_missing_everywhere_resolves_to_the_next() {
        // A platform's stack, on a machine with Adwaita Sans only.
        let ctx = fake_system(&[("Adwaita Sans", "Ubuntu-Light")]);
        let stack = names(&[
            "Helvetica",
            "Segoe UI",
            "Adwaita Sans",
            "Ubuntu",
            "sans-serif",
        ]);
        let (defs, report) =
            definitions(&[], &stack, &[], &Previews::default(), &System::get(&ctx));
        let regular = &defs.families[&FontFamily::Proportional];
        assert_eq!(regular[0], "pop:Adwaita Sans:400:system");
        let primary = report
            .primary
            .iter()
            .find(|(f, _)| f == "pop.ui.400")
            .and_then(|(_, p)| p.clone());
        assert_eq!(primary.as_deref(), Some("Adwaita Sans 400 (system)"));
        // The names after it are its fallbacks: egui's Ubuntu, once.
        assert_eq!(regular[1], "Ubuntu-Light");
        let mut seen = std::collections::BTreeSet::new();
        assert!(regular.iter().all(|k| seen.insert(k)), "{regular:?}");
        // Every name is said, in order, the missing ones as missing.
        let said: Vec<String> = report
            .resolved
            .iter()
            .filter(|r| r.list == "ui")
            .map(|r| r.to_string())
            .collect();
        assert_eq!(
            said,
            [
                "Helvetica → missing",
                "Segoe UI → missing",
                "Adwaita Sans → system Adwaita Sans 400",
                "Ubuntu → bundled Ubuntu-Light",
                "sans-serif → bundled",
            ]
        );
        for missing in ["Helvetica", "Segoe UI"] {
            assert!(
                report
                    .problems
                    .iter()
                    .any(|p| p.starts_with(&format!("font.family.ui: no font named {missing}"))),
                "{report:?}"
            );
        }
    }

    #[test]
    fn the_report_and_the_collector_say_where_each_name_was_found() {
        let ctx = fake_system(&[("Ubuntu", "Hack")]);
        crate::theme::install(&ctx, &crate::theme::ThemeSpec::new("pop-dark"));
        pass(&ctx);
        let bridge = crate::debug::DebugBridge::with_core_collectors();
        let said = |ctx: &Context| -> Vec<serde_json::Value> {
            let d = bridge.snapshot_of(ctx, "fonts").unwrap();
            d["resolved"].as_array().unwrap().clone()
        };
        let ubuntu = |list: &[serde_json::Value]| {
            list.iter()
                .find(|r| r["list"] == "ui" && r["name"] == "Ubuntu")
                .cloned()
                .unwrap()
        };
        let r = ubuntu(&said(&ctx));
        assert_eq!(r["origin"], "system");
        assert_eq!(r["face"], "Ubuntu 400");
        assert_eq!(r["says"], "Ubuntu → system Ubuntu 400");
        // Another machine, without Ubuntu: egui's, said as such.
        set_system_faces(&ctx, Some(Vec::new()));
        reinstall(&ctx);
        let r = ubuntu(&said(&ctx));
        assert_eq!(r["origin"], "bundled");
        assert_eq!(r["says"], "Ubuntu → bundled Ubuntu-Light");
        assert_eq!(
            report(&ctx)
                .unwrap()
                .resolved("ui", "Ubuntu")
                .unwrap()
                .origin,
            Some(Origin::Bundled)
        );
        // A specimen is said too, a name found nowhere as missing.
        assert_eq!(specimen(&ctx, "Nowhere Sans", Weight::REGULAR), None);
        let r = report(&ctx).unwrap();
        let nowhere = r.resolved("specimen", "Nowhere Sans").unwrap();
        assert_eq!(nowhere.origin, None);
        assert_eq!(nowhere.to_string(), "Nowhere Sans → missing");
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
        let (_, report) = definitions(
            &[],
            &names(&["Ubuntu"]),
            &names(&["Hack"]),
            &Previews::default(),
            &System::none(),
        );
        // A monospace face's digits are tabular.
        assert!(report.tabular_digits(Role::Mono));
        let hack = FontDefinitions::default().font_data["Hack"].clone();
        assert!(tabular_digits(&hack));
        assert!(monospace(&hack));
        let ubuntu = FontDefinitions::default().font_data["Ubuntu-Light"].clone();
        assert!(!monospace(&ubuntu));
        assert_eq!(report.tabular_digits(Role::Ui), tabular_digits(&ubuntu));
        // Each role and its preview, at each weight.
        assert_eq!(report.chains.len(), 4 * WEIGHTS.len());
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
        for ppp in [1.0, 1.25, 2.0] {
            ctx.set_pixels_per_point(ppp);
            pass(&ctx);
            let f = number_font(&ctx, t.font.body);
            assert_eq!(f.font.family, FontFamily::Monospace);
            // About the monospace size that matches the body's x-height,
            // never above it, its advance a whole number of pixels.
            assert!(f.font.size <= t.font.mono + 1e-3, "{ppp}: {}", f.font.size);
            assert!(f.font.size > t.font.mono * 0.8, "{ppp}: {}", f.font.size);
            let advance = ctx.fonts_mut(|fonts| fonts.glyph_width(&f.font, '0')) * ppp;
            assert!(
                (advance - advance.round()).abs() < 0.01,
                "{ppp}: {advance} pixels"
            );
            assert!(!f.cells);
            // A column of numbers, right-aligned: each digit's x from the
            // right is the same on every row.
            let rows: Vec<Vec<f32>> = ["-1234.56", "1111.11", "8888.88", "+0000.00"]
                .iter()
                .map(|n| from_right(&number_galley(&ctx, n, 13.0, Color32::WHITE)))
                .collect();
            for r in &rows[1..] {
                for (a, b) in r.iter().rev().zip(rows[0].iter().rev()) {
                    assert!((a - b).abs() < 0.01, "{ppp}: {rows:?}");
                }
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
    fn the_fonts_collector_says_what_is_installed_and_what_a_text_misses() {
        let ctx = Context::default();
        crate::theme::install(&ctx, &crate::theme::ThemeSpec::new("pop-dark"));
        // Before the first frame: no atlas yet, and no panic.
        let early = crate::debug::DebugBridge::with_core_collectors()
            .snapshot_of(&ctx, "fonts")
            .unwrap();
        assert!(early["atlas"]["size"].is_null());
        register(&ctx, Face::from_static("Inter", Weight::REGULAR, INTER_VAR));
        reinstall(&ctx);
        pass(&ctx);
        let mut bridge = crate::debug::DebugBridge::with_core_collectors();
        assert!(bridge.names().iter().any(|n| n == "fonts"));
        let d = &bridge.snapshot_of(&ctx, "fonts").unwrap();
        assert_eq!(
            d["requested"]["ui"],
            serde_json::json!(["Ubuntu", "sans-serif"])
        );
        assert_eq!(d["faces"][0]["family"], "Inter");
        assert_eq!(d["faces"][0]["origin"], "registered");
        assert_eq!(d["faces"][0]["monospace"], false);
        assert_eq!(d["numeric"], "mono");
        assert_eq!(d["sizes"]["text_styles"]["body"], 13.0);
        assert_eq!(d["text_size"], 1.0);
        assert_eq!(d["tabular_digits"]["mono"], true);
        let families = d["families"].as_array().unwrap();
        assert!(
            families.iter().any(
                |f| f["family"] == "pop.mono.400" && !f["chain"].as_array().unwrap().is_empty()
            )
        );
        assert!(d["atlas"]["fill_ratio"].as_f64().is_some());
        // `check`: what no face of the family covers.
        let f = bridge.call(
            &ctx,
            "fonts",
            "check",
            &serde_json::json!({ "text": "Aé 日本", "family": "ui" }),
        );
        assert!(f.ok, "{f:?}");
        assert_eq!(f.data["missing"], serde_json::json!(["日", "本"]));
        let f = bridge.call(&ctx, "fonts", "check", &serde_json::json!({}));
        assert!(!f.ok);
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
