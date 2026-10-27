//! The font pairs page: candidate pairs of a proportional and a
//! monospace family side by side, to choose the pair egui-pop bundles.
//! Each column draws the same interface text, numbers in a table (the
//! face's own digits and the monospace ones), a slider with its value,
//! code and a few scripts, in its pair's faces, with the size of its
//! files and whether its digits are tabular, measured.
//!
//! The pair egui-pop bundles (Inter and JetBrains Mono NL, feature
//! `bundled-fonts`) comes from `egui_pop::core::fonts::bundled`; the
//! other candidates are gallery assets (`assets/fonts`, each with its
//! licence and source). They are registered when the page is first
//! shown; egui's own pair is the baseline.

use egui::{Align, FontFamily, FontId, Layout, RichText, TextStyle, Ui, vec2};
use egui_pop::api::stories::Story;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::core::fonts::{self, Face, Weight, bundled};
use egui_pop::core::state::Slot;
use egui_pop::model::ids::{PluginId, StoryId};
use egui_pop::theme::Tokens;

use crate::shell::SHELL;

pub const FONTS_PAGE: PluginId = PluginId::from_static("gallery.fonts");

/// One face of a candidate: its family, weight and file.
#[derive(Clone, Copy, Debug)]
pub struct CandidateFace {
    pub family: &'static str,
    pub weight: u16,
    pub bytes: &'static [u8],
}

/// A pair to compare.
#[derive(Clone, Copy, Debug)]
pub struct Candidate {
    pub title: &'static str,
    /// The proportional family, as a theme names it.
    pub ui: &'static str,
    /// The monospace family.
    pub mono: &'static str,
    pub licence: &'static str,
    pub note: &'static str,
    /// The files the gallery carries; none for egui's own pair.
    pub faces: &'static [CandidateFace],
}

impl Candidate {
    /// The size of its files, in bytes (0: shipped by egui already).
    pub fn bytes(&self) -> usize {
        self.faces.iter().map(|f| f.bytes.len()).sum()
    }
}

macro_rules! face {
    ($family:literal, $weight:literal, $path:literal) => {
        CandidateFace {
            family: $family,
            weight: $weight,
            bytes: include_bytes!(concat!("../../assets/fonts/", $path)),
        }
    };
}

/// The pairs, egui's own first (the baseline).
pub const CANDIDATES: [Candidate; 4] = [
    Candidate {
        title: "egui's default",
        ui: "Ubuntu",
        mono: "Hack",
        licence: "Ubuntu Font Licence 1.0; MIT and Bitstream Vera",
        note: "The baseline: Ubuntu Light and Hack, shipped by egui.",
        faces: &[],
    },
    Candidate {
        title: "Inter + JetBrains Mono NL (bundled)",
        ui: "Inter",
        mono: "JetBrains Mono NL",
        licence: "OFL 1.1",
        note: "egui-pop's default pair. Inter variable (wght), subset; JetBrains Mono without ligatures, regular and bold, subset.",
        faces: &[
            CandidateFace {
                family: "Inter",
                weight: 400,
                bytes: bundled::INTER,
            },
            CandidateFace {
                family: "JetBrains Mono NL",
                weight: 400,
                bytes: bundled::JETBRAINS_MONO_NL_REGULAR,
            },
            CandidateFace {
                family: "JetBrains Mono NL",
                weight: 700,
                bytes: bundled::JETBRAINS_MONO_NL_BOLD,
            },
        ],
    },
    Candidate {
        title: "Noto Sans + Noto Sans Mono",
        ui: "Noto Sans",
        mono: "Noto Sans Mono",
        licence: "OFL 1.1",
        note: "Both variable (wght), subset; the family of the Noto CJK and Arabic fallbacks.",
        faces: &[
            face!("Noto Sans", 400, "noto/NotoSans-subset.ttf"),
            face!("Noto Sans Mono", 400, "noto/NotoSansMono-subset.ttf"),
        ],
    },
    Candidate {
        title: "IBM Plex Sans + Plex Mono",
        ui: "IBM Plex Sans",
        mono: "IBM Plex Mono",
        licence: "OFL 1.1, Reserved Font Name \"Plex\"",
        note: "Plex Sans variable (wght); Plex Mono regular and bold; as published (a subset must be renamed).",
        faces: &[
            face!("IBM Plex Sans", 400, "ibm-plex/IBMPlexSans-VF.ttf"),
            face!("IBM Plex Mono", 400, "ibm-plex/IBMPlexMono-Regular.ttf"),
            face!("IBM Plex Mono", 700, "ibm-plex/IBMPlexMono-Bold.ttf"),
        ],
    },
];

