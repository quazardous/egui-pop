//! Whether the pointer gives the keyboard: as the window manager does
//! between windows (`system`), or as the user chose (`click`, `hover`).
//!
//! In **click** mode the keyboard goes where the user clicks (or tabs).
//! In **hover** mode, moving the pointer over a surface (a focus scope:
//! a panel, an editor) gives it the keys, and a text field the pointer
//! moves over takes the keyboard (see [`follow_pointer`]); never while a
//! button is held, a drag is in progress or an overlay (a popup, a menu,
//! a dialog) is open.
//!
//! The window manager's way is read once, at start
//! ([`read_system`]):
//!
//! - GNOME: `gsettings get org.gnome.desktop.wm.preferences focus-mode`
//!   (`click`, or `sloppy` and `mouse` for hover);
//! - KDE: `kreadconfig6` (or `5`) `--file kwinrc --group Windows --key
//!   FocusPolicy` (`ClickToFocus`, or `FocusFollowsMouse`,
//!   `FocusUnderMouse`, `FocusStrictlyUnderMouse` for hover);
//! - Windows: the active window tracking bit of `UserPreferencesMask`
//!   (as `SPI_GETACTIVEWINDOWTRACKING` reports it), read with `reg query`;
//! - macOS and anything else: click.
//!
//! The environment variable [`ENV`] (`click` or `hover`) stands in for
//! the system, for tests and for users whose desktop is not read.
//!
//! ```
//! use egui_pop_core::focus_mode::{self, FocusMode, Preference, SystemFocus};
//!
//! let ctx = egui::Context::default();
//! let modes = focus_mode::get(&ctx);
//! modes.set_system(SystemFocus { mode: FocusMode::Hover, from: Some("test") });
//! assert_eq!(modes.mode(), FocusMode::Hover);
//! modes.set_preference(Preference::Click);
//! assert_eq!(modes.mode(), FocusMode::Click);
//! ```

use std::sync::{Arc, Mutex};

use egui::{Context, Id, Response};
use serde::Serialize;

use crate::state::lock;

/// The environment variable standing in for the system: `click` or
/// `hover` (GNOME's `sloppy` and `mouse` are read as hover).
pub const ENV: &str = "EGUI_POP_FOCUS_MODE";

/// Whether the pointer gives the keyboard.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FocusMode {
    /// The keyboard goes where the user clicks.
    #[default]
    Click,
    /// The keyboard follows the pointer.
    Hover,
}

/// What the user chose.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Preference {
    /// As the window manager does.
    #[default]
    System,
    Click,
    Hover,
}

impl Preference {
    /// The choices, as a setting stores them.
    pub const CHOICES: [&'static str; 3] = ["system", "click", "hover"];

    pub fn parse(text: &str) -> Option<Preference> {
        match text.trim() {
            "system" => Some(Preference::System),
            "click" => Some(Preference::Click),
            "hover" => Some(Preference::Hover),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Preference::System => "system",
            Preference::Click => "click",
            Preference::Hover => "hover",
        }
    }
}

/// The window manager's way, as read, and where it was read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct SystemFocus {
    pub mode: FocusMode,
    /// `GNOME`, `KDE`, `Windows`, `env`; none: not read (click).
    pub from: Option<&'static str>,
}

/// GNOME's `focus-mode` as `gsettings` prints it: `'click'`,
/// `'sloppy'`, `'mouse'`.
pub fn gnome(said: &str) -> Option<FocusMode> {
    match said.trim().trim_matches('\'') {
        "click" => Some(FocusMode::Click),
        "sloppy" | "mouse" => Some(FocusMode::Hover),
        _ => None,
    }
}

/// KDE's `FocusPolicy` (kwinrc, group Windows); none written is a
/// click.
pub fn kde(said: &str) -> Option<FocusMode> {
    match said.trim() {
        "" | "ClickToFocus" => Some(FocusMode::Click),
        "FocusFollowsMouse" | "FocusUnderMouse" | "FocusStrictlyUnderMouse" => {
            Some(FocusMode::Hover)
        }
        _ => None,
    }
}

/// Windows' `UserPreferencesMask` as `reg query` prints it: its first
/// byte's lowest bit is active window tracking (hover).
pub fn windows(said: &str) -> Option<FocusMode> {
    let line = said.lines().find(|l| l.contains("REG_BINARY"))?;
    let hex = line.split_whitespace().last()?;
    let first = u8::from_str_radix(hex.get(..2)?, 16).ok()?;
    Some(if first & 1 == 1 {
        FocusMode::Hover
    } else {
        FocusMode::Click
    })
}

