//! The application API of egui-pop (layer L3, see ADR 0001), with its
//! own semver: what plugins are written against.
//!
//! - [`Plugin`]: a feature with an id, requirements, a priority, and
//!   `connect`/`disconnect`; everything it registers is its own
//!   (ADR 0004);
//! - [`Ctx`]: what a plugin registers through: commands (declared once:
//!   keys, menus, palette and agent surface derived), key bindings, menu
//!   items, panels, zones, blocks, settings, services, lifecycle
//!   handlers, alter hooks, stories;
//! - [`zones`]: named places and blocks added, replaced, extended or
//!   wrapped (ADR 0005);
//! - [`hooks`]: alter hooks over menus, the palette and settings, run at
//!   build time; lifecycle events;
//! - [`host`]: the application's kernel behind the interface: commands
//!   run, an event journal, the store, a waker ([`host::LocalHost`] by
//!   default);
//! - [`services`]: typed service tokens;
//! - [`stories`]: gallery stories as a contribution;
//! - [`palette`]: the command palette, a plugin;
//! - [`dialogs`]: confirmations, forms, choices and notices, a plugin;
//! - [`toasts`]: notices in a corner, with actions, a plugin;
//! - [`settings_window`]: the settings window (search, sections, form)
//!   and the shortcuts editor, a plugin;
//! - [`DefaultPlugins`] and [`MinimalPlugins`]: plugin groups for
//!   [`AppBuilder::plugins`];
//! - [`AppBuilder`] and [`App`]: sorting, connecting, the shell (menu
//!   bar, zones, panels, the command palette), key dispatch; over eframe
//!   or headless.
//!
//! ```
//! use egui_pop_api::{App, Ctx, Plugin, PluginError, zones};
//! use egui_pop_api::zones::Block;
//! use egui_pop_model::commands::CommandSpec;
//! use egui_pop_model::ids::{BlockId, CommandId, PluginId};
//!
//! struct Hello;
//!
//! impl Plugin for Hello {
//!     fn id(&self) -> PluginId {
//!         PluginId::from_static("hello")
//!     }
//!
//!     fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
//!         ctx.command(
//!             CommandSpec::new(CommandId::from_static("hello.greet"), "Greet", "Hello").key("Ctrl+G"),
//!             |_| Ok(serde_json::json!("hello")),
//!         )?;
//!         ctx.block(zones::MAIN, Block::add(BlockId::from_static("hello.text"), 0, |ui, _| {
//!             ui.label("Hello");
//!         }));
//!         Ok(())
//!     }
//! }
//!
//! let ctx = egui::Context::default();
//! let mut app = App::builder().plugin(Hello).build(&ctx).unwrap();
//! assert!(app.handle().diagnostics().is_empty());
//! let out = app.run(&CommandId::from_static("hello.greet"), serde_json::Value::Null,
//!     egui_pop_model::commands::Origin::User);
//! assert_eq!(out.now(), Some(Ok(serde_json::json!("hello"))));
//! ```

pub mod app;
pub mod ctx;
pub mod dialogs;
mod groups;
pub mod hooks;
pub mod host;
pub mod menus;
pub mod palette;
pub mod plugin;
pub mod services;
pub mod settings;
pub mod settings_window;
pub mod stories;
pub mod toasts;
pub mod zones;

pub use app::{
    App, AppBuilder, AppHandle, Call, FOCUS_MODE, Request, SETTING_FOCUS, THEME, ThemeService,
};
pub use ctx::{Ctx, CtxError};
pub use groups::{DefaultPlugins, MinimalPlugins, PluginGroup};
pub use plugin::{BuildError, Plugin, PluginError, PluginInfo};

pub use egui_pop_core as core;
pub use egui_pop_model as model;
