//! The display transform: the last step from the source's pixels to the
//! window, where the application plugs in its colour management.
//!
//! - A [`DisplayTarget`] is an encoding a surface expects: primaries,
//!   transfer, the luminance of SDR white, the headroom above it, the
//!   surface format. A window's surface has one ([`set_target`],
//!   [`target`]; sRGB until said otherwise, and the GPU path records what
//!   eframe configured when it is installed).
//! - The application may **want** another for a window ([`want`]): Display
//!   P3, HDR with PQ or HLG. The viewport then draws for what the surface
//!   can carry and says why when that is less ([`resolve`],
//!   [`Resolution`]): an application declaring a P3 or HDR target gets
//!   faithful output where the platform allows it, and a correct sRGB
//!   fallback with the reason otherwise.
//! - A [`DisplayTransform`] (the application's, [`LinearSrgb`] by
//!   default) turns scene pixels into a [`Stage`] for a target: a 3×3
//!   matrix from the working space to the target's primaries (computed
//!   from their chromaticities, [`Primaries::conversion`]), a clamp, and
//!   the target's transfer (sRGB, 2.2, extended linear or extended sRGB,
//!   SMPTE ST 2084 PQ, BT.2100 HLG). The GPU path runs the stage in its
//!   shader; the CPU path ([`apply_cpu`]) runs the same arithmetic
//!   ([`Stage::encode`]) and writes 8-bit sRGB textures, so it always
//!   draws sRGB.
//! - A source that does its own transform on the CPU delivers display
//!   pixels (`Pixels::Display` of the tile source) and skips this.
//!
//! ```
//! use egui_pop_viewport::display::{DisplayTarget, DisplayTransform, LinearSrgb, Transfer};
//!
//! let stage = LinearSrgb.stage(&DisplayTarget::srgb());
//! // Linear 0.5 is sRGB-encoded 188.
//! assert_eq!(stage.encode_8bit([0.5, 0.5, 0.5]), [188, 188, 188]);
//! // SDR white at 203 nits is 58 % of the PQ signal (BT.2408).
//! assert!((Transfer::Pq.encode_at(1.0, 203.0) - 0.5807).abs() < 1e-3);
//! ```

use egui::{Color32, Context, ViewportId};
use egui_pop_core::state::Slot;

/// The primaries (and white) of an encoding.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Primaries {
    /// BT.709 / sRGB.
    Srgb,
    DisplayP3,
    /// BT.2020 (and BT.2100).
    Bt2020,
    /// Chromaticities `[x, y]` of red, green, blue and white.
    Custom {
        r: [f64; 2],
        g: [f64; 2],
        b: [f64; 2],
        w: [f64; 2],
    },
}

/// The D65 white point.
pub const D65: [f64; 2] = [0.3127, 0.3290];

type M3 = [[f64; 3]; 3];

