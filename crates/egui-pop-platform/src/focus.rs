//! The window manager's focus mode (click to focus, or focus following
//! the pointer), read in the process instead of through a helper
//! program.
//!
//! egui-pop follows the system's focus mode (`egui_pop_core::focus_mode`);
//! by default it asks `gsettings`, `kreadconfig` or `reg` at start. This
//! reads the same answers in-process:
//!
//! | | Read from | Followed while running |
//! |---|---|---|
//! | GNOME (and desktops using its settings) | the XDG Settings portal, `org.gnome.desktop.wm.preferences` `focus-mode` (`click`, `sloppy`, `mouse`) | yes: read again when the window gets the focus back |
//! | KDE Plasma | `kwinrc`, group `Windows`, `FocusPolicy`, read as a file (the portal does not carry KWin's settings) | yes, the same way |
//! | Windows | the active window tracking bit of `HKCU\Control Panel\Desktop\UserPreferencesMask`, what `SystemParametersInfo(SPI_GETACTIVEWINDOWTRACKING)` reports, read through the registry API (winreg; egui-pop has no unsafe code) | yes, the same way |
//! | macOS | nothing to read: its window manager has no focus following the pointer | |
//! | Other Linux desktops, the web | nothing read: click | |
//!
//! The `EGUI_POP_FOCUS_MODE` variable (`click`, `hover`) still stands in
//! for the system. [`read`] reads now (a D-Bus call on Linux, with a
//! timeout); give it to `AppBuilder::system_focus` so `run_native`
//! starts no helper. [`FocusModePlugin`] reads it again, from a thread,
//! each time the window gets the focus back, so a change in the
//! desktop's settings applies without a restart.
//!
//! ```
//! use egui_pop_core::focus_mode::FocusMode;
//! use egui_pop_platform::focus;
//!
//! let kwinrc = "[Windows]\nFocusPolicy=FocusFollowsMouse\n";
//! assert_eq!(focus::kwin_policy(kwinrc), Some(FocusMode::Hover));
//! let read = focus::read_with(None, "KDE", &focus::Sources {
//!     kwinrc: &|| Some(kwinrc.into()),
//!     ..focus::Sources::NONE
//! });
//! assert_eq!(read.mode, FocusMode::Hover);
//! assert_eq!(read.from, Some("KDE"));
//! ```

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use egui_pop_api::app::FOCUS_MODE;
use egui_pop_api::hooks::On;
use egui_pop_api::{Ctx, Plugin, PluginError};
use egui_pop_core::focus_mode::{self, FocusMode, SystemFocus};
use egui_pop_core::lock;
use egui_pop_model::ids::PluginId;

/// The plugin's id.
pub const PLUGIN: PluginId = PluginId::from_static("egui_pop.focus_mode");

/// Where the system's answer comes from, each a function giving the raw
/// answer (none: not there). [`Sources::SYSTEM`] reads the real ones.
#[derive(Clone, Copy)]
pub struct Sources<'a> {
    /// The text of `kwinrc`.
    pub kwinrc: &'a dyn Fn() -> Option<String>,
    /// GNOME's `focus-mode` through the Settings portal.
    pub portal: &'a dyn Fn() -> Option<String>,
    /// The first byte of Windows' `UserPreferencesMask`.
    pub registry: &'a dyn Fn() -> Option<u8>,
}

impl std::fmt::Debug for Sources<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sources").finish_non_exhaustive()
    }
}

impl Sources<'static> {
    /// Nothing answers.
    pub const NONE: Sources<'static> = Sources {
        kwinrc: &|| None,
        portal: &|| None,
        registry: &|| None,
    };

    /// The real places.
    pub const SYSTEM: Sources<'static> = Sources {
        kwinrc: &read_kwinrc,
        portal: &read_portal,
        registry: &read_registry,
    };
}

/// KWin's `FocusPolicy` in a `kwinrc` (none written: click).
pub fn kwin_policy(kwinrc: &str) -> Option<FocusMode> {
    let mut in_windows = false;
    let mut policy = None;
    for line in kwinrc.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_windows = line == "[Windows]";
        } else if in_windows && let Some(v) = line.strip_prefix("FocusPolicy=") {
            policy = Some(v.trim().to_string());
        }
    }
    focus_mode::kde(policy.as_deref().unwrap_or(""))
}

