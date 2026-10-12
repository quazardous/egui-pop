//! The display transform: the last step from the source's pixels to the
//! window, where the application plugs in its colour management.
//!
//! - A [`DisplayTarget`] says what the window's surface expects:
//!   primaries, transfer, the luminance of SDR white, the headroom above
//!   it, the surface format. It is per window ([`set_target`],
//!   [`target`]); only sRGB is supported for now, and the type is ready
//!   for wide gamut and HDR.
//! - A [`DisplayTransform`] (the application's, [`LinearSrgb`] by
//!   default) turns scene pixels into a [`Stage`] for a target: a 3×3
//!   matrix from the working space to the target's primaries, a clamp,
//!   and the target's transfer. The GPU path runs the stage in its
//!   shader; the CPU path ([`apply_cpu`]) runs the same arithmetic.
//! - A source that does its own transform on the CPU delivers display
//!   pixels (`Pixels::Display` of the tile source) and skips this.
//!
//! ```
//! use egui_pop_viewport::display::{DisplayTarget, DisplayTransform, LinearSrgb};
//!
//! let stage = LinearSrgb.stage(&DisplayTarget::srgb());
//! // Linear 0.5 is sRGB-encoded 188.
//! assert_eq!(stage.encode_8bit([0.5, 0.5, 0.5]), [188, 188, 188]);
//! ```

use egui::{Color32, Context, Id, ViewportId};

/// The primaries (and white) of an encoding.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Primaries {
    Srgb,
    DisplayP3,
    Bt2020,
    /// Chromaticities `[x, y]` of red, green, blue and white.
    Custom {
        r: [f64; 2],
        g: [f64; 2],
        b: [f64; 2],
        w: [f64; 2],
    },
}

/// The transfer the shader applies before writing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transfer {
    /// The sRGB piecewise curve.
    SrgbPiecewise,
    /// A pure 2.2 power.
    Gamma22,
    /// Linear, extended range (scRGB): negatives carry the gamut.
    ExtLinear,
    /// SMPTE ST 2084 (not implemented yet: drawn as sRGB).
    Pq,
    /// Hybrid log-gamma (not implemented yet: drawn as sRGB).
    Hlg,
}

impl Transfer {
    /// Encodes a linear value (1.0 is SDR white).
    pub fn encode(self, v: f32) -> f32 {
        match self {
            Transfer::ExtLinear => v,
            Transfer::Gamma22 => v.max(0.0).powf(1.0 / 2.2),
            Transfer::SrgbPiecewise | Transfer::Pq | Transfer::Hlg => {
                let v = v.max(0.0);
                if v <= 0.003_130_8 {
                    v * 12.92
                } else {
                    1.055 * v.powf(1.0 / 2.4) - 0.055
                }
            }
        }
    }