fn mul(a: &M3, b: &M3) -> M3 {
    let mut out = [[0.0; 3]; 3];
    for (i, row) in out.iter_mut().enumerate() {
        for (j, v) in row.iter_mut().enumerate() {
            *v = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    out
}

fn inverse(m: &M3) -> M3 {
    let [[a, b, c], [d, e, f], [g, h, i]] = *m;
    let co = [
        e * i - f * h,
        -(d * i - f * g),
        d * h - e * g,
        -(b * i - c * h),
        a * i - c * g,
        -(a * h - b * g),
        b * f - c * e,
        -(a * f - c * d),
        a * e - b * d,
    ];
    let det = a * co[0] + b * co[1] + c * co[2];
    // The adjugate is the transposed cofactor matrix.
    [
        [co[0] / det, co[3] / det, co[6] / det],
        [co[1] / det, co[4] / det, co[7] / det],
        [co[2] / det, co[5] / det, co[8] / det],
    ]
}

fn xyz(c: [f64; 2]) -> [f64; 3] {
    [c[0] / c[1], 1.0, (1.0 - c[0] - c[1]) / c[1]]
}

impl Primaries {
    /// Red, green, blue and white chromaticities `[x, y]`.
    pub fn chromaticities(&self) -> [[f64; 2]; 4] {
        match *self {
            Primaries::Srgb => [[0.64, 0.33], [0.30, 0.60], [0.15, 0.06], D65],
            Primaries::DisplayP3 => [[0.680, 0.320], [0.265, 0.690], [0.150, 0.060], D65],
            Primaries::Bt2020 => [[0.708, 0.292], [0.170, 0.797], [0.131, 0.046], D65],
            Primaries::Custom { r, g, b, w } => [r, g, b, w],
        }
    }

    /// Linear RGB in these primaries to CIE XYZ (white at `Y = 1`).
    pub fn to_xyz(&self) -> [[f64; 3]; 3] {
        let [r, g, b, w] = self.chromaticities();
        let (r, g, b) = (xyz(r), xyz(g), xyz(b));
        let p = [[r[0], g[0], b[0]], [r[1], g[1], b[1]], [r[2], g[2], b[2]]];
        let white = xyz(w);
        let inv = inverse(&p);
        let s: [f64; 3] = [0, 1, 2].map(|i| (0..3).map(|k| inv[i][k] * white[k]).sum());
        [0, 1, 2].map(|i| [0, 1, 2].map(|j| p[i][j] * s[j]))
    }

    /// The matrix from linear RGB in `from` to linear RGB in `to` (no
    /// chromatic adaptation: the named primaries share D65).
    pub fn conversion(from: Primaries, to: Primaries) -> [[f32; 3]; 3] {
        if from == to {
            return IDENTITY;
        }
        let m = mul(&inverse(&to.to_xyz()), &from.to_xyz());
        m.map(|row| row.map(|v| v as f32))
    }

    /// How wide the gamut is, to compare the named ones (sRGB 0, Display
    /// P3 1, BT.2020 2); custom primaries count as BT.2020.
    pub fn rank(&self) -> u8 {
        match self {
            Primaries::Srgb => 0,
            Primaries::DisplayP3 => 1,
            Primaries::Bt2020 | Primaries::Custom { .. } => 2,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Primaries::Srgb => "sRGB",
            Primaries::DisplayP3 => "Display P3",
            Primaries::Bt2020 => "BT.2020",
            Primaries::Custom { .. } => "custom primaries",
        }
    }
}

/// The transfer the shader applies before writing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transfer {
    /// The sRGB piecewise curve.
    SrgbPiecewise,
    /// A pure 2.2 power.
    Gamma22,
    /// Linear, extended range (scRGB): negatives carry the gamut, values
    /// above 1.0 the headroom.
    ExtLinear,
    /// The sRGB curve continued past 1.0 and mirrored below 0.0 (the
    /// extended sRGB colour spaces).
    ExtSrgb,
    /// SMPTE ST 2084 (PQ): absolute luminance, 1.0 the signal of 10 000
    /// nits.
    Pq,
    /// BT.2100 hybrid log-gamma, with SDR white at 75 % of the signal
    /// (BT.2408).
    Hlg,
}

/// PQ constants (SMPTE ST 2084).
const PQ_M1: f32 = 2610.0 / 16384.0;
const PQ_M2: f32 = 2523.0 / 4096.0 * 128.0;
const PQ_C1: f32 = 3424.0 / 4096.0;
const PQ_C2: f32 = 2413.0 / 4096.0 * 32.0;
const PQ_C3: f32 = 2392.0 / 4096.0 * 32.0;

/// HLG constants (BT.2100).
const HLG_A: f32 = 0.178_832_77;
const HLG_B: f32 = 1.0 - 4.0 * HLG_A;
const HLG_C: f32 = 0.559_910_7;

/// The HLG scene light SDR white maps to: the signal 0.75 (BT.2408).
pub const HLG_WHITE: f32 = 0.264_962_56;

/// The luminance of SDR white BT.2408 recommends for HDR (nits).
pub const HDR_REFERENCE_WHITE: f32 = 203.0;

fn srgb_encode(v: f32) -> f32 {
    if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

fn srgb_decode(v: f32) -> f32 {
    if v <= 0.040_45 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// The PQ signal of an absolute luminance in nits.
pub fn pq_encode(nits: f32) -> f32 {
    let y = (nits / 10_000.0).clamp(0.0, 1.0).powf(PQ_M1);
    ((PQ_C1 + PQ_C2 * y) / (1.0 + PQ_C3 * y)).powf(PQ_M2)
}

/// The luminance in nits of a PQ signal.
pub fn pq_decode(signal: f32) -> f32 {
    let e = signal.clamp(0.0, 1.0).powf(1.0 / PQ_M2);
    let y = ((e - PQ_C1).max(0.0) / (PQ_C2 - PQ_C3 * e)).powf(1.0 / PQ_M1);
    y * 10_000.0
}

/// The HLG signal of normalised scene light (0..1, the BT.2100 OETF).
pub fn hlg_oetf(e: f32) -> f32 {
    let e = e.clamp(0.0, 1.0);
    if e <= 1.0 / 12.0 {
        (3.0 * e).sqrt()
    } else {
        HLG_A * (12.0 * e - HLG_B).ln() + HLG_C
    }
}

/// The normalised scene light of an HLG signal (the inverse OETF).
pub fn hlg_inverse_oetf(signal: f32) -> f32 {
    let s = signal.clamp(0.0, 1.0);
    if s <= 0.5 {
        s * s / 3.0
    } else {
        (((s - HLG_C) / HLG_A).exp() + HLG_B) / 12.0
    }
}

impl Transfer {
    /// Encodes a linear value (1.0 is SDR white), SDR white at
    /// [`HDR_REFERENCE_WHITE`] nits for PQ.
    pub fn encode(self, v: f32) -> f32 {
        self.encode_at(v, HDR_REFERENCE_WHITE)
    }

    /// Encodes a linear value, SDR white at `white_nits` (used by PQ
    /// only, which is absolute).
    pub fn encode_at(self, v: f32, white_nits: f32) -> f32 {
        match self {
            Transfer::ExtLinear => v,
            Transfer::Gamma22 => v.max(0.0).powf(1.0 / 2.2),
            Transfer::SrgbPiecewise => srgb_encode(v.max(0.0)),
            Transfer::ExtSrgb => v.signum() * srgb_encode(v.abs()),
            Transfer::Pq => pq_encode(v.max(0.0) * white_nits),
            Transfer::Hlg => hlg_oetf(v.max(0.0) * HLG_WHITE),
        }
    }

    /// Decodes a signal back to linear (1.0 is SDR white), SDR white at
    /// [`HDR_REFERENCE_WHITE`] nits for PQ.
    pub fn decode(self, v: f32) -> f32 {
        self.decode_at(v, HDR_REFERENCE_WHITE)
    }

    /// Decodes a signal back to linear, SDR white at `white_nits`.
    pub fn decode_at(self, v: f32, white_nits: f32) -> f32 {
        match self {
            Transfer::ExtLinear => v,
            Transfer::Gamma22 => v.max(0.0).powf(2.2),
            Transfer::SrgbPiecewise => srgb_decode(v.max(0.0)),
            Transfer::ExtSrgb => v.signum() * srgb_decode(v.abs()),
            Transfer::Pq => pq_decode(v) / white_nits,
            Transfer::Hlg => hlg_inverse_oetf(v) / HLG_WHITE,
        }
    }

    /// Whether values above SDR white are carried.
    pub fn is_hdr(self) -> bool {
        matches!(
            self,
            Transfer::ExtLinear | Transfer::ExtSrgb | Transfer::Pq | Transfer::Hlg
        )
    }

    /// Whether negatives are carried (any gamut through any primaries).
    pub fn is_extended(self) -> bool {
        matches!(self, Transfer::ExtLinear | Transfer::ExtSrgb)
    }

    /// The highest headroom the transfer carries (relative to SDR white).
    pub fn max_headroom(self, white_nits: f32) -> f32 {
        match self {
            Transfer::SrgbPiecewise | Transfer::Gamma22 => 1.0,
            Transfer::ExtLinear | Transfer::ExtSrgb => f32::MAX,
            Transfer::Pq => 10_000.0 / white_nits.max(1.0),
            Transfer::Hlg => 1.0 / HLG_WHITE,
        }
    }

    /// The code the shader uses.
    pub fn code(self) -> u32 {
        match self {
            Transfer::SrgbPiecewise => 0,
            Transfer::Gamma22 => 1,
            Transfer::ExtLinear => 2,
            Transfer::Pq => 3,
            Transfer::Hlg => 4,
            Transfer::ExtSrgb => 5,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Transfer::SrgbPiecewise => "sRGB",
            Transfer::Gamma22 => "gamma 2.2",
            Transfer::ExtLinear => "extended linear",
            Transfer::ExtSrgb => "extended sRGB",
            Transfer::Pq => "PQ",
            Transfer::Hlg => "HLG",
        }
    }
}

/// The surface's pixel format, as configured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceFormat {
    /// 8 bits, the hardware encodes sRGB on write.
    Rgba8UnormSrgb,
    /// 8 bits, written as given (the shader encodes).
    Rgba8Unorm,
    /// 10 bits per channel.
    Rgb10a2Unorm,
    /// Half floats (scRGB, extended sRGB or HDR).
    Rgba16Float,
    /// Anything else.
    Other,
}

impl SurfaceFormat {
    /// Whether the format holds values outside 0..1.
    pub fn is_float(self) -> bool {
        self == SurfaceFormat::Rgba16Float
    }

    /// Bits per colour channel (0 when unknown).
    pub fn bits(self) -> u8 {
        match self {
            SurfaceFormat::Rgba8UnormSrgb | SurfaceFormat::Rgba8Unorm => 8,
            SurfaceFormat::Rgb10a2Unorm => 10,
            SurfaceFormat::Rgba16Float => 16,
            SurfaceFormat::Other => 0,
        }
    }
}

/// The colour space a surface is configured with: how the compositor
/// reads what is written (the colour spaces wgpu configures).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ColorSpace {
    Srgb,
    /// scRGB: linear, sRGB primaries, extended range.
    ExtendedSrgbLinear,
    /// sRGB-encoded, extended range.
    ExtendedSrgb,
    DisplayP3,
    /// Display P3, sRGB-encoded, extended range.
    ExtendedDisplayP3,
    /// HDR10: BT.2020 primaries, PQ.
    Bt2100Pq,
    /// BT.2020 primaries, HLG.
    Bt2100Hlg,
}

impl ColorSpace {
    pub const ALL: [ColorSpace; 7] = [
        ColorSpace::Srgb,
        ColorSpace::ExtendedSrgbLinear,
        ColorSpace::ExtendedSrgb,
        ColorSpace::DisplayP3,
        ColorSpace::ExtendedDisplayP3,
        ColorSpace::Bt2100Pq,
        ColorSpace::Bt2100Hlg,
    ];

    /// Its primaries and transfer.
    pub fn encoding(self) -> (Primaries, Transfer) {
        match self {
            ColorSpace::Srgb => (Primaries::Srgb, Transfer::SrgbPiecewise),
            ColorSpace::ExtendedSrgbLinear => (Primaries::Srgb, Transfer::ExtLinear),
            ColorSpace::ExtendedSrgb => (Primaries::Srgb, Transfer::ExtSrgb),
            ColorSpace::DisplayP3 => (Primaries::DisplayP3, Transfer::SrgbPiecewise),
            ColorSpace::ExtendedDisplayP3 => (Primaries::DisplayP3, Transfer::ExtSrgb),
            ColorSpace::Bt2100Pq => (Primaries::Bt2020, Transfer::Pq),
            ColorSpace::Bt2100Hlg => (Primaries::Bt2020, Transfer::Hlg),
        }
    }

    /// Whether egui's renderer draws the interface faithfully on it:
    /// egui writes sRGB-encoded values to every surface that is not an
    /// `*Srgb` format, which only the sRGB and extended sRGB colour
    /// spaces read as meant.
    pub fn egui_draws_faithfully(self) -> bool {
        matches!(self, ColorSpace::Srgb | ColorSpace::ExtendedSrgb)
    }

    /// The formats that carry it: half floats for the extended ones, at
    /// least 10 bits for PQ and HLG.
    pub fn needs(self) -> &'static [SurfaceFormat] {
        match self {
            ColorSpace::Srgb | ColorSpace::DisplayP3 => &[
                SurfaceFormat::Rgba8Unorm,
                SurfaceFormat::Rgba8UnormSrgb,
                SurfaceFormat::Rgb10a2Unorm,
                SurfaceFormat::Rgba16Float,
            ],
            ColorSpace::ExtendedSrgbLinear
            | ColorSpace::ExtendedSrgb
            | ColorSpace::ExtendedDisplayP3 => &[SurfaceFormat::Rgba16Float],
            ColorSpace::Bt2100Pq | ColorSpace::Bt2100Hlg => {
                &[SurfaceFormat::Rgb10a2Unorm, SurfaceFormat::Rgba16Float]
            }
        }
    }
}

