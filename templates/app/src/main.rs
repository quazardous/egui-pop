//! A first egui-pop application: the end state of the first-app guide
//! (`docs/first-app.md` in egui-pop's repository), made from its
//! template.
//!
//! A colour swatch with two parameters, hue and size:
//!
//! - [`builder`] says what the application is made of: egui-pop's
//!   default plugins, the history panel, a kit of its own and the
//!   [`Swatch`] plugin;
//! - [`Swatch`] (`src/swatch.rs`) registers everything else: the values,
//!   a setting, two commands (one for people, one for agents) and two
//!   panels;
//! - [`main`] opens it in a window, its settings and layout kept;
//! - `src/tests.rs` drives it headless, and keeps a picture of it.

mod swatch;

#[cfg(test)]
mod tests;

use std::path::PathBuf;

use egui_pop::api::history_view::HistoryPlugin;
use egui_pop::eframe;
use egui_pop::prelude::*;

use swatch::Swatch;

/// The application's name, in its window's title and its About box.
const NAME: &str = "Swatch";

/// The application's kit (`kit/kit.yaml`, built into the binary): a
/// theme of its own, named `swatch`, over egui-pop's `pop-dark`.
fn kit() -> egui_pop::tokens::kit::KitData {
    egui_pop::tokens::kit!("../kit")
}

/// The application, ready to be built (tests) or run ([`main`]).
fn builder() -> AppBuilder {
    App::builder()
        .name(NAME)
        // The palette, dialogs, toasts, the settings window, the
        // window's place and the saved arrangements.
        .plugins(DefaultPlugins)
        // The History panel (View menu) and the agents' `history.*`.
        .plugin(HistoryPlugin::default())
        .kit(kit())
        .theme(ThemeSpec::new("swatch"))
        .plugin(Swatch)
}

/// Where the settings, the layout and the window's place are kept: the
/// user's configuration folder, in a folder named after the crate.
fn settings_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(std::env::temp_dir);
    base.join(env!("CARGO_PKG_NAME"))
}

fn main() -> eframe::Result {
    builder()
        .persist(settings_dir())
        .run_native(NAME, eframe::NativeOptions::default())
}
