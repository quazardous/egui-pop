//! The Wayland layer of egui-pop's image viewport.
//!
//! winit binds no colour management and eframe configures every window
//! 8-bit sRGB (see the register of upstream workarounds,
//! `docs/upstream.md`), so an image cannot be shown in a wider gamut
//! through the window's own surface. This crate gives the image a
//! surface of its own instead, under the window, that the compositor
//! is told the encoding of (`docs/wayland-colour.md`):
//!
//! - [`Display::attach`] joins the connection winit opened, from the
//!   window's raw handles, on an event queue of its own (winit never sees
//!   its events), and reads what the compositor's `wp_color_manager_v1`
//!   announces ([`Capabilities`]) and its preferred image description for
//!   the window ([`Description`], kept current by [`Display::poll`]);
//! - [`Display::subsurface`] makes a [`Subsurface`]: a `wl_surface`
//!   placed below the window's, synchronised with it, taking no input,
//!   scaled through `wp_viewporter`; its [`SurfaceHandle`] is what wgpu
//!   creates a surface on;
//! - [`Subsurface::tag`] declares its encoding (named primaries and
//!   transfer function), once the description is ready.
//!
//! The crate knows nothing of egui or wgpu; the viewport maps its plain
//! types. On other systems, and for a window that is not a Wayland one,
//! [`Display::attach`] answers [`Unavailable`].

use std::fmt;

/// The raw window handles [`Display::attach`] takes.
pub use raw_window_handle;

#[cfg(target_os = "linux")]
mod client;
#[cfg(target_os = "linux")]
mod sys;

#[cfg(target_os = "linux")]
pub use client::{Display, Subsurface};
#[cfg(target_os = "linux")]
pub use sys::SurfaceHandle;

/// Named primaries of the protocol (`wp_color_manager_v1.primaries`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NamedPrimaries {
    /// BT.709 / sRGB.
    Srgb,
    DisplayP3,
    /// BT.2020 (and BT.2100).
    Bt2020,
    /// Any other, by its protocol code.
    Other(u32),
}

impl NamedPrimaries {
    pub fn from_code(code: u32) -> NamedPrimaries {
        match code {
            1 => NamedPrimaries::Srgb,
            9 => NamedPrimaries::DisplayP3,
            6 => NamedPrimaries::Bt2020,
            other => NamedPrimaries::Other(other),
        }
    }

    pub fn code(self) -> u32 {
        match self {
            NamedPrimaries::Srgb => 1,
            NamedPrimaries::DisplayP3 => 9,
            NamedPrimaries::Bt2020 => 6,
            NamedPrimaries::Other(code) => code,
        }
    }

    /// Its name in the protocol.
    pub fn name(self) -> &'static str {
        match self.code() {
            1 => "srgb",
            2 => "pal_m",
            3 => "pal",
            4 => "ntsc",
            5 => "generic_film",
            6 => "bt2020",
            7 => "cie1931_xyz",
            8 => "dci_p3",
            9 => "display_p3",
            10 => "adobe_rgb",
            _ => "unknown",
        }
    }
}

/// Named transfer functions of the protocol
/// (`wp_color_manager_v1.transfer_function`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NamedTransfer {
    /// The sRGB piecewise curve (deprecated in version 2 of the
    /// interface, announced to version 1 clients by some compositors).
    Srgb,
    /// A pure 2.2 power, the sRGB reference display's.
    Gamma22,
    /// Linear, extended range.
    ExtLinear,
    /// SMPTE ST 2084.
    St2084Pq,
    /// BT.2100 hybrid log-gamma.
    Hlg,
    /// Any other, by its protocol code.
    Other(u32),
}

impl NamedTransfer {
    pub fn from_code(code: u32) -> NamedTransfer {
        match code {
            9 => NamedTransfer::Srgb,
            2 => NamedTransfer::Gamma22,
            5 => NamedTransfer::ExtLinear,
            11 => NamedTransfer::St2084Pq,
            13 => NamedTransfer::Hlg,
            other => NamedTransfer::Other(other),
        }
    }

    pub fn code(self) -> u32 {
        match self {
            NamedTransfer::Srgb => 9,
            NamedTransfer::Gamma22 => 2,
            NamedTransfer::ExtLinear => 5,
            NamedTransfer::St2084Pq => 11,
            NamedTransfer::Hlg => 13,
            NamedTransfer::Other(code) => code,
        }
    }

    /// Its name in the protocol.
    pub fn name(self) -> &'static str {
        match self.code() {
            1 => "bt1886",
            2 => "gamma22",
            3 => "gamma28",
            4 => "st240",
            5 => "ext_linear",
            6 => "log_100",
            7 => "log_316",
            8 => "xvycc",
            9 => "srgb",
            10 => "ext_srgb",
            11 => "st2084_pq",
            12 => "st428",
            13 => "hlg",
            14 => "compound_power_2_4",
            _ => "unknown",
        }
    }
}

/// Optional features of the protocol (`wp_color_manager_v1.feature`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Feature {
    /// Parametric image descriptions (named primaries and transfer).
    Parametric,
    /// Values outside the primaries' volume (extended linear).
    ExtendedTargetVolume,
    /// Any other, by its protocol code.
    Other(u32),
}

