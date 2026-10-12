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
//!
//! Each service names its moments as steps (ADR 0011): a plugin may
//! observe, veto or redirect, replace or extend them; the application's
//! kernel stays in charge through the host (ADR 0010). What each needs
//! from the package (a desktop file, a manifest, a bundle) is in
//! `docs/packaging.md`.
//!
//! - [`dirs`]: where the application keeps its state and logs;
//! - [`window`]: the main window brought to the front.

pub mod dirs;
#[cfg(feature = "instance")]
pub mod instance;
#[cfg(feature = "keys")]
pub mod keys;
#[cfg(feature = "logs")]
pub mod logs;
#[cfg(feature = "notify")]
pub mod notify;
#[cfg(feature = "tray")]
pub mod tray;
pub mod window;
