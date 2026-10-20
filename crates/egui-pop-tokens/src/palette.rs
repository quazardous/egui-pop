//! Palette ramps generated from one seed colour.
//!
//! [`ramp`] turns a seed into the twelve steps of a colour scale, in the
//! step model of the reference tier (`docs/tokens.md`, after Radix
//! Colors): 1 and 2 backgrounds, 3 to 5 quiet fills, 6 and 7
//! borders, 8 a solid fill being pressed, 9 the solid fill, 10 the solid
//! fill under the pointer, 11 low-contrast text, 12 high-contrast text.
//! The light and dark ramps of a theme pair come from the same seed
//! ([`Scheme`]). [`Ramp`] adds the text colour that reads on step 9 (a
//! scale's `contrast`), and writes the scale as tokens.
//!
//! How a ramp is made, in OkHsl ([`Okhsl`], Björn Ottosson's
//! perceptual hue, saturation and lightness over Oklab, whose saturation
//! is relative to the sRGB gamut, so no step leaves it):
//!
//! - **Lightness** per step is a fixed target for the backgrounds and
//!   borders (1 to 7) and the high-contrast text (12), taken from the
//!   built-in `pop-light` and `pop-dark` scales; steps 8, 10 and 11
//!   follow step 9.
//! - **Step 9 is the seed** when it reads: APCA ([`crate::apca`]) checks
//!   the text on it, and its lightness moves away from the text (darker
//!   in light, lighter in dark) until the text reaches Lc 60 and WCAG
//!   4.5:1 with margin. egui-pop's accent is also text on panels, so in
//!   a light scheme step 9 is dark enough for white text, in a dark one
//!   light enough for dark text.
//! - **Saturation** is the seed's, scaled per step (quiet backgrounds,
//!   full solids), then limited per hue band: greens and cyans turn
//!   neon, and yellows muddy, at full saturation in large areas.
//! - **Hue** shifts with lightness, as warm and cool colours are seen to
//!   (lighter warm steps lean to yellow, lighter cool steps to cyan), by
//!   a few degrees at most.
//!
//! [`clamp_seed`] brings a seed into what gives a usable ramp (not
//! nearly black or white, saturation within its band); [`ramp`] applies
//! it first. A grey seed gives a neutral ramp.
//!
//! The approach follows egui_colors (MIT), which generates 12-step
//! Radix-like scales in OkHsl with hue shifts, saturation limits per
//! hue band and APCA; this is a re-derivation with documented targets,
//! not its code. The OkHsl conversions follow Ottosson's published
//! reference implementation (MIT).
//!
//! ```
//! use egui_pop_tokens::palette::{self, Scheme};
//! use egui_pop_tokens::{Color, contrast};
//!
//! let seed = Color::from_hex("#3a7bd5").unwrap();
//! let light = palette::Ramp::new(seed, Scheme::Light);
//! assert!(contrast::contrast_ratio(light.contrast, light.steps[8]) >= 4.5);
//! let dark = palette::ramp(seed, Scheme::Dark);
//! assert!(dark[0].is_dark() && !dark[11].is_dark());
//! ```

use serde_json::{Value as Json, json};

use crate::Color;
use crate::apca;
use crate::color::contrast_ratio;

/// Which kind of theme a ramp is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Scheme {
    /// Light backgrounds, dark text.
    Light,
    /// Dark backgrounds, light text.
    Dark,
}

impl Scheme {
    /// The scheme of a theme whose panels are this colour.
    pub fn of_background(panel: Color) -> Scheme {
        if panel.is_dark() {
            Scheme::Dark
        } else {
            Scheme::Light
        }
    }
}

/// A colour in OkHsl: hue in degrees (0 to 360), saturation and
/// lightness from 0 to 1, saturation relative to the most the sRGB gamut
/// holds at that hue and lightness.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Okhsl {
    pub h: f32,
    pub s: f32,
    pub l: f32,
}

impl Okhsl {
    /// A colour's OkHsl coordinates (alpha ignored).
    pub fn from_color(c: Color) -> Okhsl {
        let (h, s, l) = ok::srgb_to_okhsl(c);
        Okhsl {
            h: h as f32,
            s: s as f32,
            l: l as f32,
        }
    }

    /// The opaque sRGB colour.
    pub fn to_color(self) -> Color {
        ok::okhsl_to_srgb(
            f64::from(self.h),
            f64::from(self.s.clamp(0.0, 1.0)),
            f64::from(self.l.clamp(0.0, 1.0)),
        )
    }
}

