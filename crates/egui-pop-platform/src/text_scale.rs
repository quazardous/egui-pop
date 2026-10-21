//! The system's text scaling (the desktop's "larger text"), read in the
//! process, to seed egui-pop's text size setting.
//!
//! It is not the display's scale: winit gives that as the pixels per
//! point, and egui-pop already follows it. This is the separate factor a
//! desktop applies to text only.
//!
//! | | Read from |
//! |---|---|
//! | GNOME on Wayland (and desktops using its settings) | the XDG Settings portal, `org.gnome.desktop.interface` `text-scaling-factor` |
//! | KDE Plasma on Wayland | `forceFontDPIWayland`, else `forceFontDPI`, in `kcmfonts` then `kdeglobals` (group `General`), read as files, over 96 |
//! | Linux on X11 | nothing: GNOME and Plasma fold the text scaling into `Xft.dpi`, which winit already reads as the display's scale; reading it again would count it twice |
//! | Windows | "Make text bigger": `HKCU\Software\Microsoft\Accessibility` `TextScaleFactor` (100 to 225), read through the registry API (winreg) |
//! | macOS, other desktops, the web | nothing: 1 |
//!
//! The `EGUI_POP_TEXT_SCALE` variable (`1.25` or `125%`) stands in for the
//! system. [`read`] reads now (a D-Bus call on GNOME, with a timeout);
//! give it to `AppBuilder::system_text_scale`, which makes it the
//! default of the text size until the user chooses one:
//!
//! ```no_run
//! let app = egui_pop_api::App::builder()
//!     .system_text_scale(egui_pop_platform::text_scale::read());
//! ```
//!
//! [`read_with`] takes each source as a function, so a test answers in
//! place of the system:
//!
//! ```
//! use egui_pop_platform::text_scale::{self, Session, Sources};
//!
//! let kcmfonts = "[General]\nforceFontDPI=120\n";
//! assert_eq!(text_scale::kde_font_dpi(kcmfonts, true), Some(1.25));
//! let read = text_scale::read_with(None, &Session::wayland("KDE"), &Sources {
//!     kcmfonts: &|| Some(kcmfonts.into()),
//!     ..Sources::NONE
//! });
//! if cfg!(all(unix, not(target_os = "macos"))) {
//!     assert_eq!((read.factor, read.from), (1.25, Some("KDE")));
//! }
//! ```

use egui_pop_api::text::SystemTextScale;

/// The variable standing in for the system (`1.25`, `125%`).
pub const ENV: &str = "EGUI_POP_TEXT_SCALE";

/// The DPI a KDE factor is measured against.
const BASE_DPI: f32 = 96.0;

/// The session the application runs in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Session {
    /// `XDG_CURRENT_DESKTOP` (`GNOME`, `KDE`, `ubuntu:GNOME`).
    pub desktop: String,
    /// A Wayland session (winit picks Wayland when `WAYLAND_DISPLAY` is
    /// set), not X11.
    pub wayland: bool,
}

impl Session {
    /// A Wayland session on a desktop.
    pub fn wayland(desktop: &str) -> Self {
        Self {
            desktop: desktop.into(),
            wayland: true,
        }
    }

    /// An X11 session on a desktop.
    pub fn x11(desktop: &str) -> Self {
        Self {
            desktop: desktop.into(),
            wayland: false,
        }
    }

    /// This process's session, from its environment.
    pub fn current() -> Self {
        Self {
            desktop: std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default(),
            wayland: std::env::var_os("WAYLAND_DISPLAY").is_some_and(|d| !d.is_empty()),
        }
    }
}

/// Where the system's answer comes from, each a function giving the raw
/// answer (none: not there). [`Sources::SYSTEM`] reads the real ones.
#[derive(Clone, Copy)]
pub struct Sources<'a> {
    /// GNOME's `text-scaling-factor` through the Settings portal.
    pub portal: &'a dyn Fn() -> Option<f64>,
    /// The text of KDE's `kcmfonts`.
    pub kcmfonts: &'a dyn Fn() -> Option<String>,
    /// The text of KDE's `kdeglobals`.
    pub kdeglobals: &'a dyn Fn() -> Option<String>,
    /// Windows' `TextScaleFactor`, a percentage.
    pub registry: &'a dyn Fn() -> Option<u32>,
}

impl std::fmt::Debug for Sources<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sources").finish_non_exhaustive()
    }
}

