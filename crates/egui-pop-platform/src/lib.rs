//! Platform services for egui-pop applications, each an opt-in plugin
//! (a Cargo feature of this crate, on by default, and a plugin the
//! application adds):
//!
//! - [`instance`]: a single instance; a second launch hands its
//!   arguments and links to the running one (`instance.second_launch`,
//!   `deeplink.open`).
//! - [`notify`]: desktop notifications with actions routed to commands,
//!   falling back to toasts (`notify.show`, `notify.action`).
//! - [`tray`]: a tray icon whose menu is a declared egui-pop menu
//!   (`tray.activate`).
//! - [`keys`]: global shortcuts, the key bindings of the system scope
//!   registered with the desktop (`keys.system`).
//! - [`logs`]: logs to a rotating file in the platform's log directory,
//!   with a level setting.
//! - [`crash`]: crash reports written by a panic hook and offered on the
//!   next start (`crash.report`); nothing is sent.
//! - [`focus`]: the window manager's focus mode read in-process (the
//!   Settings portal, `kwinrc`, the registry), followed while running.
//! - [`open`]: the links service's system opener: local files and
//!   folders opened with the system's default application, and Reveal
//!   (the file manager with the file selected), through the portals in
//!   Flatpak.
//! - [`text_scale`]: the system's text scaling read in-process (the
//!   Settings portal, KDE's font DPI, the registry), the default of the
//!   text size setting.
//! - [`clipboard`]: the system clipboard over arboard (files, images,
//!   HTML, text, the primary selection), the clipboard's system backend.
//! - [`thumbnails`] (off by default): the system's thumbnails as a
//!   preview provider of the file picker (the freedesktop cache and
//!   thumbnailers; not yet on Windows and macOS).
//!
//! Each service names its moments as steps (ADR 0011): a plugin may
//! observe, veto or redirect, replace or extend them; the application's
//! kernel stays in charge through the host (ADR 0010). What each needs
//! from the package (a desktop file, a manifest, a bundle) is in
//! `docs/packaging.md`.
//!
//! - [`dirs`]: where the application keeps its state and logs;
//! - [`window`]: the main window brought to the front;
//! - [`strings`]: the text the services show themselves.

#[cfg(all(feature = "clipboard", not(target_arch = "wasm32")))]
pub mod clipboard;
#[cfg(feature = "crash")]
pub mod crash;
pub mod dirs;
#[cfg(feature = "focus")]
pub mod focus;
#[cfg(feature = "instance")]
pub mod instance;
#[cfg(feature = "keys")]
pub mod keys;
#[cfg(feature = "logs")]
pub mod logs;
#[cfg(feature = "notify")]
pub mod notify;
#[cfg(feature = "open")]
pub mod open;
pub mod strings;
#[cfg(feature = "text-scale")]
pub mod text_scale;
#[cfg(feature = "thumbnails")]
pub mod thumbnails;
#[cfg(feature = "tray")]
pub mod tray;
pub mod window;