/// The lowest and highest lightness of a usable seed.
pub const SEED_LIGHTNESS: (f32, f32) = (0.25, 0.92);

/// The Lc the text on step 9 reaches, at least (APCA's floor for
/// ordinary text).
pub const SOLID_LC: f32 = 60.0;

/// The WCAG ratio the text on steps 9 and 10, and step 9 on the
/// scheme's backgrounds, reach at least: AA's 4.5:1 with a margin for
/// panels that are not the ramp's own backgrounds.
pub const SOLID_RATIO: f32 = 5.0;

/// Background lightness targets, steps 1 to 7.
const LIGHT_L: [f32; 7] = [0.985, 0.96, 0.925, 0.89, 0.855, 0.81, 0.73];
const DARK_L: [f32; 7] = [0.075, 0.1, 0.15, 0.2, 0.25, 0.305, 0.38];
/// High-contrast text, step 12.
const LIGHT_TEXT_L: f32 = 0.22;
const DARK_TEXT_L: f32 = 0.93;
/// The most step 9 may be, light or dark, so 8, 10 and 11 stay apart.
const LIGHT_SOLID_MIN_L: f32 = 0.32;
const DARK_SOLID_MAX_L: f32 = 0.84;

/// The seed's saturation is scaled by these, steps 1 to 12.
const LIGHT_S: [f32; 12] = [
    0.45, 0.6, 0.7, 0.75, 0.75, 0.75, 0.8, 0.95, 1.0, 1.0, 0.95, 0.7,
];
const DARK_S: [f32; 12] = [
    0.3, 0.35, 0.5, 0.6, 0.65, 0.65, 0.7, 0.9, 1.0, 1.0, 0.9, 0.5,
];

/// Degrees of hue shift per unit of lightness away from step 9.
const HUE_SHIFT: f32 = 16.0;

/// A weight from 0 to 1 that is 1 inside `from..=to` and fades to 0
/// over `fade` degrees on each side (hues wrap at 360).
fn band(h: f32, from: f32, to: f32, fade: f32) -> f32 {
    let inside = |h: f32| {
        if h >= from && h <= to {
            1.0
        } else if h < from {
            (1.0 - (from - h) / fade).max(0.0)
        } else {
            (1.0 - (h - to) / fade).max(0.0)
        }
    };
    inside(h).max(inside(h - 360.0)).max(inside(h + 360.0))
}

/// The most saturation a step of this hue takes: greens and cyans
/// (OkHsl hue 125 to 200) and yellows (85 to 115) are limited in large
/// areas (steps 1 to 7) and less so in solids.
fn saturation_cap(h: f32, step: usize, scheme: Scheme) -> f32 {
    let background = step < 7;
    let green = band(h, 125.0, 200.0, 15.0);
    let yellow = band(h, 85.0, 115.0, 10.0);
    let green_cap = if background { 0.55 } else { 0.85 };
    let yellow_cap = match (background, scheme) {
        (true, Scheme::Dark) => 0.45,
        (true, Scheme::Light) => 0.75,
        (false, _) => 1.0,
    };
    let cap = |weight: f32, cap: f32| 1.0 - weight * (1.0 - cap);
    cap(green, green_cap).min(cap(yellow, yellow_cap))
}

/// The hue of a step `dl` lighter than step 9: warm hues (red to
/// yellow, 0 to 100) lean to yellow when lighter, cool ones (blue, 200
/// to 290) to cyan.
fn shifted_hue(h: f32, dl: f32) -> f32 {
    let warm = band(h, 10.0, 90.0, 20.0);
    let cool = band(h, 210.0, 280.0, 20.0);
    (h + HUE_SHIFT * dl * (warm - cool)).rem_euclid(360.0)
}

/// A seed brought into what gives a usable ramp: opaque, its lightness
/// within [`SEED_LIGHTNESS`] (nearly black or white seeds have no hue to
/// speak of) and its saturation within its hue band's limit for solids.
/// A seed already usable comes back unchanged (to the byte).
pub fn clamp_seed(seed: Color) -> Color {
    let opaque = Color { a: 1.0, ..seed };
    let hsl = Okhsl::from_color(opaque);
    let l = hsl.l.clamp(SEED_LIGHTNESS.0, SEED_LIGHTNESS.1);
    let s = hsl.s.min(saturation_cap(hsl.h, 8, Scheme::Light));
    if (l - hsl.l).abs() < 1e-4 && (s - hsl.s).abs() < 1e-4 {
        return opaque;
    }
    Okhsl { h: hsl.h, s, l }.to_color()
}