/// The window manager's way from what a reader would see: the
/// environment variable [`ENV`], the desktop (`XDG_CURRENT_DESKTOP`),
/// and `run` (a program and its arguments, its output when it
/// succeeds).
pub fn read_system_with(
    env: Option<&str>,
    desktop: &str,
    run: impl Fn(&str, &[&str]) -> Option<String>,
) -> SystemFocus {
    if let Some(said) = env {
        let mode = match said.trim() {
            "hover" => Some(FocusMode::Hover),
            other => gnome(other),
        };
        return SystemFocus {
            mode: mode.unwrap_or_default(),
            from: Some("env"),
        };
    }
    let found = |mode: Option<FocusMode>, from| mode.map(|mode| SystemFocus { mode, from });
    if cfg!(windows) {
        let said = run(
            "reg",
            &[
                "query",
                r"HKCU\Control Panel\Desktop",
                "/v",
                "UserPreferencesMask",
            ],
        );
        return found(said.as_deref().and_then(windows), Some("Windows")).unwrap_or_default();
    }
    if desktop.to_uppercase().contains("KDE") {
        for tool in ["kreadconfig6", "kreadconfig5"] {
            let said = run(
                tool,
                &[
                    "--file",
                    "kwinrc",
                    "--group",
                    "Windows",
                    "--key",
                    "FocusPolicy",
                ],
            );
            if let Some(s) = found(said.as_deref().and_then(kde), Some("KDE")) {
                return s;
            }
        }
    }
    let said = run(
        "gsettings",
        &["get", "org.gnome.desktop.wm.preferences", "focus-mode"],
    );
    found(said.as_deref().and_then(gnome), Some("GNOME")).unwrap_or_default()
}

/// The window manager's way, read now (a short subprocess on Linux and
/// Windows; click on macOS and on the web).
pub fn read_system() -> SystemFocus {
    let env = std::env::var(ENV).ok();
    if cfg!(any(target_arch = "wasm32", target_os = "macos")) && env.is_none() {
        return SystemFocus::default();
    }
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    read_system_with(env.as_deref(), &desktop, run)
}

#[cfg(not(target_arch = "wasm32"))]
fn run(program: &str, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new(program)
        .args(args)
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(target_arch = "wasm32")]
fn run(_: &str, _: &[&str]) -> Option<String> {
    None
}

#[derive(Debug, Default)]
struct State {
    system: SystemFocus,
    preference: Preference,
}

/// The focus mode of an application: the system's way and the user's
/// choice. Cheap to clone (shared); the application provides it as a
/// service, and a context uses one ([`get`], [`set`]).
#[derive(Clone, Debug, Default)]
pub struct FocusModes(Arc<Mutex<State>>);

/// What the debug bridge shows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Report {
    pub mode: FocusMode,
    pub preference: Preference,
    pub system: FocusMode,
    /// Where the system's way was read (`GNOME`, `env`...), none: not
    /// read.
    pub from: Option<&'static str>,
}

impl FocusModes {
    pub fn new(system: SystemFocus) -> Self {
        let m = Self::default();
        m.set_system(system);
        m
    }

    pub fn system(&self) -> SystemFocus {
        lock(&self.0).system
    }

    pub fn set_system(&self, system: SystemFocus) {
        lock(&self.0).system = system;
    }

    pub fn preference(&self) -> Preference {
        lock(&self.0).preference
    }

    pub fn set_preference(&self, preference: Preference) {
        lock(&self.0).preference = preference;
    }

    /// The mode in force: the user's choice, else the system's way.
    pub fn mode(&self) -> FocusMode {
        let s = lock(&self.0);
        match s.preference {
            Preference::System => s.system.mode,
            Preference::Click => FocusMode::Click,
            Preference::Hover => FocusMode::Hover,
        }
    }

    pub fn report(&self) -> Report {
        let s = lock(&self.0);
        Report {
            mode: match s.preference {
                Preference::System => s.system.mode,
                Preference::Click => FocusMode::Click,
                Preference::Hover => FocusMode::Hover,
            },
            preference: s.preference,
            system: s.system.mode,
            from: s.system.from,
        }
    }

    /// What `system` means here, for a settings form: `As the system
    /// (GNOME: hover)`.
    pub fn system_said(&self) -> String {
        let s = self.system();
        let way = match s.mode {
            FocusMode::Click => "click",
            FocusMode::Hover => "hover",
        };
        match s.from {
            Some(from) => format!("As the system ({from}: {way})"),
            None => "As the system (not read: click)".into(),
        }
    }
}

fn key() -> Id {
    Id::new("egui_pop_core::focus_mode")
}

/// The context's focus modes (created on first use: the system not read,
/// click).
pub fn get(ctx: &Context) -> FocusModes {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<FocusModes>(key()).clone())
}

