//! Hooks: synchronous `alter` hooks over declarations (menus, the palette,
//! settings), run when the application is built or a plugin comes or
//! goes, never per frame; and lifecycle events.

use std::fmt;

use egui_pop_model::ids::{CommandId, HookId, MenuId, OwnerId, SettingId};
use egui_pop_model::menus::MenuItem;
use egui_pop_model::values::Descriptor;
use serde::Serialize;

use crate::app::AppHandle;

/// A palette entry, derived from a command.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PaletteEntry {
    pub command: CommandId,
    pub title: String,
    pub category: String,
    /// Its keys as shown (`Ctrl+K Ctrl+S`).
    pub keys: Vec<String>,
}

/// A setting as the settings form shows it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SettingEntry {
    pub id: SettingId,
    /// Its path in the value store (`settings/<id>`).
    pub path: String,
    pub owner: OwnerId,
    /// A section title (the owner's, unless a hook changes it).
    pub section: String,
    pub descriptor: Descriptor,
}

/// A function altering a list of declarations.
pub type AlterFn<T> = Box<dyn Fn(&mut Vec<T>)>;

/// What a hook alters.
pub enum Alter {
    Menu(MenuId, AlterFn<MenuItem>),
    Palette(AlterFn<PaletteEntry>),
    Settings(AlterFn<SettingEntry>),
}

impl fmt::Debug for Alter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Alter::Menu(m, _) => write!(f, "Menu({m})"),
            Alter::Palette(_) => write!(f, "Palette"),
            Alter::Settings(_) => write!(f, "Settings"),
        }
    }
}

/// An alter hook: an id, what it alters, a priority (higher first) and
/// the hooks it must run after.
#[derive(Debug)]
pub struct Hook {
    pub id: HookId,
    pub alter: Alter,
    pub priority: i32,
    pub after: Vec<HookId>,
}

impl Hook {
    /// Alters a menu's items (order, removal, additions).
    pub fn menu(id: HookId, menu: MenuId, f: impl Fn(&mut Vec<MenuItem>) + 'static) -> Hook {
        Hook {
            id,
            alter: Alter::Menu(menu, Box::new(f)),
            priority: 0,
            after: Vec::new(),
        }
    }

    /// Alters the palette's entries.
    pub fn palette(id: HookId, f: impl Fn(&mut Vec<PaletteEntry>) + 'static) -> Hook {
        Hook {
            id,
            alter: Alter::Palette(Box::new(f)),
            priority: 0,
            after: Vec::new(),
        }
    }

    /// Alters the settings form's entries.
    pub fn settings(id: HookId, f: impl Fn(&mut Vec<SettingEntry>) + 'static) -> Hook {
        Hook {
            id,
            alter: Alter::Settings(Box::new(f)),
            priority: 0,
            after: Vec::new(),
        }
    }

    pub fn priority(mut self, priority: i32) -> Hook {
        self.priority = priority;
        self
    }

    /// Runs after another hook (a missing one is reported, and ignored).
    pub fn after(mut self, hook: HookId) -> Hook {
        self.after.push(hook);
        self
    }
}

/// Moments of an application's life.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "event", content = "plugin", rename_all = "snake_case")]
pub enum Lifecycle {
    /// Every plugin is connected; the first frame comes next.
    Started,
    /// A plugin was connected after start-up.
    Connected(egui_pop_model::ids::PluginId),
    /// A plugin was disconnected.
    Disconnected(egui_pop_model::ids::PluginId),
    /// The declarations were rebuilt (menus, palette, settings, zones).
    Rebuilt,
}

impl Lifecycle {
    /// The kind, ignoring the plugin: what handlers subscribe to.
    pub fn kind(&self) -> &'static str {
        match self {
            Lifecycle::Started => "started",
            Lifecycle::Connected(_) => "connected",
            Lifecycle::Disconnected(_) => "disconnected",
            Lifecycle::Rebuilt => "rebuilt",
        }
    }
}

/// Which lifecycle events a handler wants.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum On {
    Started,
    Connected,
    Disconnected,
    Rebuilt,
}

impl On {
    pub(crate) fn matches(self, e: &Lifecycle) -> bool {
        matches!(
            (self, e),
            (On::Started, Lifecycle::Started)
                | (On::Connected, Lifecycle::Connected(_))
                | (On::Disconnected, Lifecycle::Disconnected(_))
                | (On::Rebuilt, Lifecycle::Rebuilt)
        )
    }
}

/// A lifecycle handler.
pub type LifecycleFn = Box<dyn FnMut(&AppHandle, &Lifecycle)>;