/// Whether a seed gives a ramp as it is ([`clamp_seed`] leaves it).
pub fn is_usable_seed(seed: Color) -> bool {
    clamp_seed(seed).to_hex() == Color { a: 1.0, ..seed }.to_hex()
}

/// The dark text of a dark scheme's solid: nearly black, of the seed's
/// hue.
fn dark_text(h: f32, s: f32) -> Color {
    Okhsl {
        h,
        s: s.min(0.3),
        l: 0.06,
    }
    .to_color()
}

/// The text colour that reads on a solid fill, as APCA ranks them:
/// white, or `dark` (nearly black), whichever has the larger Lc.
pub fn text_on(solid: Color, dark: Color) -> Color {
    if apca::lc(Color::WHITE, solid).abs() >= apca::lc(dark, solid).abs() {
        Color::WHITE
    } else {
        dark
    }
}

/// Whether text reads on a solid well enough for a ramp's step 9.
fn reads(text: Color, solid: Color) -> bool {
    apca::lc(text, solid).abs() >= SOLID_LC && contrast_ratio(text, solid) >= SOLID_RATIO
}

/// What a ramp is drawn with in a theme, when it is known: the ramp is
/// then fitted to it ([`Ramp::fitted`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Backdrop {
    /// Where step 9 is drawn as text (an accent on panels, a keyword in
    /// an editor): it reaches [`FITTED_RATIO`] on each.
    pub surfaces: Vec<Color>,
    /// Where step 9 is drawn as a part to see (a slider's fill on its
    /// track): it reaches 3:1 (3.1, for rounding) on each when the
    /// scheme's direction allows.
    pub parts: Vec<Color>,
    /// Text drawn on steps 3 to 5 (a selection): those steps move away
    /// from it until it reaches [`FITTED_RATIO`].
    pub text: Vec<Color>,
}

/// The WCAG ratio a fitted ramp reaches on the surfaces and text of its
/// [`Backdrop`]: AA's 4.5:1, with a small margin for rounding.
pub const FITTED_RATIO: f32 = 4.6;

/// A ramp and the text colour on its solid step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ramp {
    /// Steps 1 to 12 (`steps[0]` is step 1).
    pub steps: [Color; 12],
    /// The text that reads on step 9 and step 10 (a scale's `contrast`).
    pub contrast: Color,
}

impl Ramp {
    /// The ramp of a seed in a scheme (the seed clamped first).
    pub fn new(seed: Color, scheme: Scheme) -> Ramp {
        Ramp::fitted(seed, scheme, &Backdrop::default())
    }