/// The numbers of the table: their digits line up only when tabular.
pub const NUMBERS: [f64; 6] = [0.0, 1.11, -1234.56, 88.88, 7.5, 1000.0];

const SCRIPTS: [(&str, &str); 6] = [
    ("Latin", "Àéïõü ßŒ ąčęł ñ"),
    ("Vietnamese", "Tiếng Việt"),
    ("Greek", "Ελληνικά αβγδ"),
    ("Cyrillic", "Кириллица ёжик"),
    ("Symbols", "€ £ ¥ → ≤ ≥ ± × −"),
    ("CJK (no face here)", "日本語 中文"),
];

/// The faces were registered in this context.
const REGISTERED: Slot<bool> = Slot::opaque("gallery.fonts.registered");
/// The slider's value.
const EXPOSURE: Slot<f32> = Slot::opaque("gallery.fonts.exposure");

/// Registers the candidates' faces (once per context): each family is
/// then found by name, by the page and by any theme or user naming it.
pub fn register_candidates(ctx: &egui::Context) {
    if REGISTERED.get(ctx).unwrap_or(false) {
        return;
    }
    for c in &CANDIDATES {
        for f in c.faces {
            fonts::register(ctx, Face::from_static(f.family, Weight(f.weight), f.bytes));
        }
    }
    REGISTERED.set(ctx, true);
    fonts::reinstall(ctx);
}

/// The advances of the digits `0` to `9` in a family at a size.
pub fn digit_advances(ctx: &egui::Context, family: FontFamily, size: f32) -> Vec<f32> {
    let id = FontId::new(size, family);
    ctx.fonts_mut(|f| ('0'..='9').map(|c| f.glyph_width(&id, c)).collect())
}

/// Whether advances are all equal (within a hundredth of a point).
pub fn all_equal(advances: &[f32]) -> bool {
    advances.windows(2).all(|w| (w[0] - w[1]).abs() < 0.01)
}

/// The page.
#[derive(Debug, Default)]
pub struct FontsPage;

impl Plugin for FontsPage {
    fn id(&self) -> PluginId {
        FONTS_PAGE
    }

    fn title(&self) -> String {
        "Font pairs".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.story(
            Story::new(StoryId::from_static("fonts"), "Font pairs", show)
                .description("Candidate pairs of a proportional and a monospace family side by side, egui's own as the baseline: interface text, numbers in a table, a slider, code and scripts, with each pair's size and whether its digits are tabular. Where the pair egui-pop bundles is chosen.")
                .group("Theme", 28),
        )?;
        Ok(())
    }
}

/// The families a candidate draws with, once installed: proportional
/// regular and semibold, monospace regular (a specimen is installed at
/// every weight at once).
fn families(ctx: &egui::Context, c: &Candidate) -> Option<[FontFamily; 3]> {
    // Ask them all, so they are installed in one go.
    let asked = [
        fonts::specimen(ctx, c.ui, Weight::REGULAR),
        fonts::specimen(ctx, c.ui, Weight::SEMIBOLD),
        fonts::specimen(ctx, c.mono, Weight::REGULAR),
    ];
    let [Some(a), Some(b), Some(m)] = asked else {
        return None;
    };
    Some([a, b, m])
}

fn show(ui: &mut Ui, _app: &AppHandle) {
    let ctx = ui.ctx().clone();
    register_candidates(&ctx);
    ui.label("Each column draws the same content in one pair. egui applies no OpenType feature: digits line up only in a face whose default digits are tabular, ligatures cannot be turned off (hence JetBrains Mono NL), and a weight is a variable face's wght axis or a face of its own. Sizes are the files carried, subset to Latin, Greek and Cyrillic where the licence allows; egui-pop bundles Inter + JetBrains Mono NL (feature bundled-fonts), within a budget of about 1 MB.");
    let t = Tokens::of(&ctx);
    let width = 300.0;
    ui.add_space(t.space.m);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(t.space.l, t.space.l);
        for c in &CANDIDATES {
            ui.vertical(|ui| {
                ui.set_width(width);
                egui::Frame::new()
                    .fill(t.surface.raised)
                    .stroke(egui::Stroke::new(1.0, t.border.default))
                    .corner_radius(egui::CornerRadius::same(
                        t.radius.widget.round().clamp(0.0, 255.0) as u8,
                    ))
                    .inner_margin(egui::Margin::same(t.space.m.round().clamp(0.0, 127.0) as i8))
                    .show(ui, |ui| {
                        ui.set_width(width - 2.0 * t.space.m);
                        column(ui, c);
                    });
            });
        }
    });
}

