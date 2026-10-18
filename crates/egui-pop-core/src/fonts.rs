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
//! the `FontId` to draw with. A variable face (a `wght` axis) serves
//! every weight its axis covers, set on the face (`FontTweak::coords`);
//! a static face serves its own weight, and a weight a family lacks takes
//! its nearest one, said in the report. egui's `Proportional` and `Monospace` families are
//! the regular weight of `ui` and `mono`, so egui's own widgets follow the
//! theme too.
//!
//! [`install`] runs with the theme (`theme::install_tokens`); it replaces
//! egui's font definitions only when the families or the registered faces
//! changed (one slow frame: egui rebuilds its atlas).

use std::sync::Arc;

use egui::{Context, FontData, FontDefinitions, FontFamily, FontId, Id};

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
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

/// A font face the application gives: its family name (as a theme names
/// it), its weight and the bytes of a `.ttf` or `.otf` file.
#[derive(Clone)]
pub struct Face {
    pub family: String,
    pub weight: Weight,
    data: Arc<FontData>,
}

impl Face {
    /// A face from bytes the binary embeds.
    pub fn from_static(family: &str, weight: Weight, bytes: &'static [u8]) -> Self {
        Self {
            family: family.into(),
            weight,
            data: Arc::new(FontData::from_static(bytes)),
        }
    }

    /// A face from bytes read at run time.
    pub fn from_owned(family: &str, weight: Weight, bytes: Vec<u8>) -> Self {
        Self {
            family: family.into(),
            weight,
            data: Arc::new(FontData::from_owned(bytes)),
        }
    }
}

impl std::fmt::Debug for Face {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Face")
            .field("family", &self.family)
            .field("weight", &self.weight)
            .finish_non_exhaustive()
    }
}

/// What [`install`] did: the face each family starts with, and what it
/// could not do.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    /// Per egui family (`pop.ui.700`): the face drawn first, as
    /// "Family weight (source)", or `None` for egui's bundled fonts.
    pub primary: Vec<(String, Option<String>)>,
    pub problems: Vec<String>,
}

#[derive(Clone, Default)]
struct Registered {
    faces: Vec<Face>,
    generation: u64,
}

fn registered_id() -> Id {
    Id::new("egui_pop_core::fonts::registered")
}

fn installed_id() -> Id {
    Id::new("egui_pop_core::fonts::installed")
}

/// Registers a face for the families a theme names; installed with the
/// next theme install. A face of the same family and weight is replaced.
pub fn register(ctx: &Context, face: Face) {
    ctx.data_mut(|d| {
        let r = d.get_temp_mut_or_default::<Registered>(registered_id());
        r.faces
            .retain(|f| !(f.family.eq_ignore_ascii_case(&face.family) && f.weight == face.weight));
        r.faces.push(face);
        r.generation += 1;
    });
}

/// The last install's report.
pub fn report(ctx: &Context) -> Option<Arc<Report>> {
    ctx.data(|d| d.get_temp::<(Key, Arc<Report>)>(installed_id()))
        .map(|(_, r)| r)
}

#[derive(Clone, PartialEq)]
struct Key {
    ui: Vec<String>,
    mono: Vec<String>,
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
    let registered = ctx.data(|d| {
        d.get_temp::<Registered>(registered_id())
            .unwrap_or_default()
    });
    let key = Key {
        ui: ui.to_vec(),
        mono: mono.to_vec(),
        generation: registered.generation,
    };
    if let Some((k, report)) = ctx.data(|d| d.get_temp::<(Key, Arc<Report>)>(installed_id()))
        && k == key
    {
        return report;
    }
    let (defs, report) = definitions(&registered.faces, ui, mono, &System::get());
    for p in &report.problems {
        log::warn!(target: "egui_pop", "fonts: {p}");
    }
    ctx.set_fonts(defs);
    let report = Arc::new(report);
    ctx.data_mut(|d| d.insert_temp(installed_id(), (key, report.clone())));
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
                source: "registered",
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
            chain.extend(tail.iter().cloned());
            let fam = family(role, weight);
            if let FontFamily::Name(n) = &fam {
                report.primary.push((n.to_string(), primary));
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
}

#[cfg(test)]
mod tests {
    use super::*;

    static FAKE: &[u8] = b"not a real font, never rasterised here";

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
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
}