    /// The ramp of a seed in a scheme, fitted to what a theme draws it
    /// with: step 9 moves on (darker in light, lighter in dark) until it
    /// also reads on the backdrop's surfaces and is seen on its parts,
    /// and steps 3 to 5 move away from its text until it reads on them.
    pub fn fitted(seed: Color, scheme: Scheme, backdrop: &Backdrop) -> Ramp {
        let clamped = clamp_seed(seed);
        let seed = Okhsl::from_color(clamped);
        let (h, s0) = (seed.h, seed.s);
        let mults = match scheme {
            Scheme::Light => LIGHT_S,
            Scheme::Dark => DARK_S,
        };
        let sat = |step: usize| (s0 * mults[step]).min(saturation_cap(h, step, scheme));

        // Step 9: the seed, its lightness moved until the text reads.
        let s9 = sat(8);
        let dark = dark_text(h, s0);
        let (text, mut l9) = match scheme {
            Scheme::Light => (Color::WHITE, seed.l.max(LIGHT_SOLID_MIN_L)),
            Scheme::Dark => (dark, seed.l.min(DARK_SOLID_MAX_L)),
        };
        let solid = |l: f32| Okhsl { h, s: s9, l }.to_color();
        let behind = match scheme {
            Scheme::Light => Color::WHITE,
            Scheme::Dark => dark,
        };
        for _ in 0..200 {
            let c = solid(l9);
            let fits = backdrop
                .surfaces
                .iter()
                .all(|b| contrast_ratio(c, *b) >= FITTED_RATIO)
                && backdrop.parts.iter().all(|b| contrast_ratio(c, *b) >= 3.1);
            if reads(text, c) && contrast_ratio(c, behind) >= SOLID_RATIO && fits {
                break;
            }
            match scheme {
                Scheme::Light if l9 > 0.05 => l9 -= 0.005,
                Scheme::Dark if l9 < 0.98 => l9 += 0.005,
                _ => break,
            }
        }

        let mut l = [0.0_f32; 12];
        match scheme {
            Scheme::Light => {
                l[..7].copy_from_slice(&LIGHT_L);
                l[8] = l9;
                l[7] = (l9 + 0.09).min(LIGHT_L[6] - 0.04);
                l[9] = l9 - 0.045;
                l[10] = (l9 - 0.03).min(0.45);
                l[11] = LIGHT_TEXT_L.min(l[10] - 0.1);
            }
            Scheme::Dark => {
                l[..7].copy_from_slice(&DARK_L);
                l[8] = l9;
                l[7] = (l9 - 0.08).max(DARK_L[6] + 0.05);
                l[9] = (l9 + 0.05).min(0.97);
                l[10] = (l9 + 0.08).clamp(0.76, 0.9);
                l[11] = DARK_TEXT_L.max(l[10] + 0.04);
            }
        }
        // Steps 3 to 5 away from the text drawn on them, then 1 and 2
        // kept beyond them.
        let quiet = |i: usize, l: f32| {
            Okhsl {
                h: shifted_hue(h, l - l9),
                s: sat(i),
                l,
            }
            .to_color()
        };
        for i in (2..5).rev() {
            for _ in 0..100 {
                let c = quiet(i, l[i]);
                if backdrop
                    .text
                    .iter()
                    .all(|t| contrast_ratio(*t, c) >= FITTED_RATIO)
                {
                    break;
                }
                match scheme {
                    Scheme::Light if l[i] < 0.995 => l[i] = (l[i] + 0.005).min(0.995),
                    Scheme::Dark if l[i] > 0.01 => l[i] = (l[i] - 0.005).max(0.01),
                    _ => break,
                }
            }
        }
        for i in (0..2).rev() {
            l[i] = match scheme {
                Scheme::Light => l[i].max((l[i + 1] + 0.005).min(0.998)),
                Scheme::Dark => l[i].min((l[i + 1] - 0.005).max(0.0)),
            };
        }
        let mut steps = [Color::BLACK; 12];
        let as_given = (l9 - seed.l).abs() < 1e-6 && (s9 - s0).abs() < 1e-6;
        for (i, step) in steps.iter_mut().enumerate() {
            if i == 8 && as_given {
                // The seed itself, not its round trip through OkHsl.
                *step = clamped;
                continue;
            }
            let hue = if i == 8 { h } else { shifted_hue(h, l[i] - l9) };
            let s = if i == 8 { s9 } else { sat(i) };
            *step = Okhsl { h: hue, s, l: l[i] }.to_color();
        }
        let contrast = text_on(steps[8], dark);
        Ramp { steps, contrast }
    }

    /// The scale as raw tokens: `<name>.1` to `<name>.12` and
    /// `<name>.contrast`, each a `#rrggbb` colour.
    pub fn tokens(&self, name: &str) -> Vec<(String, Json)> {
        let mut out: Vec<(String, Json)> = self
            .steps
            .iter()
            .enumerate()
            .map(|(i, c)| (format!("{name}.{}", i + 1), json!(c.to_hex())))
            .collect();
        out.push((format!("{name}.contrast"), json!(self.contrast.to_hex())));
        out
    }
}

/// The twelve steps generated from a seed for a scheme ([`Ramp::new`]).
pub fn ramp(seed: Color, scheme: Scheme) -> [Color; 12] {
    Ramp::new(seed, scheme).steps
}

/// OkHsl over Oklab, after Björn Ottosson's reference implementation
/// ("Okhsv and Okhsl", 2021, MIT).
mod ok {
    use std::f64::consts::PI;

    use crate::Color;

    fn linear(c: f32) -> f64 {
        let c = f64::from(c.clamp(0.0, 1.0));
        if c <= 0.040_45 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }

    fn linear_srgb_to_oklab(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
        let l = 0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b;
        let m = 0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b;
        let s = 0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b;
        let (l_, m_, s_) = (l.cbrt(), m.cbrt(), s.cbrt());
        (
            0.210_454_255_3 * l_ + 0.793_617_785_0 * m_ - 0.004_072_046_8 * s_,
            1.977_998_495_1 * l_ - 2.428_592_205_0 * m_ + 0.450_593_709_9 * s_,
            0.025_904_037_1 * l_ + 0.782_771_766_2 * m_ - 0.808_675_766_0 * s_,
        )
    }

