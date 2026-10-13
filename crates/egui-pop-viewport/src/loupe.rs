//! The loupe: the pixels under the pointer, magnified, in a square that
//! follows it.
//!
//! It shows **displayed pixels**: the viewport's own tile textures,
//! after the display transform, drawn again through a magnifying
//! mapping and clipped to the loupe. On the GPU path the half-float
//! tiles are sampled in a second paint callback (the same shader, its
//! own uniforms), with no read back to the CPU; on the CPU path the
//! tiles' egui textures (nearest filtering) are drawn again. Each
//! displayed pixel (an image pixel from 100 % up, a screen pixel below)
//! becomes a square of `magnification` points, crisp, with an optional
//! grid, a marker around the centre pixel and a readout of its values:
//! as displayed (RGB, 0 to 255, and Lab) or as the source gave them
//! (the scene values).
//!
//! The loupe sits beside the pointer, above and to the right, and flips
//! at the viewport's edges. It is turned on by the application (a tool
//! option: [`ViewportState::loupe_mut`](crate::ViewportState::loupe_mut))
//! or by a key ([`Loupe::key`]); the viewport signals each toggle
//! ([`Signal::Loupe`](crate::Signal::Loupe)).

use egui::{Key, Pos2, Rect, Vec2, pos2, vec2};

use crate::display::Stage;
use crate::source::Readout;
use crate::view::Mapping;

/// The smallest magnification.
pub const MIN_MAGNIFICATION: u32 = 4;
/// The largest magnification.
pub const MAX_MAGNIFICATION: u32 = 16;

/// What the loupe's readout shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Values {
    /// No readout.
    Off,
    /// The values as displayed: RGB (0 to 255) and Lab, through the
    /// display transform.
    #[default]
    Display,
    /// The values the source gave (scene-referred, as named by it).
    Scene,
}

impl Values {
    pub fn name(self) -> &'static str {
        match self {
            Values::Off => "off",
            Values::Display => "display",
            Values::Scene => "scene",
        }
    }
}

/// A viewport's loupe settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Loupe {
    /// Shown while the pointer is over the viewport.
    pub on: bool,
    /// The side of one displayed pixel in the loupe, in points: 4 to 16.
    pub magnification: u32,
    /// The side of the loupe's picture, in points.
    pub side: f32,
    /// Lines between the displayed pixels.
    pub grid: bool,
    /// A square around the centre pixel (the one under the pointer).
    pub marker: bool,
    pub values: Values,
    /// The key that turns it on and off while the viewport is hovered or
    /// focused (`None`: no key; the application toggles `on`).
    pub key: Option<Key>,
    /// The space between the pointer and the loupe, in points.
    pub offset: f32,
}

impl Default for Loupe {
    fn default() -> Self {
        Loupe {
            on: false,
            magnification: 8,
            side: 144.0,
            grid: true,
            marker: true,
            values: Values::Display,
            key: None,
            offset: 24.0,
        }
    }
}

impl Loupe {
    /// The magnification, within 4 to 16.
    pub fn magnification(&self) -> u32 {
        self.magnification
            .clamp(MIN_MAGNIFICATION, MAX_MAGNIFICATION)
    }
}

/// Where a loupe of `size` goes for a pointer: `offset` points above and
/// to the right of it, flipped to the left at the right edge of
/// `bounds` and below at its top, and kept inside when it does not fit
/// either way.
pub fn place(pointer: Pos2, size: Vec2, bounds: Rect, offset: f32) -> Rect {
    let mut x = pointer.x + offset;
    if x + size.x > bounds.right() {
        x = pointer.x - offset - size.x;
    }
    let mut y = pointer.y - offset - size.y;
    if y < bounds.top() {
        y = pointer.y + offset;
    }
    let x = x.clamp(bounds.left(), (bounds.right() - size.x).max(bounds.left()));
    let y = y.clamp(bounds.top(), (bounds.bottom() - size.y).max(bounds.top()));
    Rect::from_min_size(pos2(x, y), size)
}

