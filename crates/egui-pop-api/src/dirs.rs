//! Where an application keeps its settings: the user's configuration
//! folder, as each platform names it.
//!
//! | | Configuration |
//! |---|---|
//! | Linux and BSD | `$XDG_CONFIG_HOME/<app>` (`~/.config/<app>`) |
//! | macOS | `~/Library/Application Support/<app>` |
//! | Windows | `%APPDATA%\<app>` |
//!
//! `AppBuilder::persist_in_config` saves the settings, the layout and
//! the window's place there. Inside a Flatpak sandbox,
//! `XDG_CONFIG_HOME` points into the application's own directory, so
//! the same code finds the sandbox's place. The state, the logs and the
//! caches are elsewhere (`egui_pop_platform::dirs`).
//!
//! ```
//! let dir = egui_pop_api::dirs::config_dir_with("demo", |name| match name {
//!     "XDG_CONFIG_HOME" => Some("/config".into()),
//!     _ => None,
//! });
//! if cfg!(all(unix, not(target_os = "macos"))) {
//!     assert_eq!(dir, Some("/config/demo".into()));
//! }
//! ```

use std::path::PathBuf;

/// The application's configuration folder, from the environment `env`
/// gives (`None`: the variables it needs are missing, or on the web).
pub fn config_dir_with(app: &str, env: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    if cfg!(target_arch = "wasm32") {
        return None;
    }
    let absolute = |v: Option<String>| v.map(PathBuf::from).filter(|p| p.is_absolute());
    let base = if cfg!(windows) {
        absolute(env("APPDATA"))?
    } else if cfg!(target_os = "macos") {
        absolute(env("HOME"))?.join("Library/Application Support")
    } else {
        match absolute(env("XDG_CONFIG_HOME")) {
            Some(p) => p,
            None => absolute(env("HOME"))?.join(".config"),
        }
    };
    Some(base.join(app))
}

/// The application's configuration folder (see the module).
pub fn config_dir(app: &str) -> Option<PathBuf> {
    config_dir_with(app, |name| std::env::var(name).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_folder_follows_the_platform() {
        let env = |name: &str| match name {
            "HOME" => Some("/home/me".to_string()),
            "APPDATA" => Some("C:\\Users\\me\\AppData\\Roaming".to_string()),
            _ => None,
        };
        let dir = config_dir_with("swatch", env).unwrap();
        if cfg!(windows) {
            assert!(dir.ends_with("Roaming\\swatch"), "{dir:?}");
        } else if cfg!(target_os = "macos") {
            assert_eq!(
                dir,
                PathBuf::from("/home/me/Library/Application Support/swatch")
            );
        } else {
            assert_eq!(dir, PathBuf::from("/home/me/.config/swatch"));
        }
        // A relative or missing base names no place.
        assert_eq!(config_dir_with("swatch", |_| None), None);
        let relative =
            |name: &str| (name == "XDG_CONFIG_HOME" || name == "HOME").then(|| "rel".to_string());
        if cfg!(all(unix, not(target_os = "macos"))) {
            assert_eq!(config_dir_with("swatch", relative), None);
        }
    }
}