impl Sources<'static> {
    /// Nothing answers.
    pub const NONE: Sources<'static> = Sources {
        portal: &|| None,
        kcmfonts: &|| None,
        kdeglobals: &|| None,
        registry: &|| None,
    };

    /// The real places.
    pub const SYSTEM: Sources<'static> = Sources {
        portal: &read_portal,
        kcmfonts: &|| read_config("kcmfonts"),
        kdeglobals: &|| read_config("kdeglobals"),
        registry: &read_registry,
    };
}

/// A factor as `EGUI_POP_TEXT_SCALE` writes it: `1.25` or `125%`.
pub fn parse_env(said: &str) -> Option<f32> {
    let said = said.trim();
    let factor = match said.strip_suffix('%') {
        Some(p) => p.trim().parse::<f32>().ok()? / 100.0,
        None => said.parse::<f32>().ok()?,
    };
    positive(factor)
}

/// KDE's forced font DPI in a configuration file's `General` group
/// (`forceFontDPIWayland` first on Wayland), as a factor of 96; 0 or
/// absent: none.
pub fn kde_font_dpi(text: &str, wayland: bool) -> Option<f32> {
    let mut in_general = false;
    let (mut x11, mut on_wayland) = (None, None);
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_general = line == "[General]";
            continue;
        }
        if !in_general {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let dpi = value.trim().parse::<f32>().ok();
        match key.trim() {
            "forceFontDPI" => x11 = dpi,
            "forceFontDPIWayland" => on_wayland = dpi,
            _ => {}
        }
    }
    let dpi = if wayland { on_wayland.or(x11) } else { x11 }?;
    positive(dpi / BASE_DPI)
}

fn positive(factor: f32) -> Option<f32> {
    (factor.is_finite() && factor > 0.0).then_some(factor)
}

/// The system's text scaling from what `sources` answer (see the
/// module); `env` is `EGUI_POP_TEXT_SCALE`'s value.
pub fn read_with(env: Option<&str>, session: &Session, sources: &Sources<'_>) -> SystemTextScale {
    if let Some(factor) = env.and_then(parse_env) {
        return SystemTextScale::new(factor, "env");
    }
    if cfg!(windows) {
        return (sources.registry)()
            .and_then(|p| positive(p as f32 / 100.0))
            .map(|f| SystemTextScale::new(f, "Windows"))
            .unwrap_or_default();
    }
    if cfg!(any(target_os = "macos", target_arch = "wasm32")) || !session.wayland {
        // On X11 the text scaling is in Xft.dpi, the display's scale.
        return SystemTextScale::default();
    }
    if session.desktop.to_uppercase().contains("KDE") {
        let kde = (sources.kcmfonts)()
            .and_then(|t| kde_font_dpi(&t, true))
            .or_else(|| (sources.kdeglobals)().and_then(|t| kde_font_dpi(&t, true)));
        if let Some(factor) = kde {
            return SystemTextScale::new(factor, "KDE");
        }
    }
    (sources.portal)()
        .and_then(|f| positive(f as f32))
        .map(|f| SystemTextScale::new(f, "GNOME"))
        .unwrap_or_default()
}

/// The system's text scaling, read now in-process (see the module).
pub fn read() -> SystemTextScale {
    let env = std::env::var(ENV).ok();
    read_with(env.as_deref(), &Session::current(), &Sources::SYSTEM)
}

fn read_config(name: &str) -> Option<String> {
    let config = std::env::var("XDG_CONFIG_HOME")
        .ok()
        .filter(|c| !c.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| std::path::Path::new(&h).join(".config"))
        })?;
    std::fs::read_to_string(config.join(name)).ok()
}

#[cfg(all(unix, not(target_os = "macos"), not(target_arch = "wasm32")))]
fn read_portal() -> Option<f64> {
    use std::time::Duration;
    let ask = async {
        let settings = ashpd::desktop::settings::Settings::new().await.ok()?;
        settings
            .read::<f64>("org.gnome.desktop.interface", "text-scaling-factor")
            .await
            .ok()
    };
    let timeout = async {
        async_io::Timer::after(Duration::from_secs(2)).await;
        None
    };
    futures_lite::future::block_on(futures_lite::future::or(ask, timeout))
}

#[cfg(not(all(unix, not(target_os = "macos"), not(target_arch = "wasm32"))))]
fn read_portal() -> Option<f64> {
    None
}

#[cfg(windows)]
fn read_registry() -> Option<u32> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Software\\Microsoft\\Accessibility")
        .ok()?
        .get_value::<u32, _>("TextScaleFactor")
        .ok()
}

#[cfg(not(windows))]
fn read_registry() -> Option<u32> {
    None
}