/// How the loupe magnifies: what is at window physical pixel `p` lands
/// at `at + (p - center) × scale`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sampling {
    /// The centre of the displayed pixel under the pointer, in window
    /// physical pixels.
    pub center: [f64; 2],
    /// The side of a displayed pixel on screen, in physical pixels: the
    /// zoom from 100 % up (an image pixel), else 1 (a screen pixel).
    pub texel: f64,
    /// The side of a displayed pixel in the loupe, in physical pixels: a
    /// whole number.
    pub block: f64,
    /// The loupe's centre, in window physical pixels, placed so that its
    /// blocks fall on whole pixels.
    pub at: [f64; 2],
    /// The image pixel under the pointer, when over the image.
    pub pixel: Option<[i64; 2]>,
}

impl Sampling {
    /// The sampling for a pointer (points) over a mapping, at a
    /// magnification, the loupe centred near `loupe_center` (points).
    pub fn new(m: &Mapping, pointer: Pos2, magnification: u32, loupe_center: Pos2) -> Sampling {
        let ppp = f64::from(m.ppp);
        let p = [f64::from(pointer.x) * ppp, f64::from(pointer.y) * ppp];
        let screen = [p[0].floor() + 0.5, p[1].floor() + 0.5];
        let pixel = m.pixel_at(pointer);
        let (center, texel) = if m.zoom >= 1.0 {
            let i = m.px_to_image(screen);
            let c = m.image_to_px([i[0].floor() + 0.5, i[1].floor() + 0.5]);
            (c, m.zoom)
        } else {
            (screen, 1.0)
        };
        let block = (f64::from(magnification) * ppp).round().max(1.0);
        let raw = [
            f64::from(loupe_center.x) * ppp,
            f64::from(loupe_center.y) * ppp,
        ];
        let snap = |v: f64| (v - block / 2.0).round() + block / 2.0;
        Sampling {
            center,
            texel,
            block,
            at: [snap(raw[0]), snap(raw[1])],
            pixel,
        }
    }

    /// Loupe pixels per screen pixel.
    pub fn scale(&self) -> f64 {
        self.block / self.texel
    }

    /// A window physical position, magnified into the loupe.
    pub fn map(&self, p: [f64; 2]) -> [f64; 2] {
        let s = self.scale();
        [
            self.at[0] + (p[0] - self.center[0]) * s,
            self.at[1] + (p[1] - self.center[1]) * s,
        ]
    }

    /// A rectangle in window physical pixels (`[x0, y0, x1, y1]`),
    /// magnified into the loupe.
    pub fn map_rect(&self, r: [f64; 4]) -> [f64; 4] {
        let a = self.map([r[0], r[1]]);
        let b = self.map([r[2], r[3]]);
        [a[0], a[1], b[0], b[1]]
    }

    /// The centre pixel's square in the loupe, in physical pixels.
    pub fn center_block(&self) -> [f64; 4] {
        let h = self.block / 2.0;
        [
            self.at[0] - h,
            self.at[1] - h,
            self.at[0] + h,
            self.at[1] + h,
        ]
    }

    /// The grid's lines across a loupe rectangle (physical pixels): the
    /// positions of the vertical lines, then of the horizontal ones.
    pub fn grid_lines(&self, r: [f64; 4]) -> (Vec<f64>, Vec<f64>) {
        let lines = |from: f64, to: f64, at: f64| {
            let first = ((from - (at - self.block / 2.0)) / self.block).ceil();
            let mut v = Vec::new();
            let mut k = first;
            loop {
                let x = at - self.block / 2.0 + k * self.block;
                if x > to || v.len() > 512 {
                    break;
                }
                v.push(x);
                k += 1.0;
            }
            v
        };
        (lines(r[0], r[2], self.at[0]), lines(r[1], r[3], self.at[1]))
    }
}

/// A scene colour (the source's `R`, `G` and `B`, when it names them) as
/// displayed: through the display stage, in 8 bits.
pub fn displayed(readout: &Readout, stage: &Stage) -> Option<[u8; 3]> {
    let get = |name: &str| {
        readout
            .values
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| *v)
    };
    Some(stage.encode_8bit([get("R")?, get("G")?, get("B")?]))
}