    fn oklab_to_linear_srgb(l: f64, a: f64, b: f64) -> (f64, f64, f64) {
        let l_ = l + 0.396_337_777_4 * a + 0.215_803_757_3 * b;
        let m_ = l - 0.105_561_345_8 * a - 0.063_854_172_8 * b;
        let s_ = l - 0.089_484_177_5 * a - 1.291_485_548_0 * b;
        let (l3, m3, s3) = (l_.powi(3), m_.powi(3), s_.powi(3));
        (
            4.076_741_662_1 * l3 - 3.307_711_591_3 * m3 + 0.230_969_929_2 * s3,
            -1.268_438_004_6 * l3 + 2.609_757_401_1 * m3 - 0.341_319_396_5 * s3,
            -0.004_196_086_3 * l3 - 0.703_418_614_7 * m3 + 1.707_614_701_0 * s3,
        )
    }

    /// The most saturation (C / L) at a hue (`a`, `b` normalised) that
    /// stays in sRGB.
    fn compute_max_saturation(a: f64, b: f64) -> f64 {
        let (k0, k1, k2, k3, k4, wl, wm, ws) = if -1.881_703_28 * a - 0.809_364_93 * b > 1.0 {
            (
                1.190_862_77,
                1.765_767_28,
                0.596_626_41,
                0.755_151_97,
                0.567_712_45,
                4.076_741_662_1,
                -3.307_711_591_3,
                0.230_969_929_2,
            )
        } else if 1.814_441_04 * a - 1.194_452_76 * b > 1.0 {
            (
                0.739_565_15,
                -0.459_544_04,
                0.082_854_27,
                0.125_410_70,
                0.145_032_04,
                -1.268_438_004_6,
                2.609_757_401_1,
                -0.341_319_396_5,
            )
        } else {
            (
                1.357_336_52,
                -0.009_157_99,
                -1.151_302_10,
                -0.505_596_06,
                0.006_921_67,
                -0.004_196_086_3,
                -0.703_418_614_7,
                1.707_614_701_0,
            )
        };
        let mut s = k0 + k1 * a + k2 * b + k3 * a * a + k4 * a * b;
        let k_l = 0.396_337_777_4 * a + 0.215_803_757_3 * b;
        let k_m = -0.105_561_345_8 * a - 0.063_854_172_8 * b;
        let k_s = -0.089_484_177_5 * a - 1.291_485_548_0 * b;
        // One step of Halley's method.
        let l_ = 1.0 + s * k_l;
        let m_ = 1.0 + s * k_m;
        let s_ = 1.0 + s * k_s;
        let (l, m, ss) = (l_.powi(3), m_.powi(3), s_.powi(3));
        let (l_ds, m_ds, s_ds) = (
            3.0 * k_l * l_ * l_,
            3.0 * k_m * m_ * m_,
            3.0 * k_s * s_ * s_,
        );
        let (l_ds2, m_ds2, s_ds2) = (
            6.0 * k_l * k_l * l_,
            6.0 * k_m * k_m * m_,
            6.0 * k_s * k_s * s_,
        );
        let f = wl * l + wm * m + ws * ss;
        let f1 = wl * l_ds + wm * m_ds + ws * s_ds;
        let f2 = wl * l_ds2 + wm * m_ds2 + ws * s_ds2;
        s -= f * f1 / (f1 * f1 - 0.5 * f * f2);
        s
    }

    /// The lightness and chroma of the most saturated colour of a hue.
    fn find_cusp(a: f64, b: f64) -> (f64, f64) {
        let s_cusp = compute_max_saturation(a, b);
        let (r, g, bl) = oklab_to_linear_srgb(1.0, s_cusp * a, s_cusp * b);
        let l_cusp = (1.0 / r.max(g).max(bl)).cbrt();
        (l_cusp, l_cusp * s_cusp)
    }

