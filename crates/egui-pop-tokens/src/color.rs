//! Colours: sRGB with straight alpha, read from DTCG colour values, and the
//! WCAG 2.x contrast ratio.

use std::fmt;

/// An sRGB colour with straight (not premultiplied) alpha, each channel
/// from 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const BLACK: Color = Color::rgb(0.0, 0.0, 0.0);
    pub const WHITE: Color = Color::rgb(1.0, 1.0, 1.0);
    /// What a missing colour shows as: loud on purpose.
    pub const MISSING: Color = Color::rgb(1.0, 0.0, 1.0);

    /// An opaque colour.
    pub const fn rgb(r: f32, g: f32, b: f32) -> Color {
        Color { r, g, b, a: 1.0 }
    }

    /// Reads `#rgb`, `#rrggbb` or `#rrggbbaa`.
    pub fn from_hex(text: &str) -> Option<Color> {
        let h = text.strip_prefix('#')?;
        if !h.is_ascii() {
            return None;
        }
        let byte = |i: usize| {
            u8::from_str_radix(h.get(i..i + 2)?, 16)
                .ok()
                .map(|b| f32::from(b) / 255.0)
        };
        let nibble = |i: usize| {
            u8::from_str_radix(h.get(i..=i)?, 16)
                .ok()
                .map(|b| f32::from(b * 17) / 255.0)
        };
        match h.len() {
            3 => Some(Color::rgb(nibble(0)?, nibble(1)?, nibble(2)?)),
            6 => Some(Color::rgb(byte(0)?, byte(2)?, byte(4)?)),
            8 => Some(Color {
                a: byte(6)?,
                ..Color::rgb(byte(0)?, byte(2)?, byte(4)?)
            }),
            _ => None,
        }
    }

    /// From OKLCH: lightness 0 to 1, chroma, hue in degrees. Out-of-gamut
    /// colours are clipped to sRGB.
    pub fn from_oklch(l: f32, c: f32, h: f32, alpha: f32) -> Color {
        let (hs, hc) = f64::from(h).to_radians().sin_cos();
        let c = f64::from(c);
        Color::from_oklab(f64::from(l), c * hc, c * hs, alpha)
    }

    /// From OKLab: lightness 0 to 1, a and b axes.
    pub fn from_oklab(l: f64, a: f64, b: f64, alpha: f32) -> Color {
        let l_ = l + 0.396_337_777_4 * a + 0.215_803_757_3 * b;
        let m_ = l - 0.105_561_345_8 * a - 0.063_854_172_8 * b;
        let s_ = l - 0.089_484_177_5 * a - 1.291_485_548_0 * b;
        let (l3, m3, s3) = (l_.powi(3), m_.powi(3), s_.powi(3));
        let r = 4.076_741_662_1 * l3 - 3.307_711_591_3 * m3 + 0.230_969_929_2 * s3;
        let g = -1.268_438_004_6 * l3 + 2.609_757_401_1 * m3 - 0.341_319_396_5 * s3;
        let bl = -0.004_196_086_3 * l3 - 0.703_418_614_7 * m3 + 1.707_614_701_0 * s3;
        Color {
            r: encode(r),
            g: encode(g),
            b: encode(bl),
            a: alpha,
        }
    }

    /// From linear-light sRGB channels.
    pub fn from_linear(r: f32, g: f32, b: f32, alpha: f32) -> Color {
        Color {
            r: encode(f64::from(r)),
            g: encode(f64::from(g)),
            b: encode(f64::from(b)),
            a: alpha,
        }
    }

    /// The channels as bytes, `[r, g, b, a]`, straight alpha.
    pub fn to_srgba_u8(self) -> [u8; 4] {
        let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        [b(self.r), b(self.g), b(self.b), b(self.a)]
    }

    /// `#rrggbb`, or `#rrggbbaa` when not opaque.
    pub fn to_hex(self) -> String {
        let [r, g, b, a] = self.to_srgba_u8();
        if a == 255 {
            format!("#{r:02x}{g:02x}{b:02x}")
        } else {
            format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
        }
    }

    /// This colour painted over `background`, which is taken as opaque.
    pub fn over(self, background: Color) -> Color {
        let a = self.a.clamp(0.0, 1.0);
        let mix = |f: f32, b: f32| f * a + b * (1.0 - a);
        Color::rgb(
            mix(self.r, background.r),
            mix(self.g, background.g),
            mix(self.b, background.b),
        )
    }

    /// WCAG 2.x relative luminance of the colour, alpha ignored.
    pub fn relative_luminance(self) -> f32 {
        let lin = |c: f32| {
            let c = f64::from(c.clamp(0.0, 1.0));
            if c <= 0.040_45 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        (0.2126 * lin(self.r) + 0.7152 * lin(self.g) + 0.0722 * lin(self.b)) as f32
    }

    /// Whether the colour reads as dark (relative luminance under that of
    /// a middle grey).
    pub fn is_dark(self) -> bool {
        self.relative_luminance() < 0.18
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// Linear light to the sRGB transfer curve, clipped to 0..1.
fn encode(x: f64) -> f32 {
    let x = x.clamp(0.0, 1.0);
    let v = if x <= 0.003_130_8 {
        12.92 * x
    } else {
        1.055 * x.powf(1.0 / 2.4) - 0.055
    };
    v as f32
}

/// The WCAG 2.x contrast ratio of a foreground over a background, from 1
/// to 21. A translucent foreground is first painted over the background;
/// the background is taken as opaque.
pub fn contrast_ratio(foreground: Color, background: Color) -> f32 {
    let background = Color {
        a: 1.0,
        ..background
    };
    let fg = foreground.over(background).relative_luminance();
    let bg = background.relative_luminance();
    let (hi, lo) = if fg > bg { (fg, bg) } else { (bg, fg) };
    (hi + 0.05) / (lo + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips() {
        for text in ["#000000", "#f0c060", "#12345678"] {
            assert_eq!(Color::from_hex(text).unwrap().to_hex(), text);
        }
        assert_eq!(Color::from_hex("#fff"), Some(Color::WHITE));
        assert_eq!(Color::from_hex("f0c060"), None);
        assert_eq!(Color::from_hex("#f0c06"), None);
        assert_eq!(Color::from_hex("#gg0000"), None);
    }

    #[test]
    fn contrast_matches_wcag_reference_values() {
        assert!((contrast_ratio(Color::BLACK, Color::WHITE) - 21.0).abs() < 1e-3);
        assert!((contrast_ratio(Color::WHITE, Color::WHITE) - 1.0).abs() < 1e-6);
        // #767676 on white is the classic 4.54:1.
        let grey = Color::from_hex("#767676").unwrap();
        assert!((contrast_ratio(grey, Color::WHITE) - 4.54).abs() < 0.01);
        // Symmetric.
        assert_eq!(
            contrast_ratio(grey, Color::BLACK),
            contrast_ratio(Color::BLACK, grey)
        );
    }

    #[test]
    fn translucent_foregrounds_are_composited_first() {
        let half_white = Color {
            a: 0.5,
            ..Color::WHITE
        };
        let r = contrast_ratio(half_white, Color::BLACK);
        assert!(r > 1.0 && r < 21.0 / 2.0, "{r}");
    }

    #[test]
    fn oklch_converts_to_srgb() {
        assert_eq!(Color::from_oklch(1.0, 0.0, 0.0, 1.0).to_hex(), "#ffffff");
        assert_eq!(Color::from_oklch(0.0, 0.0, 0.0, 1.0).to_hex(), "#000000");
        // The gold accent of pop-dark, written in OKLCH in its theme file.
        assert_eq!(
            Color::from_oklch(0.832, 0.126, 83.0, 1.0).to_hex(),
            "#f0c060"
        );
    }
}