/// Makes a context use these focus modes.
pub fn set(ctx: &Context, modes: &FocusModes) {
    ctx.data_mut(|d| d.insert_temp(key(), modes.clone()));
}

/// The mode in force in a context.
pub fn mode(ctx: &Context) -> FocusMode {
    get(ctx).mode()
}

#[derive(Debug, Default)]
struct Pointer {
    pass: u64,
    before: Option<egui::Pos2>,
    now: Option<egui::Pos2>,
}

/// Whether the pointer is somewhere else than in the previous pass (its
/// first position counts as a move).
fn pointer_moved(ctx: &Context) -> bool {
    let st = crate::state::shared::<Pointer>(ctx, "focus_mode.pointer");
    let mut p = lock(&st);
    let pass = crate::state::pass(ctx);
    if p.pass != pass {
        p.pass = pass;
        p.before = p.now;
        p.now = ctx.input(|i| i.pointer.hover_pos());
    }
    p.now.is_some() && p.now != p.before
}

/// Whether the pointer may move the keyboard focus in this pass: hover
/// mode, the pointer moved, no button held, no drag, no overlay open.
pub fn pointer_may_focus(ctx: &Context) -> bool {
    mode(ctx) == FocusMode::Hover
        && pointer_moved(ctx)
        && !ctx.input(|i| i.pointer.any_down())
        && ctx.dragged_id().is_none()
        && crate::overlay::stack(ctx).is_empty()
}

/// In hover mode, gives the keyboard to a widget (a text field) when the
/// pointer moves over it. Call it with the widget's response.
pub fn follow_pointer(response: &Response) {
    if response.hovered() && !response.has_focus() && pointer_may_focus(&response.ctx) {
        response.request_focus();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_managers_ways_read() {
        assert_eq!(gnome("'click'\n"), Some(FocusMode::Click));
        assert_eq!(gnome("'sloppy'"), Some(FocusMode::Hover));
        assert_eq!(gnome("'mouse'"), Some(FocusMode::Hover));
        assert_eq!(gnome("nothing"), None);
        assert_eq!(kde(""), Some(FocusMode::Click));
        assert_eq!(kde("FocusFollowsMouse\n"), Some(FocusMode::Hover));
        assert_eq!(kde("FocusStrictlyUnderMouse"), Some(FocusMode::Hover));
        assert_eq!(kde("ClickToFocus"), Some(FocusMode::Click));
        let reg = "\r\nHKEY_CURRENT_USER\\Control Panel\\Desktop\r\n    \
                   UserPreferencesMask    REG_BINARY    9E1E078012000000\r\n";
        assert_eq!(windows(reg), Some(FocusMode::Click));
        assert_eq!(
            windows(&reg.replace("9E1E", "9F1E")),
            Some(FocusMode::Hover)
        );
        assert_eq!(windows("ERROR"), None);
    }

    #[test]
    fn the_reader_prefers_the_environment_then_the_desktop() {
        let gnome_says = |said: &'static str| {
            move |p: &str, _: &[&str]| (p == "gsettings").then(|| said.to_string())
        };
        assert_eq!(
            read_system_with(Some("hover"), "GNOME", gnome_says("'click'")),
            SystemFocus {
                mode: FocusMode::Hover,
                from: Some("env")
            }
        );
        if cfg!(windows) {
            return;
        }
        assert_eq!(
            read_system_with(None, "GNOME", gnome_says("'sloppy'")),
            SystemFocus {
                mode: FocusMode::Hover,
                from: Some("GNOME")
            }
        );
        let kde_says =
            |p: &str, _: &[&str]| (p == "kreadconfig5").then(|| "FocusUnderMouse\n".to_string());
        assert_eq!(
            read_system_with(None, "KDE", kde_says),
            SystemFocus {
                mode: FocusMode::Hover,
                from: Some("KDE")
            }
        );
        assert_eq!(
            read_system_with(None, "", |_: &str, _: &[&str]| None),
            SystemFocus::default()
        );
    }

    #[test]
    fn the_users_choice_beats_the_system() {
        let m = FocusModes::new(SystemFocus {
            mode: FocusMode::Hover,
            from: Some("GNOME"),
        });
        assert_eq!(m.mode(), FocusMode::Hover);
        assert_eq!(m.system_said(), "As the system (GNOME: hover)");
        m.set_preference(Preference::Click);
        assert_eq!(m.mode(), FocusMode::Click);
        assert_eq!(m.report().system, FocusMode::Hover);
        assert_eq!(Preference::parse("hover"), Some(Preference::Hover));
        assert_eq!(Preference::parse("x"), None);
    }
}