    /// Where the line from (L0, 0) to (L1, C1) leaves the gamut, as `t`.
    fn find_gamut_intersection(a: f64, b: f64, l1: f64, c1: f64, l0: f64) -> f64 {
        let (cusp_l, cusp_c) = find_cusp(a, b);
        if ((l1 - l0) * cusp_c - (cusp_l - l0) * c1) <= 0.0 {
            return cusp_c * l0 / (c1 * cusp_l + cusp_c * (l0 - l1));
        }
        let mut t = cusp_c * (l0 - 1.0) / (c1 * (cusp_l - 1.0) + cusp_c * (l0 - l1));
        let dl = l1 - l0;
        let dc = c1;
        let k_l = 0.396_337_777_4 * a + 0.215_803_757_3 * b;
        let k_m = -0.105_561_345_8 * a - 0.063_854_172_8 * b;
        let k_s = -0.089_484_177_5 * a - 1.291_485_548_0 * b;
        let l_dt = dl + dc * k_l;
        let m_dt = dl + dc * k_m;
        let s_dt = dl + dc * k_s;
        let ll = l0 * (1.0 - t) + t * l1;
        let c = t * c1;
        let l_ = ll + c * k_l;
        let m_ = ll + c * k_m;
        let s_ = ll + c * k_s;
        let (l, m, s) = (l_.powi(3), m_.powi(3), s_.powi(3));
        let (ldt, mdt, sdt) = (
            3.0 * l_dt * l_ * l_,
            3.0 * m_dt * m_ * m_,
            3.0 * s_dt * s_ * s_,
        );
        let (ldt2, mdt2, sdt2) = (
            6.0 * l_dt * l_dt * l_,
            6.0 * m_dt * m_dt * m_,
            6.0 * s_dt * s_dt * s_,
        );
        let step = |w: (f64, f64, f64)| {
            let f = w.0 * l + w.1 * m + w.2 * s - 1.0;
            let f1 = w.0 * ldt + w.1 * mdt + w.2 * sdt;
            let f2 = w.0 * ldt2 + w.1 * mdt2 + w.2 * sdt2;
            let u = f1 / (f1 * f1 - 0.5 * f * f2);
            if u >= 0.0 { -f * u } else { f64::MAX }
        };
        let t_r = step((4.076_741_662_1, -3.307_711_591_3, 0.230_969_929_2));
        let t_g = step((-1.268_438_004_6, 2.609_757_401_1, -0.341_319_396_5));
        let t_b = step((-0.004_196_086_3, -0.703_418_614_7, 1.707_614_701_0));
        t += t_r.min(t_g).min(t_b);
        t
    }

    const K1: f64 = 0.206;
    const K2: f64 = 0.03;
    const K3: f64 = (1.0 + K1) / (1.0 + K2);

    fn toe(x: f64) -> f64 {
        0.5 * (K3 * x - K1 + ((K3 * x - K1) * (K3 * x - K1) + 4.0 * K2 * K3 * x).sqrt())
    }

    fn toe_inv(x: f64) -> f64 {
        (x * x + K1 * x) / (K3 * (x + K2))
    }

    fn st_mid(a: f64, b: f64) -> (f64, f64) {
        let s = 0.115_169_93
            + 1.0
                / (7.447_789_70
                    + 4.159_012_40 * b
                    + a * (-2.195_573_47
                        + 1.751_984_01 * b
                        + a * (-2.137_049_48 - 10.023_010_43 * b
                            + a * (-4.248_945_61 + 5.387_708_19 * b + 4.698_910_13 * a))));
        let t = 0.112_396_42
            + 1.0
                / (1.613_203_20 - 0.681_243_79 * b
                    + a * (0.403_706_12
                        + 0.901_481_23 * b
                        + a * (-0.270_879_43
                            + 0.612_239_90 * b
                            + a * (0.002_992_15 - 0.453_995_68 * b - 0.146_618_72 * a))));
        (s, t)
    }

    /// C at saturation 0 to 0.8 (C0), at 0.8 (Cmid) and at 1 (Cmax).
    fn cs(l: f64, a: f64, b: f64) -> (f64, f64, f64) {
        let (cusp_l, cusp_c) = find_cusp(a, b);
        let c_max = find_gamut_intersection(a, b, l, 1.0, l);
        let (st_s, st_t) = (cusp_c / cusp_l, cusp_c / (1.0 - cusp_l));
        let k = c_max / (l * st_s).min((1.0 - l) * st_t);
        let (mid_s, mid_t) = st_mid(a, b);
        let ca = l * mid_s;
        let cb = (1.0 - l) * mid_t;
        let c_mid = 0.9 * k * (1.0 / (1.0 / ca.powi(4) + 1.0 / cb.powi(4))).sqrt().sqrt();
        let ca = l * 0.4;
        let cb = (1.0 - l) * 0.8;
        let c_0 = (1.0 / (1.0 / (ca * ca) + 1.0 / (cb * cb))).sqrt();
        (c_0, c_mid, c_max)
    }