fn column(ui: &mut Ui, c: &Candidate) {
    let ctx = ui.ctx().clone();
    let t = Tokens::of(&ctx);
    ui.strong(c.title);
    let size = match c.bytes() {
        0 => "in egui already".to_string(),
        b => format!("{} KB", (b as f32 / 1000.0).round()),
    };
    ui.weak(format!("{} · {size}", c.licence));
    ui.weak(c.note);
    let Some([regular, strong, mono]) = families(&ctx, c) else {
        ui.weak("Loading the faces…");
        return;
    };
    let body = t.font.body;
    let ui_tabular = all_equal(&digit_advances(&ctx, regular.clone(), body));
    let mono_tabular = all_equal(&digit_advances(&ctx, mono.clone(), t.font.mono));
    ui.label(format!(
        "Tabular digits: {} (proportional), {} (monospace)",
        if ui_tabular { "yes" } else { "no" },
        if mono_tabular { "yes" } else { "no" }
    ));
    ui.separator();
    ui.scope(|ui| {
        let style = ui.style_mut();
        for (text_style, family) in [
            (TextStyle::Body, &regular),
            (TextStyle::Button, &regular),
            (TextStyle::Small, &regular),
            (TextStyle::Heading, &strong),
            (TextStyle::Monospace, &mono),
        ] {
            if let Some(f) = style.text_styles.get_mut(&text_style) {
                f.family = family.clone();
            }
        }
        sample(ui, c, &regular, &mono);
    });
}

fn sample(ui: &mut Ui, c: &Candidate, regular: &FontFamily, mono: &FontFamily) {
    let ctx = ui.ctx().clone();
    let t = Tokens::of(&ctx);
    ui.heading("Export the selection");
    ui.label("Sphinx of black quartz, judge my vow. The quick brown fox jumps over the lazy dog.");
    ui.horizontal(|ui| {
        let _ = ui.button("Apply");
        let _ = ui.button("Cancel");
        let mut on = true;
        ui.checkbox(&mut on, "Keep metadata");
    });
    ui.horizontal(|ui| {
        for (name, w) in [
            ("Regular", Weight::REGULAR),
            ("Medium", Weight::MEDIUM),
            ("Semibold", Weight::SEMIBOLD),
            ("Bold", Weight::BOLD),
        ] {
            // A specimen is installed at every weight at once.
            let family = fonts::specimen(&ctx, c.ui, w).unwrap_or_else(|| regular.clone());
            ui.label(RichText::new(name).family(family));
        }
    });
    let mut exposure = EXPOSURE.get(&ctx).unwrap_or(1.25);
    ui.add(
        egui::Slider::new(&mut exposure, -5.0..=5.0)
            .text("Exposure")
            .fixed_decimals(2),
    );
    EXPOSURE.set(&ctx, exposure);

    ui.add_space(t.space.s);
    ui.label(RichText::new("Numbers, right-aligned").strong());
    let cell = vec2(110.0, ui.spacing().interact_size.y);
    // Fixed cells, no Grid: the column's size never depends on the
    // previous frame (the faces are installed while the page settles).
    let row = |ui: &mut Ui, cells: [(RichText, bool); 2]| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = t.space.m;
            for (text, weak) in cells {
                ui.allocate_ui_with_layout(cell, Layout::right_to_left(Align::Center), |ui| {
                    ui.set_min_size(cell);
                    if weak {
                        ui.weak(text.text());
                    } else {
                        ui.label(text);
                    }
                });
            }
        });
    };
    row(
        ui,
        [
            (RichText::new("proportional"), true),
            (RichText::new("monospace"), true),
        ],
    );
    for n in NUMBERS {
        let text = format!("{n:.2}");
        row(
            ui,
            [
                (RichText::new(&text).family(regular.clone()), false),
                (RichText::new(&text).family(mono.clone()), false),
            ],
        );
    }

    ui.add_space(t.space.s);
    ui.label(RichText::new("Code").strong());
    for line in ["fn main() { let x = 0O1lI; }", "a != b -> c >= d |{}[]"] {
        ui.monospace(line);
    }

    ui.add_space(t.space.s);
    ui.label(RichText::new("Scripts").strong());
    for (name, text) in SCRIPTS {
        ui.horizontal(|ui| {
            ui.weak(name);
            ui.label(text);
        });
    }
}