/// CIE L*a*b* (D65) of an 8-bit sRGB colour.
pub fn lab_from_srgb8(rgb: [u8; 3]) -> [f32; 3] {
    let lin = |v: u8| {
        let c = f32::from(v) / 255.0;
        if c <= 0.040_45 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let [r, g, b] = rgb.map(lin);
    let x = 0.412_456_4 * r + 0.357_576_1 * g + 0.180_437_5 * b;
    let y = 0.212_672_9 * r + 0.715_152_2 * g + 0.072_175 * b;
    let z = 0.019_333_9 * r + 0.119_192 * g + 0.950_304_1 * b;
    let f = |t: f32| {
        let d = 6.0_f32 / 29.0;
        if t > d * d * d {
            t.cbrt()
        } else {
            t / (3.0 * d * d) + 4.0 / 29.0
        }
    };
    let (fx, fy, fz) = (f(x / 0.950_47), f(y), f(z / 1.088_83));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

/// The readout's lines for a pixel: its coordinates and the mode, then
/// its values as asked (`none` when the source gave nothing usable).
pub fn readout_lines(
    pixel: Option<[i64; 2]>,
    readout: Option<&Readout>,
    values: Values,
    stage: Option<&Stage>,
    labels: [&str; 3],
) -> Vec<String> {
    let [display, scene, none] = labels;
    let mut out = Vec::new();
    let mode = match values {
        Values::Off => return out,
        Values::Display => display,
        Values::Scene => scene,
    };
    match pixel {
        Some(p) => out.push(format!("{}, {}  {mode}", p[0], p[1])),
        None => {
            out.push(none.to_string());
            return out;
        }
    }
    let readout = readout.filter(|r| Some(r.pixel) == pixel);
    match (values, readout) {
        (Values::Display, Some(r)) => match stage.and_then(|s| displayed(r, s)) {
            Some(rgb) => {
                let lab = lab_from_srgb8(rgb);
                out.push(format!("RGB {} {} {}", rgb[0], rgb[1], rgb[2]));
                // Rounded first, so a grey reads 0, never -0.
                let [l, a, b] = lab.map(|v| if v.abs() < 0.5 { 0.0 } else { v.round() });
                out.push(format!("Lab {l:.0} {a:.0} {b:.0}"));
            }
            None => out.push(none.to_string()),
        },
        (Values::Scene, Some(r)) if !r.values.is_empty() => {
            let mut s = String::new();
            for (name, v) in &r.values {
                if !s.is_empty() {
                    s.push_str("  ");
                }
                s.push_str(&format!("{name} {v:.3}"));
            }
            out.push(s);
        }
        _ => out.push(none.to_string()),
    }
    out
}

/// What the loupe showed in the last frame (for tests and the debug
/// bridge).
#[derive(Clone, Debug, PartialEq)]
pub struct LoupeFrame {
    /// The whole loupe, in points.
    pub rect: Rect,
    /// Its picture, in points.
    pub picture: Rect,
    pub sampling: Sampling,
    /// Tiles drawn again in it.
    pub tiles: usize,
    /// On the GPU path (else the CPU's textures).
    pub gpu: bool,
    /// The readout's lines.
    pub lines: Vec<String>,
}

/// Points from physical pixels.
pub(crate) fn to_points(r: [f64; 4], ppp: f32) -> Rect {
    let s = f64::from(ppp);
    Rect::from_min_max(
        pos2((r[0] / s) as f32, (r[1] / s) as f32),
        pos2((r[2] / s) as f32, (r[3] / s) as f32),
    )
}

/// The picture's side and the loupe's padding, snapped to whole physical
/// pixels.
pub(crate) fn picture_size(l: &Loupe, ppp: f32) -> Vec2 {
    let side = egui_pop_core::scale::snap_size(l.side.max(32.0), ppp);
    vec2(side, side)
}