/// Where the target's description comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetSource {
    /// Nothing declared: sRGB assumed.
    Assumed,
    /// Declared by the driver's window system integration (the surface
    /// configured through wgpu).
    Wsi,
    /// Declared by the application itself: through a compositor
    /// protocol (Wayland's `wp_color_management_v1`, any primaries and
    /// transfer the protocol names, no surface colour space needed), or
    /// on a surface it configured itself.
    Tagged,
    /// Read from the operating system.
    Os,
}

/// What a window's surface expects.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisplayTarget {
    /// Primaries and white of the encoding the surface declares (not the
    /// monitor's).
    pub primaries: Primaries,
    pub transfer: Transfer,
    /// Nits the encoding's reference white maps to.
    pub sdr_white_nits: f32,
    /// Highest value the target may carry, relative to SDR white (1.0
    /// for SDR).
    pub headroom: f32,
    pub surface_format: SurfaceFormat,
    pub source: TargetSource,
}

impl Default for DisplayTarget {
    fn default() -> Self {
        DisplayTarget::srgb()
    }
}

/// Why a target is not drawn as wanted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fallback {
    /// Nobody configured the surface for it (its source is `Assumed`).
    NotConfigured,
    /// No surface colour space has these primaries and this transfer
    /// (nor the compositor protocol, for a tagged target).
    NoColorSpace,
    /// The format cannot carry the transfer (8 bits for PQ, unsigned
    /// values for an extended range).
    Format(SurfaceFormat),
    /// Headroom above SDR white on an SDR transfer.
    HeadroomOnSdr,
    /// The surface carries less than wanted (its encoding, and what is
    /// missing).
    Surface { surface: String, missing: String },
    /// As `Surface`, and the image could not get a surface of its own
    /// (why: no colour management on the compositor, an X11 window...).
    Unavailable {
        surface: String,
        missing: String,
        why: String,
    },
    /// The CPU path draws 8-bit sRGB textures.
    CpuPath,
    /// The application's hook chose so.
    Hook,
}

impl std::fmt::Display for Fallback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Fallback::NotConfigured => {
                f.write_str("the window's surface is not configured for it (nothing declared it)")
            }
            Fallback::NoColorSpace => {
                f.write_str("no surface colour space or compositor encoding has these primaries and this transfer")
            }
            Fallback::Format(format) => write!(f, "a {format:?} surface cannot carry it"),
            Fallback::HeadroomOnSdr => {
                f.write_str("headroom above SDR white needs an HDR transfer")
            }
            Fallback::Surface { surface, missing } => {
                write!(f, "the surface is {surface}: {missing} cannot be shown")
            }
            Fallback::Unavailable {
                surface,
                missing,
                why,
            } => write!(
                f,
                "the surface is {surface}: {missing} cannot be shown ({why})"
            ),
            Fallback::CpuPath => f.write_str(
                "the CPU path draws 8-bit sRGB textures (the GPU path draws wide gamut and HDR)",
            ),
            Fallback::Hook => f.write_str("the application's hook chose this target"),
        }
    }
}

