//! APCA lightness contrast (Lc), informative only.
//!
//! The Accessible Perceptual Contrast Algorithm, as published with the
//! WCAG 3 working draft (APCA-W3, constants of version 0.0.98G): a
//! contrast that depends on which colour is the text and which the
//! background, and on their polarity. Lc runs from about -108 (white
//! text on black) to about 106 (black text on white); its sign says the
//! polarity (positive: dark text on a light background), its size how
//! readable the text is (Lc 60 is the floor for ordinary text, Lc 75 for
//! body text, Lc 45 for large text and solid interface parts).
//!
//! egui-pop's theme contract stays on WCAG 2.x ([`crate::contrast`]):
//! Lc is used where a direction must be chosen, to pick the text colour
//! of an accent fill and to set the fill's lightness
//! ([`crate::palette`]), and shown beside the WCAG ratio. This is a
//! reimplementation of the published formula; egui_colors (MIT) does
//! the same (`estimate_lc`).

use crate::Color;

// The exponents of the four polarity cases.
const NORM_BG: f64 = 0.56;
const NORM_TXT: f64 = 0.57;
const REV_TXT: f64 = 0.62;
const REV_BG: f64 = 0.65;
// The soft clamp of near-black luminances.
const BLACK_THRESHOLD: f64 = 0.022;
const BLACK_CLAMP: f64 = 1.414;
// Scaling, offsets and the low clip.
const SCALE: f64 = 1.14;
const LOW_OFFSET: f64 = 0.027;
const LOW_CLIP: f64 = 0.1;
const DELTA_Y_MIN: f64 = 0.0005;

/// The screen luminance APCA reads: each sRGB channel raised to 2.4 (no
/// linear segment), weighted as sRGB's luminance.
fn screen_y(c: Color) -> f64 {
    let ch = |v: f32| f64::from(v.clamp(0.0, 1.0)).powf(2.4);
    0.212_672_9 * ch(c.r) + 0.715_152_2 * ch(c.g) + 0.072_175_0 * ch(c.b)
}

fn soft_clamp(y: f64) -> f64 {
    if y > BLACK_THRESHOLD {
        y
    } else {
        y + (BLACK_THRESHOLD - y).powf(BLACK_CLAMP)
    }
}

/// The lightness contrast of `text` over `background`, alpha ignored
/// (a translucent text is first painted over the background). Positive
/// for dark text on a light background, negative for light text on a
/// dark one; 0 when the two are too close to tell.
///
/// ```
/// use egui_pop_tokens::{Color, apca};
///
/// let lc = apca::lc(Color::BLACK, Color::WHITE);
/// assert!((lc - 106.04).abs() < 0.1);
/// assert!(apca::lc(Color::WHITE, Color::BLACK) < -107.0);
/// ```
pub fn lc(text: Color, background: Color) -> f32 {
    let background = Color {
        a: 1.0,
        ..background
    };
    let txt = soft_clamp(screen_y(text.over(background)));
    let bg = soft_clamp(screen_y(background));
    if (bg - txt).abs() < DELTA_Y_MIN {
        return 0.0;
    }
    let out = if bg > txt {
        let c = (bg.powf(NORM_BG) - txt.powf(NORM_TXT)) * SCALE;
        if c < LOW_CLIP { 0.0 } else { c - LOW_OFFSET }
    } else {
        let c = (bg.powf(REV_BG) - txt.powf(REV_TXT)) * SCALE;
        if c > -LOW_CLIP { 0.0 } else { c + LOW_OFFSET }
    };
    (out * 100.0) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Color {
        Color::from_hex(s).unwrap()
    }

    #[test]
    fn matches_the_published_reference_values() {
        // The reference pairs given with APCA-W3 0.0.98G.
        let cases = [
            ("#888888", "#ffffff", 63.06),
            ("#ffffff", "#888888", -68.54),
            ("#000000", "#aaaaaa", 58.15),
            ("#aaaaaa", "#000000", -56.24),
            ("#112233", "#ddeeff", 91.67),
            ("#ddeeff", "#112233", -93.07),
            ("#112233", "#444444", 8.32),
            ("#444444", "#112233", -7.52),
        ];
        for (text, bg, want) in cases {
            let got = lc(hex(text), hex(bg));
            assert!(
                (got - want).abs() < 0.2,
                "{text} on {bg}: {got} (want {want})"
            );
        }
    }

    #[test]
    fn the_same_colour_has_no_contrast() {
        assert_eq!(lc(hex("#808080"), hex("#808080")), 0.0);
    }
}
