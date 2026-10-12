//! Where an application keeps what it writes for itself: its state, its
//! logs, its crash reports. Read from the environment, as each platform
//! names these places:
//!
//! | | State | Logs |
//! |---|---|---|
//! | Linux and BSD | `$XDG_STATE_HOME/<app>` (`~/.local/state/<app>`) | `<state>/logs` |
//! | macOS | `~/Library/Application Support/<app>` | `~/Library/Logs/<app>` |
//! | Windows | `%LOCALAPPDATA%\<app>` | `<state>\logs` |
//!
//! Crash reports go to `<state>/crashes`. Inside a Flatpak sandbox,
//! `XDG_STATE_HOME` points into the application's own directory, so the
//! same code finds the sandbox's place.
//!
//! Owned rather than `dirs` or `directories` (MIT OR Apache-2.0): three
//! environment variables, and those crates have no state directory on
//! macOS and Windows anyway.
//!
//! ```
//! let dir = egui_pop_platform::dirs::state_dir_with("demo", |name| match name {
//!     "XDG_STATE_HOME" => Some("/state".into()),
//!     _ => None,
//! });
//! if cfg!(all(unix, not(target_os = "macos"))) {
//!     assert_eq!(dir, Some("/state/demo".into()));
//! }
//! ```

use std::path::{Path, PathBuf};

/// The application's state directory, from the environment `env` gives
/// (none: the variables it needs are missing).
pub fn state_dir_with(app: &str, env: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let absolute = |v: Option<String>| v.map(PathBuf::from).filter(|p| p.is_absolute());
    let base = if cfg!(windows) {
        absolute(env("LOCALAPPDATA"))?
    } else if cfg!(target_os = "macos") {
        absolute(env("HOME"))?.join("Library/Application Support")
    } else {
        match absolute(env("XDG_STATE_HOME")) {
            Some(p) => p,
            None => absolute(env("HOME"))?.join(".local/state"),
        }
    };
    Some(base.join(app))
}

/// The application's state directory (see the module).
pub fn state_dir(app: &str) -> Option<PathBuf> {
    state_dir_with(app, |name| std::env::var(name).ok())
}

/// The application's log directory (see the module).
pub fn log_dir(app: &str) -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        let home = std::env::var("HOME").ok().map(PathBuf::from)?;
        return Some(home.join("Library/Logs").join(app));
    }
    state_dir(app).map(|d| d.join("logs"))
}

/// The application's crash report directory: `<state>/crashes`.
pub fn crash_dir(app: &str) -> Option<PathBuf> {
    state_dir(app).map(|d| d.join("crashes"))
}

/// Opens a file or a folder with what the system opens it with
/// (`xdg-open`, `open`, `explorer`); returns at once.
pub fn reveal(path: &Path) -> Result<(), String> {
    let program = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::process::Command::new(program)
            .arg(path)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("{program}: {e}"))
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (program, path);
        Err("nothing opens files on the web".into())
    }
}