impl DisplayTarget {
    /// sRGB, SDR, 8 bits: what every surface can show.
    pub fn srgb() -> DisplayTarget {
        DisplayTarget {
            primaries: Primaries::Srgb,
            transfer: Transfer::SrgbPiecewise,
            sdr_white_nits: 80.0,
            headroom: 1.0,
            surface_format: SurfaceFormat::Rgba8UnormSrgb,
            source: TargetSource::Assumed,
        }
    }

    /// Display P3, SDR (what an application wanting the P3 gamut asks
    /// for).
    pub fn display_p3() -> DisplayTarget {
        DisplayTarget {
            primaries: Primaries::DisplayP3,
            surface_format: SurfaceFormat::Rgba8Unorm,
            ..DisplayTarget::srgb()
        }
    }

    /// HDR10: BT.2020 primaries, PQ, SDR white at 203 nits, `headroom`
    /// times brighter at most.
    pub fn hdr_pq(headroom: f32) -> DisplayTarget {
        DisplayTarget {
            primaries: Primaries::Bt2020,
            transfer: Transfer::Pq,
            sdr_white_nits: HDR_REFERENCE_WHITE,
            headroom,
            surface_format: SurfaceFormat::Rgb10a2Unorm,
            source: TargetSource::Assumed,
        }
    }

    /// BT.2100 HLG, SDR white at 75 % of the signal.
    pub fn hdr_hlg() -> DisplayTarget {
        DisplayTarget {
            transfer: Transfer::Hlg,
            headroom: 1.0 / HLG_WHITE,
            ..DisplayTarget::hdr_pq(1.0)
        }
    }

    /// The encoding of a surface colour space on a format, as declared
    /// by `source`; `headroom` is kept within what the transfer carries.
    pub fn for_color_space(
        color_space: ColorSpace,
        format: SurfaceFormat,
        headroom: f32,
        source: TargetSource,
    ) -> DisplayTarget {
        let (primaries, transfer) = color_space.encoding();
        let sdr_white_nits = match transfer {
            Transfer::Pq | Transfer::Hlg => HDR_REFERENCE_WHITE,
            _ => 80.0,
        };
        let headroom = if transfer.is_hdr() {
            headroom.clamp(1.0, transfer.max_headroom(sdr_white_nits))
        } else {
            1.0
        };
        DisplayTarget {
            primaries,
            transfer,
            sdr_white_nits,
            headroom,
            surface_format: format,
            source,
        }
    }

    /// Who declared it.
    pub fn with_source(mut self, source: TargetSource) -> DisplayTarget {
        self.source = source;
        self
    }

    /// The surface colour space this encoding is, if any (a 2.2 power
    /// counts as sRGB).
    pub fn color_space(&self) -> Option<ColorSpace> {
        ColorSpace::ALL.into_iter().find(|c| {
            let (p, t) = c.encoding();
            p == self.primaries
                && (t == self.transfer
                    || (t == Transfer::SrgbPiecewise && self.transfer == Transfer::Gamma22))
        })
    }

    /// Why the viewport cannot draw this encoding faithfully, if it
    /// cannot: it must be a colour space a surface can be configured
    /// with, on a format that carries it, declared by whoever configured
    /// the surface (an `Assumed` target is sRGB). A `Tagged` target needs
    /// no colour space: the compositor protocol names its primaries and
    /// transfer ([`DisplayTarget::untaggable`]).
    pub fn unsupported(&self) -> Option<Fallback> {
        let srgb = self.primaries == Primaries::Srgb
            && matches!(self.transfer, Transfer::SrgbPiecewise | Transfer::Gamma22);
        if self.headroom > 1.0 && !self.transfer.is_hdr() {
            return Some(Fallback::HeadroomOnSdr);
        }
        if srgb {
            return None;
        }
        if self.source == TargetSource::Tagged {
            let tag = self.untaggable()?;
            // A surface the application configured in a colour space.
            return match self.color_space() {
                Some(space) if space.needs().contains(&self.surface_format) => None,
                _ => Some(tag),
            };
        }
        let Some(space) = self.color_space() else {
            return Some(Fallback::NoColorSpace);
        };
        if !space.needs().contains(&self.surface_format) {
            return Some(Fallback::Format(self.surface_format));
        }
        if self.source == TargetSource::Assumed {
            return Some(Fallback::NotConfigured);
        }
        None
    }

    /// Why a surface tagged through the compositor protocol cannot carry
    /// this encoding, if it cannot: the protocol names the sRGB, Display
    /// P3 and BT.2020 primaries (not custom ones) and every transfer but
    /// extended sRGB; the format must carry the transfer (half floats for
    /// extended linear, at least 10 bits for PQ and HLG).
    pub fn untaggable(&self) -> Option<Fallback> {
        if matches!(self.primaries, Primaries::Custom { .. }) || self.transfer == Transfer::ExtSrgb
        {
            return Some(Fallback::NoColorSpace);
        }
        let carries = match self.transfer {
            Transfer::ExtLinear | Transfer::ExtSrgb => self.surface_format.is_float(),
            Transfer::Pq | Transfer::Hlg => self.surface_format.bits() >= 10,
            Transfer::SrgbPiecewise | Transfer::Gamma22 => {
                self.surface_format != SurfaceFormat::Other
            }
        };
        (!carries).then_some(Fallback::Format(self.surface_format))
    }

    /// Whether the viewport draws this target faithfully. The stage
    /// encodes every transfer and primaries (CPU and GPU), so this says
    /// whether the encoding is one a surface can be configured with, on
    /// a format that carries it, declared by whoever configured it
    /// ([`DisplayTarget::unsupported`] says why not). Others are drawn
    /// as sRGB ([`DisplayTarget::effective`]).
    pub fn is_supported(&self) -> bool {
        self.unsupported().is_none()
    }

    /// The target actually drawn: this one when supported, else sRGB.
    pub fn effective(&self) -> DisplayTarget {
        if self.is_supported() {
            *self
        } else {
            DisplayTarget {
                surface_format: self.surface_format,
                source: self.source,
                ..DisplayTarget::srgb()
            }
        }
    }

    /// The bounds after the matrix: `[0, 1]` for SDR, `[0, headroom]`
    /// for PQ and HLG, `[-headroom, headroom]` for the extended ones
    /// (negatives carry colours outside the primaries).
    pub fn range(&self) -> [f32; 2] {
        let h = self.headroom.max(1.0);
        match self.transfer {
            Transfer::SrgbPiecewise | Transfer::Gamma22 => [0.0, 1.0],
            Transfer::Pq | Transfer::Hlg => [0.0, h],
            Transfer::ExtLinear | Transfer::ExtSrgb => [-h, h],
        }
    }