impl Feature {
    pub fn from_code(code: u32) -> Feature {
        match code {
            1 => Feature::Parametric,
            6 => Feature::ExtendedTargetVolume,
            other => Feature::Other(other),
        }
    }

    pub fn code(self) -> u32 {
        match self {
            Feature::Parametric => 1,
            Feature::ExtendedTargetVolume => 6,
            Feature::Other(code) => code,
        }
    }

    /// Its name in the protocol.
    pub fn name(self) -> &'static str {
        match self.code() {
            0 => "icc_v2_v4",
            1 => "parametric",
            2 => "set_primaries",
            3 => "set_tf_power",
            4 => "set_luminances",
            5 => "set_mastering_display_primaries",
            6 => "extended_target_volume",
            7 => "windows_scrgb",
            8 => "windows_bt2100",
            _ => "unknown",
        }
    }
}

/// The name of a rendering intent's protocol code.
pub fn intent_name(code: u32) -> &'static str {
    match code {
        0 => "perceptual",
        1 => "relative",
        2 => "saturation",
        3 => "absolute",
        4 => "relative_bpc",
        5 => "absolute_no_adaptation",
        _ => "unknown",
    }
}

/// What the compositor's `wp_color_manager_v1` announces to a version 1
/// client (what a Vulkan driver binding version 1 sees too).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Capabilities {
    /// The interface's version the compositor offers.
    pub version: u32,
    pub features: Vec<Feature>,
    pub primaries: Vec<NamedPrimaries>,
    pub transfers: Vec<NamedTransfer>,
    /// Rendering intents, by protocol code ([`intent_name`]).
    pub intents: Vec<u32>,
}

impl Capabilities {
    pub fn has_feature(&self, feature: Feature) -> bool {
        self.features.contains(&feature)
    }

    pub fn has_primaries(&self, primaries: NamedPrimaries) -> bool {
        self.primaries.contains(&primaries)
    }

    pub fn has_transfer(&self, transfer: NamedTransfer) -> bool {
        self.transfers.contains(&transfer)
    }

    /// Whether a surface can be tagged with these primaries and this
    /// transfer: a parametric description of announced names (extended
    /// linear only with values outside the primaries allowed).
    pub fn can_tag(&self, primaries: NamedPrimaries, transfer: NamedTransfer) -> bool {
        self.has_feature(Feature::Parametric)
            && self.has_primaries(primaries)
            && self.has_transfer(transfer)
            && (transfer != NamedTransfer::ExtLinear
                || self.has_feature(Feature::ExtendedTargetVolume))
    }

    /// Whether a Vulkan driver presenting to a surface may already hold
    /// its colour-management object, even for plain sRGB: Mesa's Wayland
    /// window system integration (25.1 and later) makes one on every
    /// swapchain's surface when the compositor announces the `srgb`
    /// primaries with the `srgb` transfer function to a version 1
    /// client. A second one on the same surface is a fatal protocol
    /// error, so such a surface must not be tagged by the application.
    pub fn wsi_claims_srgb_surfaces(&self) -> bool {
        self.has_primaries(NamedPrimaries::Srgb) && self.has_transfer(NamedTransfer::Srgb)
    }

    /// The names of the features, primaries, transfers and intents.
    pub fn names(&self) -> [Vec<&'static str>; 4] {
        [
            self.features.iter().map(|f| f.name()).collect(),
            self.primaries.iter().map(|p| p.name()).collect(),
            self.transfers.iter().map(|t| t.name()).collect(),
            self.intents.iter().map(|i| intent_name(*i)).collect(),
        ]
    }
}

/// Chromaticities `[x, y]` of red, green, blue and white.
pub type Chromaticities = [[f64; 2]; 4];

/// Luminances in cd/m².
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Luminances {
    pub min: f64,
    pub max: f64,
    /// The reference white's.
    pub reference: f64,
}

/// An image description as the compositor gives it
/// (`wp_image_description_info_v1`): the encoding it prefers for a
/// surface, and the display's volume (the target).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Description {
    pub primaries_named: Option<NamedPrimaries>,
    pub primaries: Option<Chromaticities>,
    pub transfer: Option<NamedTransfer>,
    /// A pure power transfer, its exponent.
    pub tf_power: Option<f64>,
    pub luminances: Option<Luminances>,
    /// The display's primaries.
    pub target_primaries: Option<Chromaticities>,
    /// The display's minimum and maximum luminance, in cd/m².
    pub target_luminance: Option<[f64; 2]>,
    pub target_max_cll: Option<u32>,
    pub target_max_fall: Option<u32>,
    /// Whether it came as an ICC profile.
    pub icc: bool,
}

impl Description {
    /// How much brighter than reference white the display goes, when
    /// both are known (1.0 for SDR).
    pub fn headroom(&self) -> Option<f64> {
        let reference = self.luminances?.reference;
        let max = self.target_luminance?[1];
        (reference > 0.0).then(|| (max / reference).max(1.0))
    }
}

