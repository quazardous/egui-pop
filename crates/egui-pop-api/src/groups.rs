//! Plugin groups: what an application starts from.

use crate::Plugin;
use crate::palette::PalettePlugin;

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

/// What most applications want: the command palette (and, as they come,
/// the other components of the paved road).
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultPlugins;

impl PluginGroup for DefaultPlugins {
    fn plugins(self) -> Vec<Box<dyn Plugin>> {
        vec![Box::new(PalettePlugin)]
    }
}
