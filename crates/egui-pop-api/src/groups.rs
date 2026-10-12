//! Plugin groups: what an application starts from.

use crate::Plugin;
use crate::arrangements::ArrangementsPlugin;
use crate::dialogs::DialogsPlugin;
use crate::palette::PalettePlugin;
use crate::settings_window::SettingsPlugin;
use crate::toasts::ToastsPlugin;
use crate::window::WindowPlugin;

/// Plugins added together (`AppBuilder::plugins`).
pub trait PluginGroup {
    fn plugins(self) -> Vec<Box<dyn Plugin>>;
}

/// The framework alone: the shell, its menus, undo and redo, quit, the
/// focus mode setting. No palette, dialogs or toasts.
#[derive(Clone, Copy, Debug, Default)]
pub struct MinimalPlugins;

impl PluginGroup for MinimalPlugins {
    fn plugins(self) -> Vec<Box<dyn Plugin>> {
        Vec::new()
    }
}

/// What most applications want: the command palette, dialogs, toasts and
/// the settings window with its shortcuts editor, the window's place,
/// named arrangements of the workbench (and
/// the next components of the paved road as they come).
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultPlugins;

impl PluginGroup for DefaultPlugins {
    fn plugins(self) -> Vec<Box<dyn Plugin>> {
        vec![
            Box::new(PalettePlugin),
            Box::new(DialogsPlugin),
            Box::new(ToastsPlugin),
            Box::new(SettingsPlugin),
            Box::new(WindowPlugin),
            Box::new(ArrangementsPlugin),
        ]
    }
}