/// Protocol chromaticities (`× 1 000 000`) as `[x, y]` pairs.
pub fn chromaticities(v: [i32; 8]) -> Chromaticities {
    let c = |i: usize| [f64::from(v[i]) / 1e6, f64::from(v[i + 1]) / 1e6];
    [c(0), c(2), c(4), c(6)]
}

/// Why there is no image surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unavailable {
    /// Not a Wayland window (X11, another system).
    NotWayland,
    /// The window's handles could not be read.
    Handle(String),
    /// The compositor could not be asked.
    Connection(String),
}

impl fmt::Display for Unavailable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Unavailable::NotWayland => f.write_str("not a Wayland window (X11 or another system)"),
            Unavailable::Handle(e) => write!(f, "the window's handles: {e}"),
            Unavailable::Connection(e) => write!(f, "the compositor: {e}"),
        }
    }
}

impl std::error::Error for Unavailable {}

/// [`Display::attach`] on a system without Wayland.
#[cfg(not(target_os = "linux"))]
#[derive(Debug)]
pub struct Display(());

#[cfg(not(target_os = "linux"))]
impl Display {
    /// Answers [`Unavailable::NotWayland`]: Wayland is Linux only here.
    pub fn attach(
        _window: &(impl raw_window_handle::HasWindowHandle + raw_window_handle::HasDisplayHandle),
    ) -> Result<Display, Unavailable> {
        Err(Unavailable::NotWayland)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps(primaries: &[u32], transfers: &[u32], features: &[u32]) -> Capabilities {
        Capabilities {
            version: 1,
            features: features.iter().map(|c| Feature::from_code(*c)).collect(),
            primaries: primaries
                .iter()
                .map(|c| NamedPrimaries::from_code(*c))
                .collect(),
            transfers: transfers
                .iter()
                .map(|c| NamedTransfer::from_code(*c))
                .collect(),
            intents: vec![0],
        }
    }

    #[test]
    fn codes_round_trip_and_are_named() {
        for code in 0..16 {
            assert_eq!(NamedPrimaries::from_code(code).code(), code);
            assert_eq!(NamedTransfer::from_code(code).code(), code);
            assert_eq!(Feature::from_code(code).code(), code);
        }
        assert_eq!(NamedPrimaries::Bt2020.name(), "bt2020");
        assert_eq!(NamedPrimaries::DisplayP3.name(), "display_p3");
        assert_eq!(NamedTransfer::Gamma22.name(), "gamma22");
        assert_eq!(NamedTransfer::St2084Pq.name(), "st2084_pq");
        assert_eq!(NamedTransfer::from_code(14).name(), "compound_power_2_4");
        assert_eq!(Feature::Parametric.name(), "parametric");
        assert_eq!(intent_name(1), "relative");
    }

    #[test]
    fn tagging_needs_parametric_and_the_names() {
        // KWin-like: no srgb transfer.
        let kwin = caps(&[1, 6, 9], &[1, 2, 5, 11, 13], &[1, 6]);
        assert!(kwin.can_tag(NamedPrimaries::Bt2020, NamedTransfer::Gamma22));
        assert!(kwin.can_tag(NamedPrimaries::Srgb, NamedTransfer::ExtLinear));
        assert!(!kwin.wsi_claims_srgb_surfaces());
        // No parametric feature: nothing.
        let icc_only = caps(&[1, 6], &[2], &[0]);
        assert!(!icc_only.can_tag(NamedPrimaries::Bt2020, NamedTransfer::Gamma22));
        // Extended linear without the extended volume: not tagged.
        let narrow = caps(&[1, 6], &[2, 5], &[1]);
        assert!(!narrow.can_tag(NamedPrimaries::Srgb, NamedTransfer::ExtLinear));
        // GNOME-like, as a version 1 client sees it: srgb + srgb.
        let gnome = caps(&[1, 6, 9], &[1, 2, 3, 5, 9, 11], &[1]);
        assert!(gnome.can_tag(NamedPrimaries::Bt2020, NamedTransfer::Gamma22));
        assert!(gnome.wsi_claims_srgb_surfaces());
        assert_eq!(
            gnome.names()[2],
            [
                "bt1886",
                "gamma22",
                "gamma28",
                "ext_linear",
                "srgb",
                "st2084_pq"
            ]
        );
    }

    #[test]
    fn descriptions_give_the_headroom() {
        let mut d = Description {
            luminances: Some(Luminances {
                min: 0.2,
                max: 80.0,
                reference: 80.0,
            }),
            ..Description::default()
        };
        assert_eq!(d.headroom(), None);
        d.target_luminance = Some([0.05, 400.0]);
        assert_eq!(d.headroom(), Some(5.0));
        d.target_luminance = Some([0.05, 60.0]);
        assert_eq!(d.headroom(), Some(1.0));
        let c = chromaticities([
            708_000, 292_000, 170_000, 797_000, 131_000, 46_000, 312_700, 329_000,
        ]);
        assert_eq!(c[0], [0.708, 0.292]);
        assert_eq!(c[3], [0.3127, 0.329]);
    }
}