    /// The code the shader uses.
    pub fn code(self) -> u32 {
        match self {
            Transfer::SrgbPiecewise | Transfer::Pq | Transfer::Hlg => 0,
            Transfer::Gamma22 => 1,
            Transfer::ExtLinear => 2,
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
    /// Half floats (scRGB or HDR).
    Rgba16Float,
    /// Anything else.
    Other,
}

/// Where the target's description comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetSource {
    /// Nothing declared: sRGB assumed.
    Assumed,
    /// Declared by the driver's window system integration.
    Wsi,
    /// Declared by the application itself (a compositor protocol).
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

    /// Whether the viewport draws this target faithfully today (sRGB
    /// primaries, an SDR transfer). Others are drawn as sRGB.
    pub fn is_supported(&self) -> bool {
        self.primaries == Primaries::Srgb
            && matches!(self.transfer, Transfer::SrgbPiecewise | Transfer::Gamma22)
            && self.headroom <= 1.0
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
}

fn target_id(viewport: ViewportId) -> Id {
    Id::new(("egui_pop_viewport::display::target", viewport))
}

/// The display target of the window being drawn (sRGB until set).
pub fn target(ctx: &Context) -> DisplayTarget {
    let id = target_id(ctx.viewport_id());
    ctx.data(|d| d.get_temp(id)).unwrap_or_default()
}

/// Sets the display target of a window (for example when its surface or
/// monitor changes). Viewports in that window follow on the next frame
/// and signal it.
pub fn set_target(ctx: &Context, viewport: ViewportId, target: DisplayTarget) {
    ctx.data_mut(|d| d.insert_temp(target_id(viewport), target));
    ctx.request_repaint_of(viewport);
}

/// The arithmetic of a display transform for one target, as the shader
/// runs it: `encode(clamp(matrix × rgb))`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stage {
    /// Working space to the target's primaries, by rows.
    pub matrix: [[f32; 3]; 3],
    /// Lower and upper bound after the matrix (`[0, headroom]` for SDR).
    pub clamp: [f32; 2],
    pub transfer: Transfer,
}

impl Stage {
    /// One pixel, encoded (0..1 for SDR).
    pub fn encode(&self, rgb: [f32; 3]) -> [f32; 3] {
        let m = &self.matrix;
        [0, 1, 2].map(|i| {
            let v = m[i][0] * rgb[0] + m[i][1] * rgb[1] + m[i][2] * rgb[2];
            self.transfer.encode(v.clamp(self.clamp[0], self.clamp[1]))
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

    /// Bumped when the transform changes (pixels transformed on the CPU
    /// are then redone).
    fn revision(&self) -> u64 {
        0
    }
}

/// Scene pixels already in linear sRGB primaries: clamp and encode.
#[derive(Clone, Copy, Debug, Default)]
pub struct LinearSrgb;

impl DisplayTransform for LinearSrgb {
    fn stage(&self, target: &DisplayTarget) -> Stage {
        let t = target.effective();
        Stage {
            matrix: IDENTITY,
            clamp: [0.0, t.headroom.max(1.0)],
            transfer: t.transfer,
        }
    }
}

/// Scene pixels in another working space, through a matrix to the
/// target's primaries (sRGB for now), clamped and encoded: for example
/// linear Rec.2020 to sRGB.
#[derive(Clone, Copy, Debug)]
pub struct Matrix(pub [[f32; 3]; 3]);

impl DisplayTransform for Matrix {
    fn stage(&self, target: &DisplayTarget) -> Stage {
        let t = target.effective();
        Stage {
            matrix: self.0,
            clamp: [0.0, t.headroom.max(1.0)],
            transfer: t.transfer,
        }
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

    #[test]
    fn srgb_encoding_matches_the_standard_curve() {
        let s = LinearSrgb.stage(&DisplayTarget::srgb());
        assert_eq!(s.encode_8bit([0.0, 1.0, 0.002]), [0, 255, 7]);
        assert_eq!(s.encode_8bit([0.215_861, 0.5, 2.0]), [128, 188, 255]);
    }

    #[test]
    fn unsupported_targets_draw_as_srgb() {
        let hdr = DisplayTarget {
            primaries: Primaries::Bt2020,
            transfer: Transfer::Pq,
            sdr_white_nits: 203.0,
            headroom: 4.0,
            surface_format: SurfaceFormat::Rgba16Float,
            source: TargetSource::Tagged,
        };
        assert!(!hdr.is_supported());
        assert_eq!(hdr.effective().primaries, Primaries::Srgb);
        assert_eq!(LinearSrgb.stage(&hdr).clamp, [0.0, 1.0]);
    }

    #[test]
    fn the_cpu_path_runs_the_stage() {
        let m = Matrix([[0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]]);
        let mut out = [Color32::BLACK; 1];
        apply_cpu(&m, &DisplayTarget::srgb(), &[1.0, 0.0, 0.0, 1.0], &mut out);
        assert_eq!(out[0], Color32::from_rgb(0, 255, 0));
    }
}