    /// A short description ("Display P3 primaries, sRGB transfer, SDR").
    pub fn describe(&self) -> String {
        let range = if self.transfer.is_hdr() {
            format!("headroom {:.1}", self.headroom)
        } else {
            "SDR".into()
        };
        format!(
            "{} primaries, {} transfer, {range}",
            self.primaries.name(),
            self.transfer.name()
        )
    }

    /// What this target carries less than `wanted` (a wider gamut, more
    /// headroom), if anything.
    pub fn missing(&self, wanted: &DisplayTarget) -> Option<String> {
        let gamut = if self.transfer.is_extended() {
            u8::MAX
        } else {
            self.primaries.rank()
        };
        let mut missing = Vec::new();
        if wanted.primaries.rank() > gamut {
            missing.push(format!("the {} gamut", wanted.primaries.name()));
        }
        let wanted_headroom = if wanted.transfer.is_hdr() {
            wanted.headroom
        } else {
            1.0
        };
        let headroom = if self.transfer.is_hdr() {
            self.headroom
        } else {
            1.0
        };
        if wanted_headroom > headroom + 1e-3 {
            missing.push(format!("headroom {wanted_headroom:.1} (HDR)"));
        }
        (!missing.is_empty()).then(|| missing.join(" and "))
    }
}

/// What a window draws for, and why when less than wanted.
#[derive(Clone, Debug, PartialEq)]
pub struct Resolution {
    /// What the application wants for the window (sRGB unless said).
    pub wanted: DisplayTarget,
    /// What the window's surface expects.
    pub surface: DisplayTarget,
    /// What is drawn: the stage encodes for it.
    pub drawn: DisplayTarget,
    /// Why `drawn` carries less than `wanted`, or why the surface's
    /// encoding is not drawn.
    pub fallback: Option<Fallback>,
}

impl Resolution {
    /// Whether the window shows what was wanted.
    pub fn faithful(&self) -> bool {
        self.fallback.is_none()
    }

    /// The reason, in words.
    pub fn reason(&self) -> Option<String> {
        self.fallback.as_ref().map(ToString::to_string)
    }
}

/// What the viewport draws for, given what is wanted, what the surface
/// expects and whether the GPU path runs: the surface's encoding when it
/// is supported (the stage converts the wanted content into it), sRGB
/// otherwise; the fallback says what is lost.
///
/// ```
/// use egui_pop_viewport::display::{self, ColorSpace, DisplayTarget, SurfaceFormat, TargetSource};
///
/// // eframe today: an 8-bit sRGB surface; P3 is drawn as sRGB, with the reason.
/// let r = display::resolve(&DisplayTarget::display_p3(), &DisplayTarget::srgb(), true);
/// assert_eq!(r.drawn.color_space(), Some(ColorSpace::Srgb));
/// assert!(r.reason().unwrap().contains("Display P3"));
///
/// // A surface configured as extended sRGB on half floats carries P3.
/// let ext = DisplayTarget::for_color_space(
///     ColorSpace::ExtendedSrgb, SurfaceFormat::Rgba16Float, 4.0, TargetSource::Wsi);
/// assert!(display::resolve(&DisplayTarget::display_p3(), &ext, true).faithful());
/// ```
pub fn resolve(wanted: &DisplayTarget, surface: &DisplayTarget, gpu: bool) -> Resolution {
    let srgb = DisplayTarget {
        surface_format: surface.surface_format,
        source: surface.source,
        ..DisplayTarget::srgb()
    };
    let (drawn, fallback) = match surface.unsupported() {
        Some(f) => (srgb, Some(f)),
        None if !gpu && surface.color_space() != Some(ColorSpace::Srgb) => {
            // sRGB textures on any surface egui draws faithfully; what
            // they lose is said only when it was wanted.
            let lost = srgb.missing(wanted).map(|_| Fallback::CpuPath);
            (srgb, lost)
        }
        None => {
            let lost = surface.missing(wanted).map(|missing| Fallback::Surface {
                surface: surface.describe(),
                missing,
            });
            (*surface, lost)
        }
    };
    Resolution {
        wanted: *wanted,
        surface: *surface,
        drawn,
        fallback,
    }
}

/// What a window's surface expects, per window.
const TARGET: Slot<DisplayTarget> = Slot::new("viewport.display.target");
/// What the application wants for a window, per window.
const WANTED: Slot<DisplayTarget> = Slot::new("viewport.display.wanted");
/// What every window's surface expects unless set per window.
const DEFAULT_TARGET: Slot<DisplayTarget> = Slot::new("viewport.display.default_target");
/// What a window's image is drawn into, per window.
const IMAGE: Slot<ImageSurface> = Slot::new("viewport.display.image");

/// What the surface of the window being drawn expects: what was set for
/// it, else what the GPU path found when installed, else sRGB.
pub fn target(ctx: &Context) -> DisplayTarget {
    target_of(ctx, ctx.viewport_id())
}

/// What a window's image is drawn into: the image's own surface when it
/// has one ([`set_image_surface`]), else the window's surface.
pub fn target_of(ctx: &Context, viewport: ViewportId) -> DisplayTarget {
    IMAGE
        .keyed(viewport)
        .get(ctx)
        .and_then(|i| i.target)
        .or_else(|| TARGET.keyed(viewport).get(ctx))
        .or_else(|| DEFAULT_TARGET.get(ctx))
        .unwrap_or_default()
}

/// The image's own surface in a window, as the image surface layer
/// decided it (`image_surface`, Wayland).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImageSurface {
    /// What the image's surface expects, when the image has one (it then
    /// overrides the window's surface for the image).
    pub target: Option<DisplayTarget>,
    /// Why the image has no surface of its own, or one carrying less than
    /// wanted.
    pub why: Option<String>,
}

/// Sets what a window's image is drawn into when it has a surface of
/// its own, and why not (or why less than wanted); viewports in that
/// window follow on the next frame.
pub fn set_image_surface(ctx: &Context, viewport: ViewportId, image: ImageSurface) {
    let slot = IMAGE.keyed(viewport);
    let changed = slot.get(ctx).as_ref() != Some(&image);
    slot.set(ctx, image);
    if changed {
        ctx.request_repaint_of(viewport);
    }
}

/// What was set with [`set_image_surface`] for a window.
pub fn image_surface_of(ctx: &Context, viewport: ViewportId) -> Option<ImageSurface> {
    IMAGE.keyed(viewport).get(ctx)
}

/// Sets what a window's surface expects (for example when its surface
/// or monitor changes, or when the application configured the surface
/// itself). Viewports in that window follow on the next frame and signal
/// it.
pub fn set_target(ctx: &Context, viewport: ViewportId, target: DisplayTarget) {
    TARGET.keyed(viewport).set(ctx, target);
    ctx.request_repaint_of(viewport);
}