/// The system's focus mode from what `sources` answer (see the module).
pub fn read_with(env: Option<&str>, desktop: &str, sources: &Sources<'_>) -> SystemFocus {
    if let Some(said) = env {
        let mode = match said.trim() {
            "hover" => Some(FocusMode::Hover),
            other => focus_mode::gnome(other),
        };
        return SystemFocus {
            mode: mode.unwrap_or_default(),
            from: Some("env"),
        };
    }
    let found = |mode: Option<FocusMode>, from| mode.map(|mode| SystemFocus { mode, from });
    if cfg!(windows) {
        let mode = (sources.registry)().map(|b| {
            if b & 1 == 1 {
                FocusMode::Hover
            } else {
                FocusMode::Click
            }
        });
        return found(mode, Some("Windows")).unwrap_or_default();
    }
    if cfg!(target_os = "macos") {
        return SystemFocus::default();
    }
    if desktop.to_uppercase().contains("KDE")
        && let Some(s) = found(
            (sources.kwinrc)().as_deref().and_then(kwin_policy),
            Some("KDE"),
        )
    {
        return s;
    }
    found(
        (sources.portal)().as_deref().and_then(focus_mode::gnome),
        Some("GNOME"),
    )
    .unwrap_or_default()
}

/// The system's focus mode, read now in-process (see the module).
pub fn read() -> SystemFocus {
    let env = std::env::var(focus_mode::ENV).ok();
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    read_with(env.as_deref(), &desktop, &Sources::SYSTEM)
}

fn read_kwinrc() -> Option<String> {
    let config = std::env::var("XDG_CONFIG_HOME")
        .ok()
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| std::path::Path::new(&h).join(".config"))
        })?;
    std::fs::read_to_string(config.join("kwinrc")).ok()
}

#[cfg(all(unix, not(target_os = "macos"), not(target_arch = "wasm32")))]
fn read_portal() -> Option<String> {
    use std::time::Duration;
    let ask = async {
        let settings = ashpd::desktop::settings::Settings::new().await.ok()?;
        settings
            .read::<String>("org.gnome.desktop.wm.preferences", "focus-mode")
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
fn read_portal() -> Option<String> {
    None
}

#[cfg(windows)]
fn read_registry() -> Option<u8> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;
    let desktop = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Control Panel\\Desktop")
        .ok()?;
    desktop
        .get_raw_value("UserPreferencesMask")
        .ok()?
        .bytes
        .first()
        .copied()
}

#[cfg(not(windows))]
fn read_registry() -> Option<u8> {
    None
}

/// Reads the focus mode in-process at start and again each time the
/// window gets the focus back, from a thread (see the module).
#[derive(Clone, Debug, Default)]
pub struct FocusModePlugin {
    /// Read instead of the system (tests).
    fixed: Option<SystemFocus>,
}

impl FocusModePlugin {
    pub fn new() -> Self {
        Self::default()
    }

    /// Answers `system` instead of reading (tests, the gallery).
    pub fn fixed(system: SystemFocus) -> Self {
        Self {
            fixed: Some(system),
        }
    }
}

impl Plugin for FocusModePlugin {
    fn id(&self) -> PluginId {
        PLUGIN
    }

    fn title(&self) -> String {
        "Focus mode".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let modes = ctx
            .app()
            .service(&FOCUS_MODE)
            .ok_or_else(|| PluginError::Failed("no focus mode service".into()))?;
        let read_now: Arc<dyn Fn() -> SystemFocus + Send + Sync> = match self.fixed {
            Some(f) => Arc::new(move || f),
            None => Arc::new(read),
        };
        // What the reading thread found, taken at the next pass.
        let found: Arc<Mutex<Option<SystemFocus>>> = Arc::default();
        let reading = Arc::new(AtomicBool::new(false));
        let ask = {
            let (found, reading, read_now) = (found.clone(), reading.clone(), read_now.clone());
            let waker = ctx.app().egui();
            move || {
                if reading.swap(true, Ordering::AcqRel) {
                    return;
                }
                let (found, done, read_now, waker) = (
                    found.clone(),
                    reading.clone(),
                    read_now.clone(),
                    waker.clone(),
                );
                let spawned = std::thread::Builder::new()
                    .name("egui-pop focus mode".into())
                    .spawn(move || {
                        *lock(&found) = Some(read_now());
                        done.store(false, Ordering::Release);
                        waker.request_repaint();
                    });
                if spawned.is_err() {
                    reading.store(false, Ordering::Release);
                }
            }
        };
        if self.fixed.is_some() {
            modes.set_system(read_now());
        } else {
            ask();
        }
        let mut focused = true;
        ctx.on(On::Pass, move |app, _| {
            if let Some(system) = lock(&found).take()
                && system != modes.system()
            {
                modes.set_system(system);
                app.egui().request_repaint();
            }
            let now = app.egui().input(|i| i.viewport().focused.unwrap_or(true));
            if now && !focused {
                ask();
            }
            focused = now;
        });
        Ok(())
    }
}