    pub(super) fn okhsl_to_srgb(h: f64, s: f64, l: f64) -> Color {
        if l >= 1.0 {
            return Color::WHITE;
        }
        if l <= 0.0 {
            return Color::BLACK;
        }
        let rad = h.to_radians();
        let (a_, b_) = (rad.cos(), rad.sin());
        let ll = toe_inv(l);
        let c = if s <= 0.0 {
            0.0
        } else {
            let (c_0, c_mid, c_max) = cs(ll, a_, b_);
            const MID: f64 = 0.8;
            const MID_INV: f64 = 1.25;
            if s < MID {
                let t = MID_INV * s;
                let k1 = MID * c_0;
                let k2 = 1.0 - k1 / c_mid;
                t * k1 / (1.0 - k2 * t)
            } else {
                let t = (s - MID) / (1.0 - MID);
                let k0 = c_mid;
                let k1 = (1.0 - MID) * c_mid * c_mid * MID_INV * MID_INV / c_0;
                let k2 = 1.0 - k1 / (c_max - c_mid);
                k0 + t * k1 / (1.0 - k2 * t)
            }
        };
        let (r, g, b) = oklab_to_linear_srgb(ll, c * a_, c * b_);
        Color::from_linear(r as f32, g as f32, b as f32, 1.0)
    }