/// Sets what every window's surface expects unless set per window (the
/// GPU path calls it with what eframe configured).
pub fn set_default_target(ctx: &Context, target: DisplayTarget) {
    DEFAULT_TARGET.set(ctx, target);
    ctx.request_repaint();
}

/// What the application wants for the window being drawn (sRGB until
/// said).
pub fn wanted(ctx: &Context) -> DisplayTarget {
    let viewport = ctx.viewport_id();
    WANTED.keyed(viewport).get_or_default(ctx)
}

/// Declares what the application wants for a window: Display P3, HDR.
/// The viewport draws it when the surface carries it, and says why not
/// otherwise ([`Resolution::fallback`]).
pub fn want(ctx: &Context, viewport: ViewportId, wanted: DisplayTarget) {
    WANTED.keyed(viewport).set(ctx, wanted);
    ctx.request_repaint_of(viewport);
}

/// What the window being drawn draws for: what is wanted against what
/// its surface expects, on the GPU path when it is installed (as a
/// viewport resolves it, before its `before_display` hook).
pub fn resolution(ctx: &Context) -> Resolution {
    let mut r = resolve(&wanted(ctx), &target(ctx), crate::tiles::device(ctx).gpu);
    let why = image_surface_of(ctx, ctx.viewport_id()).and_then(|i| i.why);
    if let (Some(why), Some(Fallback::Surface { surface, missing })) = (why, r.fallback.clone()) {
        r.fallback = Some(Fallback::Unavailable {
            surface,
            missing,
            why,
        });
    }
    r
}

/// The arithmetic of a display transform for one target, as the shader
/// runs it: `encode(clamp(matrix × rgb))`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stage {
    /// Working space to the target's primaries, by rows.
    pub matrix: [[f32; 3]; 3],
    /// Lower and upper bound after the matrix (`[0, 1]` for SDR).
    pub clamp: [f32; 2],
    pub transfer: Transfer,
    /// Nits SDR white maps to (PQ, which is absolute, uses it).
    pub white_nits: f32,
}

impl Stage {
    /// Linear pixels in `working` primaries to `target`: the matrix
    /// between the primaries, the target's range and transfer.
    pub fn convert(working: Primaries, target: &DisplayTarget) -> Stage {
        Stage {
            matrix: Primaries::conversion(working, target.primaries),
            clamp: target.range(),
            transfer: target.transfer,
            white_nits: target.sdr_white_nits,
        }
    }

    /// This stage after `pre` (a matrix applied to the pixels first).
    pub fn after(mut self, pre: [[f32; 3]; 3]) -> Stage {
        let m = self.matrix;
        self.matrix =
            [0, 1, 2].map(|i| [0, 1, 2].map(|j| (0..3).map(|k| m[i][k] * pre[k][j]).sum()));
        self
    }

    /// One pixel, encoded (0..1 for SDR; the extended transfers go
    /// beyond).
    pub fn encode(&self, rgb: [f32; 3]) -> [f32; 3] {
        let m = &self.matrix;
        [0, 1, 2].map(|i| {
            let v = m[i][0] * rgb[0] + m[i][1] * rgb[1] + m[i][2] * rgb[2];
            self.transfer
                .encode_at(v.clamp(self.clamp[0], self.clamp[1]), self.white_nits)
        })
    }

    /// One pixel, encoded in 8 bits.
    pub fn encode_8bit(&self, rgb: [f32; 3]) -> [u8; 3] {
        self.encode(rgb)
            .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
    }
}

