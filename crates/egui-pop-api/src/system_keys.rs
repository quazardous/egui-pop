//! Shortcuts in the system scope: keys bound with
//! `BindOptions::system()` (`KeyScope::System`, `system` in keymap
//! files) also run their command while the application is not focused,
//! when a plugin registers them with the desktop (the platform plugin of
//! `egui-pop-platform`). Inside the window they fire as global bindings
//! do.
//!
//! Whoever registers them says how it went through the [`SYSTEM_KEYS`]
//! service: whether the desktop offers global shortcuts at all
//! ([`Support`]), and each binding's [`KeyStatus`]. The shortcuts editor
//! shows it next to each command, so a shortcut that cannot work outside
//! the window says so instead of failing silently; without the service,
//! the editor says nothing registers them.
//!
//! ```
//! use egui_pop_api::system_keys::{KeyStatus, Support, SystemKeys};
//!
//! let keys = SystemKeys::default();
//! assert_eq!(keys.support(), Support::Unknown);
//! keys.set_support(Support::Available { backend: "x11".into() });
//! keys.set_status("Ctrl+Alt+P", KeyStatus::Registered);
//! assert_eq!(keys.status("Ctrl+Alt+P"), Some(KeyStatus::Registered));
//! ```

use std::collections::BTreeMap;
use std::sync::Mutex;

use egui_pop_core::lock;
use serde::Serialize;

use crate::services::ServiceToken;

/// The system shortcuts' state, as a service of the plugin registering
/// them.
pub const SYSTEM_KEYS: ServiceToken<SystemKeys> = ServiceToken::new("app.system_keys");

/// Whether the desktop takes global shortcuts.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(tag = "support", rename_all = "lowercase")]
pub enum Support {
    /// Not known yet (the backend is starting).
    #[default]
    Unknown,
    /// Through this backend (`x11`, `windows`, `macos`, `portal`).
    Available { backend: String },
    /// Not here, and why (a Wayland compositor without the portal, the
    /// web).
    Unavailable { reason: String },
}

/// One binding's registration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "status", content = "detail", rename_all = "lowercase")]
pub enum KeyStatus {
    /// The desktop delivers it.
    Registered,
    /// Asked, not answered yet (a portal asking the user).
    Pending,
    /// Refused, and why (taken by another application, keys the
    /// desktop cannot grab, declined by the user).
    Failed(String),
}

#[derive(Debug, Default)]
struct State {
    support: Support,
    keys: BTreeMap<String, KeyStatus>,
}

/// The system shortcuts' state: shared, written by the registering
/// plugin from any thread, read by the shortcuts editor.
#[derive(Debug, Default)]
pub struct SystemKeys(Mutex<State>);

impl SystemKeys {
    pub fn support(&self) -> Support {
        lock(&self.0).support.clone()
    }

    pub fn set_support(&self, support: Support) {
        lock(&self.0).support = support;
    }

    /// A binding's registration, by its keys as shown (`Ctrl+Alt+P`).
    pub fn status(&self, keys: &str) -> Option<KeyStatus> {
        lock(&self.0).keys.get(keys).cloned()
    }

    pub fn set_status(&self, keys: &str, status: KeyStatus) {
        lock(&self.0).keys.insert(keys.into(), status);
    }

    /// Forgets every binding's registration (before registering again).
    pub fn clear(&self) {
        lock(&self.0).keys.clear();
    }

    /// Every binding's registration, by keys.
    pub fn statuses(&self) -> BTreeMap<String, KeyStatus> {
        lock(&self.0).keys.clone()
    }
}