    pub(super) fn srgb_to_okhsl(c: Color) -> (f64, f64, f64) {
        let (ll, a, b) = linear_srgb_to_oklab(linear(c.r), linear(c.g), linear(c.b));
        let l = toe(ll).clamp(0.0, 1.0);
        let chroma = (a * a + b * b).sqrt();
        if chroma < 1e-6 || ll <= 1e-6 || ll >= 1.0 - 1e-6 {
            return (0.0, 0.0, l);
        }
        let (a_, b_) = (a / chroma, b / chroma);
        let h = (0.5 + 0.5 * (-b).atan2(-a) / PI) * 360.0;
        let (c_0, c_mid, c_max) = cs(ll, a_, b_);
        const MID: f64 = 0.8;
        const MID_INV: f64 = 1.25;
        let s = if chroma < c_mid {
            let k1 = MID * c_0;
            let k2 = 1.0 - k1 / c_mid;
            let t = chroma / (k1 + k2 * chroma);
            t * MID
        } else {
            let k0 = c_mid;
            let k1 = (1.0 - MID) * c_mid * c_mid * MID_INV * MID_INV / c_0;
            let k2 = 1.0 - k1 / (c_max - c_mid);
            let t = (chroma - k0) / (k1 + k2 * (chroma - k0));
            MID + (1.0 - MID) * t
        };
        (h.rem_euclid(360.0), s.clamp(0.0, 1.0), l)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeds() -> Vec<Color> {
        let mut out = Vec::new();
        for h in (0..360).step_by(15) {
            for s in [0.35, 0.7, 1.0] {
                for l in [0.3, 0.5, 0.65, 0.8] {
                    out.push(Okhsl { h: h as f32, s, l }.to_color());
                }
            }
        }
        for hex in [
            "#000000", "#ffffff", "#808080", "#ffff00", "#00ff00", "#0000ff",
        ] {
            out.push(Color::from_hex(hex).unwrap());
        }
        out
    }

    #[test]
    fn okhsl_round_trips() {
        for hex in [
            "#3a7bd5", "#f0c060", "#2e64a6", "#c0392b", "#27ae60", "#808080",
        ] {
            let c = Color::from_hex(hex).unwrap();
            let back = Okhsl::from_color(c).to_color();
            assert_eq!(back.to_hex(), hex, "{:?}", Okhsl::from_color(c));
        }
        let white = Okhsl::from_color(Color::WHITE);
        assert!(white.s < 1e-3 && (white.l - 1.0).abs() < 1e-3, "{white:?}");
        // Saturation 1 stays in the gamut.
        for h in (0..360).step_by(10) {
            for l in [0.2, 0.5, 0.8] {
                let c = Okhsl {
                    h: h as f32,
                    s: 1.0,
                    l,
                }
                .to_color();
                let back = Okhsl::from_color(c);
                assert!(back.s > 0.9, "h {h} l {l}: {back:?}");
            }
        }
    }

    #[test]
    fn contrast_targets_are_met_across_hues_in_both_schemes() {
        for scheme in [Scheme::Light, Scheme::Dark] {
            for seed in seeds() {
                let r = Ramp::new(seed, scheme);
                let s = &r.steps;
                let what = format!("{} {scheme:?}", seed.to_hex());
                // Text on the solid and on its hover, by both measures.
                for step in [8, 9] {
                    let ratio = contrast_ratio(r.contrast, s[step]);
                    assert!(ratio >= 4.5, "{what}: contrast on {}: {ratio}", step + 1);
                }
                let lc = apca::lc(r.contrast, s[8]).abs();
                assert!(lc >= SOLID_LC, "{what}: Lc on 9: {lc}");
                // The solid as text on the backgrounds (egui-pop's accent
                // is drawn as text on panels).
                for bg in [0, 1] {
                    let ratio = contrast_ratio(s[8], s[bg]);
                    assert!(ratio >= 4.5, "{what}: 9 on {}: {ratio}", bg + 1);
                }
                // Low- and high-contrast text on the backgrounds.
                for bg in [0, 1] {
                    let low = contrast_ratio(s[10], s[bg]);
                    let high = contrast_ratio(s[11], s[bg]);
                    assert!(low >= 4.5, "{what}: 11 on {}: {low}", bg + 1);
                    assert!(high >= 7.0, "{what}: 12 on {}: {high}", bg + 1);
                }
                // Borders that must be seen: step 8 on the backgrounds.
                let ui = contrast_ratio(s[7], s[1]);
                assert!(ui >= 3.0, "{what}: 8 on 2: {ui}");
                // Backgrounds and borders run one way.
                let lum: Vec<f32> = s[..7].iter().map(|c| c.relative_luminance()).collect();
                for w in lum.windows(2) {
                    match scheme {
                        Scheme::Light => assert!(w[0] > w[1], "{what}: {lum:?}"),
                        Scheme::Dark => assert!(w[0] < w[1], "{what}: {lum:?}"),
                    }
                }
                assert_eq!(scheme == Scheme::Dark, s[0].is_dark(), "{what}");
            }
        }
    }

    #[test]
    fn ramps_are_deterministic() {
        for seed in seeds() {
            for scheme in [Scheme::Light, Scheme::Dark] {
                assert_eq!(Ramp::new(seed, scheme), Ramp::new(seed, scheme));
            }
        }
    }

    #[test]
    fn a_grey_seed_gives_a_neutral_ramp() {
        for scheme in [Scheme::Light, Scheme::Dark] {
            for c in ramp(Color::from_hex("#7a7a7a").unwrap(), scheme) {
                let [r, g, b, _] = c.to_srgba_u8();
                assert!(r.abs_diff(g) <= 1 && g.abs_diff(b) <= 1, "{c}");
            }
        }
    }

    #[test]
    fn a_usable_seed_is_the_dark_solid_or_close_to_it() {
        // A mid blue reads with dark text in dark: it is step 9 as given.
        let seed = Color::from_hex("#8cc0ff").unwrap();
        assert!(is_usable_seed(seed));
        let dark = ramp(seed, Scheme::Dark);
        assert_eq!(dark[8].to_hex(), seed.to_hex());
        // In light it is darkened until white text reads, its hue kept.
        let light = ramp(seed, Scheme::Light);
        let (a, b) = (Okhsl::from_color(seed), Okhsl::from_color(light[8]));
        assert!(b.l < a.l && (a.h - b.h).abs() < 1.0, "{a:?} {b:?}");
    }

    #[test]
    fn seeds_out_of_range_are_clamped() {
        for hex in ["#000000", "#ffffff", "#020304", "#fefefe"] {
            let seed = Color::from_hex(hex).unwrap();
            assert!(!is_usable_seed(seed), "{hex}");
            let l = Okhsl::from_color(clamp_seed(seed)).l;
            assert!(
                l >= SEED_LIGHTNESS.0 - 1e-3 && l <= SEED_LIGHTNESS.1 + 1e-3,
                "{hex}: {l}"
            );
        }
        let neon = Color::from_hex("#00ff66").unwrap();
        assert!(Okhsl::from_color(clamp_seed(neon)).s <= 0.86);
    }

    #[test]
    fn writes_a_scale_as_tokens() {
        let t = Ramp::new(Color::from_hex("#c0392b").unwrap(), Scheme::Light).tokens("brick");
        assert_eq!(t.len(), 13);
        assert_eq!(t[0].0, "brick.1");
        assert_eq!(t[12].0, "brick.contrast");
        assert!(
            t.iter()
                .all(|(_, v)| v.as_str().is_some_and(|s| s.starts_with('#')))
        );
    }
}