/// The identity matrix.
pub const IDENTITY: [[f32; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// The application's colour management at the end of the viewport.
pub trait DisplayTransform: Send + Sync + std::fmt::Debug {
    /// The stage for a target (the GPU path runs it in its shader).
    fn stage(&self, target: &DisplayTarget) -> Stage;

    /// Scene pixels (RGBA floats) to display colours, on the CPU. By
    /// default the stage's arithmetic; override for a LUT or anything
    /// the stage cannot express (then deliver display pixels from the
    /// source, as the GPU path only runs stages).
    fn apply(&self, target: &DisplayTarget, scene: &[f32], out: &mut [Color32]) {
        let stage = self.stage(target);
        for (px, o) in scene.chunks_exact(4).zip(out.iter_mut()) {
            let [r, g, b] = stage.encode_8bit([px[0], px[1], px[2]]);
            let a = (px[3].clamp(0.0, 1.0) * 255.0).round() as u8;
            *o = Color32::from_rgba_unmultiplied(r, g, b, a);
        }
    }

    /// Scene pixels (RGBA floats) to encoded floats for a target, on the
    /// CPU (an export, a surface of the application's own): the stage's
    /// arithmetic, alpha kept.
    fn apply_f32(&self, target: &DisplayTarget, scene: &[f32], out: &mut [f32]) {
        let stage = self.stage(target);
        for (px, o) in scene.chunks_exact(4).zip(out.chunks_exact_mut(4)) {
            let [r, g, b] = stage.encode([px[0], px[1], px[2]]);
            o.copy_from_slice(&[r, g, b, px[3]]);
        }
    }

    /// Bumped when the transform changes (pixels transformed on the CPU
    /// are then redone).
    fn revision(&self) -> u64 {
        0
    }
}

/// Scene pixels already in linear sRGB primaries: converted to the
/// target's primaries, clamped and encoded.
#[derive(Clone, Copy, Debug, Default)]
pub struct LinearSrgb;

impl DisplayTransform for LinearSrgb {
    fn stage(&self, target: &DisplayTarget) -> Stage {
        Stage::convert(Primaries::Srgb, &target.effective())
    }
}

/// Scene pixels in linear RGB of other primaries (a wide working space:
/// linear BT.2020, custom primaries): converted to the target's
/// primaries, clamped and encoded.
#[derive(Clone, Copy, Debug)]
pub struct Linear(pub Primaries);

impl DisplayTransform for Linear {
    fn stage(&self, target: &DisplayTarget) -> Stage {
        Stage::convert(self.0, &target.effective())
    }
}

/// Scene pixels through a matrix of the application's to linear sRGB,
/// then to the target's primaries, clamped and encoded.
#[derive(Clone, Copy, Debug)]
pub struct Matrix(pub [[f32; 3]; 3]);

impl DisplayTransform for Matrix {
    fn stage(&self, target: &DisplayTarget) -> Stage {
        Stage::convert(Primaries::Srgb, &target.effective()).after(self.0)
    }
}

/// The CPU path: scene pixels through a transform for a target.
pub fn apply_cpu(
    transform: &dyn DisplayTransform,
    target: &DisplayTarget,
    scene: &[f32],
    out: &mut [Color32],
) {
    transform.apply(&target.effective(), scene, out);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    fn configured(space: ColorSpace, format: SurfaceFormat, headroom: f32) -> DisplayTarget {
        DisplayTarget::for_color_space(space, format, headroom, TargetSource::Wsi)
    }

    #[test]
    fn srgb_encoding_matches_the_standard_curve() {
        let s = LinearSrgb.stage(&DisplayTarget::srgb());
        assert_eq!(s.encode_8bit([0.0, 1.0, 0.002]), [0, 255, 7]);
        assert_eq!(s.encode_8bit([0.215_861, 0.5, 2.0]), [128, 188, 255]);
    }

    #[test]
    fn pq_matches_the_reference_values() {
        // SMPTE ST 2084 / BT.2100 reference points (nits to signal).
        for (nits, signal) in [
            (0.0, 0.000_000_7),
            (100.0, 0.508_078),
            (203.0, 0.580_690),
            (1000.0, 0.751_827),
            (4000.0, 0.902_572),
            (10_000.0, 1.0),
        ] {
            assert!(
                close(pq_encode(nits), signal, 2e-5),
                "{nits}: {}",
                pq_encode(nits)
            );
        }
        assert!(close(pq_decode(0.508_078), 100.0, 0.01));
        assert!(close(pq_decode(0.751_827), 1000.0, 0.1));
    }

    #[test]
    fn pq_round_trips() {
        for i in 0..=200 {
            let v = i as f32 / 20.0; // 0..10 times SDR white
            let e = Transfer::Pq.encode_at(v, 203.0);
            let back = Transfer::Pq.decode_at(e, 203.0);
            assert!(close(back, v, 1e-3 * v.max(1.0)), "{v} -> {e} -> {back}");
        }
    }

    #[test]
    fn hlg_matches_the_reference_values() {
        assert!(close(hlg_oetf(0.0), 0.0, 1e-7));
        assert!(close(hlg_oetf(1.0 / 12.0), 0.5, 1e-6));
        assert!(close(hlg_oetf(1.0), 1.0, 1e-6));
        // SDR white at 75 % of the signal (BT.2408).
        assert!(close(Transfer::Hlg.encode(1.0), 0.75, 1e-5));
        assert!(close(hlg_inverse_oetf(0.75), HLG_WHITE, 1e-6));
        // Both branches meet at 1/12.
        assert!(close(hlg_inverse_oetf(0.5), 1.0 / 12.0, 1e-6));
    }

    #[test]
    fn hlg_round_trips() {
        for i in 0..=100 {
            let e = i as f32 / 100.0;
            let back = hlg_oetf(hlg_inverse_oetf(e));
            assert!(close(back, e, 1e-5), "{e} -> {back}");
        }
        let max = Transfer::Hlg.max_headroom(203.0);
        for i in 0..=40 {
            let v = i as f32 / 40.0 * max;
            let back = Transfer::Hlg.decode(Transfer::Hlg.encode(v));
            assert!(close(back, v, 1e-4 * v.max(1.0)), "{v} -> {back}");
        }
    }

    #[test]
    fn extended_srgb_mirrors_negatives_and_continues_past_white() {
        let t = Transfer::ExtSrgb;
        assert!(close(t.encode(-0.5), -t.encode(0.5), 1e-7));
        assert!(t.encode(4.0) > 1.0);
        for v in [-2.0, -0.2, 0.001, 0.5, 1.0, 3.0] {
            assert!(close(t.decode(t.encode(v)), v, 1e-5), "{v}");
        }
    }

    #[test]
    #[allow(clippy::needless_range_loop)]
    fn primaries_convert_as_published() {
        // Linear sRGB to Display P3 and to BT.2020 (BT.2087), by rows.
        let p3 = Primaries::conversion(Primaries::Srgb, Primaries::DisplayP3);
        let want_p3 = [
            [0.822_462, 0.177_538, 0.0],
            [0.033_194, 0.966_806, 0.0],
            [0.017_083, 0.072_397, 0.910_520],
        ];
        let bt2020 = Primaries::conversion(Primaries::Srgb, Primaries::Bt2020);
        let want_2020 = [
            [0.627_404, 0.329_283, 0.043_313],
            [0.069_097, 0.919_540, 0.011_362],
            [0.016_391, 0.088_013, 0.895_595],
        ];
        for (m, w) in [(p3, want_p3), (bt2020, want_2020)] {
            for i in 0..3 {
                for j in 0..3 {
                    assert!(close(m[i][j], w[i][j], 2e-4), "{m:?}");
                }
            }
        }
        // White stays white; there and back is the identity.
        let back = Primaries::conversion(Primaries::Bt2020, Primaries::Srgb);
        for i in 0..3 {
            let row: f32 = bt2020[i].iter().sum();
            assert!(close(row, 1.0, 1e-5));
            for j in 0..3 {
                let v: f32 = (0..3).map(|k| back[i][k] * bt2020[k][j]).sum();
                assert!(close(v, if i == j { 1.0 } else { 0.0 }, 1e-5));
            }
        }
    }

    #[test]
    fn unsupported_targets_draw_as_srgb() {
        let hdr = DisplayTarget {
            primaries: Primaries::Bt2020,
            transfer: Transfer::Pq,
            sdr_white_nits: 203.0,
            headroom: 4.0,
            surface_format: SurfaceFormat::Rgba16Float,
            source: TargetSource::Assumed,
        };
        assert_eq!(hdr.unsupported(), Some(Fallback::NotConfigured));
        assert_eq!(hdr.effective().primaries, Primaries::Srgb);
        assert_eq!(LinearSrgb.stage(&hdr).clamp, [0.0, 1.0]);
        // PQ on 8 bits, P3 with a PQ curve: no surface carries them.
        let tagged = hdr.with_source(TargetSource::Tagged);
        assert!(tagged.is_supported());
        let eight = DisplayTarget {
            surface_format: SurfaceFormat::Rgba8Unorm,
            ..tagged
        };
        assert_eq!(
            eight.unsupported(),
            Some(Fallback::Format(SurfaceFormat::Rgba8Unorm))
        );
        // No wgpu colour space is Display P3 with PQ.
        let odd = DisplayTarget {
            primaries: Primaries::DisplayP3,
            ..tagged.with_source(TargetSource::Wsi)
        };
        assert_eq!(odd.unsupported(), Some(Fallback::NoColorSpace));
    }

    #[test]
    fn tagged_targets_need_no_surface_colour_space() {
        // BT.2020 with a 2.2 power: no wgpu colour space, a protocol
        // encoding (bt2020 + gamma22).
        let rec2020 = DisplayTarget {
            primaries: Primaries::Bt2020,
            transfer: Transfer::Gamma22,
            surface_format: SurfaceFormat::Rgb10a2Unorm,
            ..DisplayTarget::srgb()
        };
        assert_eq!(rec2020.color_space(), None);
        assert_eq!(rec2020.unsupported(), Some(Fallback::NoColorSpace));
        assert_eq!(rec2020.effective().primaries, Primaries::Srgb);
        let tagged = rec2020.with_source(TargetSource::Tagged);
        assert!(tagged.is_supported());
        assert_eq!(tagged.effective(), tagged);
        // The stage encodes a 2.2 power into BT.2020: sRGB white stays
        // white, pure sRGB red is inside BT.2020.
        let s = LinearSrgb.stage(&tagged);
        assert_eq!(s.transfer, Transfer::Gamma22);
        for v in s.encode([1.0, 1.0, 1.0]) {
            assert!(close(v, 1.0, 1e-5), "{v}");
        }
        let red = s.encode([1.0, 0.0, 0.0]);
        assert!(red[0] < 1.0 && red[1] > 0.0 && red[2] > 0.0, "{red:?}");
        // Eight bits carry an SDR transfer.
        let eight = DisplayTarget {
            surface_format: SurfaceFormat::Rgba8Unorm,
            ..tagged
        };
        assert!(eight.is_supported());
        // Display P3 with PQ is a protocol encoding, on 10 bits.
        let p3_pq = DisplayTarget {
            primaries: Primaries::DisplayP3,
            transfer: Transfer::Pq,
            sdr_white_nits: HDR_REFERENCE_WHITE,
            headroom: 4.0,
            ..tagged
        };
        assert!(p3_pq.is_supported());
        assert_eq!(
            DisplayTarget {
                surface_format: SurfaceFormat::Rgba8Unorm,
                ..p3_pq
            }
            .unsupported(),
            Some(Fallback::Format(SurfaceFormat::Rgba8Unorm))
        );
        // Not tagged: custom primaries, extended sRGB, extended linear
        // on 10 bits.
        let custom = DisplayTarget {
            primaries: Primaries::Custom {
                r: [0.7, 0.3],
                g: [0.2, 0.7],
                b: [0.15, 0.05],
                w: D65,
            },
            ..tagged
        };
        assert_eq!(custom.unsupported(), Some(Fallback::NoColorSpace));
        let ext = DisplayTarget {
            transfer: Transfer::ExtSrgb,
            ..tagged
        };
        assert_eq!(ext.unsupported(), Some(Fallback::NoColorSpace));
        let linear = DisplayTarget {
            transfer: Transfer::ExtLinear,
            ..tagged
        };
        assert_eq!(
            linear.unsupported(),
            Some(Fallback::Format(SurfaceFormat::Rgb10a2Unorm))
        );
        // Wanted BT.2020 on a tagged BT.2020 surface: faithful on the
        // GPU path.
        assert!(resolve(&rec2020, &tagged, true).faithful());
        assert_eq!(
            resolve(&rec2020, &tagged, false).fallback,
            Some(Fallback::CpuPath)
        );
    }

    #[test]
    fn configured_surfaces_are_drawn_faithfully() {
        let pq = configured(ColorSpace::Bt2100Pq, SurfaceFormat::Rgb10a2Unorm, 5.0);
        assert!(pq.is_supported());
        let s = LinearSrgb.stage(&pq);
        assert_eq!(s.transfer, Transfer::Pq);
        assert_eq!(s.clamp, [0.0, 5.0]);
        // sRGB white is BT.2020 white, at 203 nits.
        for v in s.encode([1.0, 1.0, 1.0]) {
            assert!(close(v, 0.580_69, 1e-4), "{v}");
        }
        // scRGB carries P3 red as a negative green and blue.
        let sc = configured(
            ColorSpace::ExtendedSrgbLinear,
            SurfaceFormat::Rgba16Float,
            4.0,
        );
        let red = Linear(Primaries::DisplayP3)
            .stage(&sc)
            .encode([1.0, 0.0, 0.0]);
        assert!(red[0] > 1.0 && red[1] < 0.0 && red[2] < 0.0, "{red:?}");
    }

    #[test]
    fn resolution_falls_back_with_a_reason() {
        let p3 = DisplayTarget::display_p3();
        let hdr = DisplayTarget::hdr_pq(4.0);
        // An 8-bit sRGB surface (eframe's): sRGB drawn, the reason given.
        let r = resolve(&p3, &DisplayTarget::srgb(), true);
        assert_eq!(r.drawn.color_space(), Some(ColorSpace::Srgb));
        assert!(matches!(r.fallback, Some(Fallback::Surface { .. })));
        let reason = resolve(&hdr, &DisplayTarget::srgb(), true)
            .reason()
            .unwrap();
        assert!(
            reason.contains("BT.2020") && reason.contains("headroom 4.0"),
            "{reason}"
        );
        // sRGB wanted on sRGB: faithful.
        assert!(resolve(&DisplayTarget::srgb(), &DisplayTarget::srgb(), false).faithful());
        // Extended sRGB carries P3 and HDR on the GPU path, not on the CPU.
        let ext = configured(ColorSpace::ExtendedSrgb, SurfaceFormat::Rgba16Float, 4.0);
        assert!(resolve(&p3, &ext, true).faithful());
        assert!(resolve(&hdr, &ext, true).faithful());
        let cpu = resolve(&p3, &ext, false);
        assert_eq!(cpu.fallback, Some(Fallback::CpuPath));
        assert_eq!(cpu.drawn.primaries, Primaries::Srgb);
        assert!(resolve(&DisplayTarget::srgb(), &ext, false).faithful());
        // HDR10 carries P3 and its headroom.
        let pq = configured(ColorSpace::Bt2100Pq, SurfaceFormat::Rgb10a2Unorm, 10.0);
        assert!(resolve(&hdr, &pq, true).faithful());
        // Less headroom than wanted is said.
        let dim = configured(ColorSpace::ExtendedSrgb, SurfaceFormat::Rgba16Float, 2.0);
        assert!(
            resolve(&hdr, &dim, true)
                .reason()
                .unwrap()
                .contains("headroom")
        );
    }

    #[test]
    fn the_cpu_path_runs_the_stage() {
        let m = Matrix([[0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]]);
        let mut out = [Color32::BLACK; 1];
        apply_cpu(&m, &DisplayTarget::srgb(), &[1.0, 0.0, 0.0, 1.0], &mut out);
        assert_eq!(out[0], Color32::from_rgb(0, 255, 0));
        let mut f = [0.0; 4];
        let pq = configured(ColorSpace::Bt2100Pq, SurfaceFormat::Rgba16Float, 4.0);
        LinearSrgb.apply_f32(&pq, &[1.0, 1.0, 1.0, 0.5], &mut f);
        assert!(close(f[0], 0.580_69, 1e-4) && f[3] == 0.5, "{f:?}");
    }
}
