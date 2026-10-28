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
//! - [`steps`]: the moments of a feature's life plugins may watch,
//!   veto, replace or extend (ADR 0011);
//! - [`contexts`]: the context stack (places and modes, ADR 0012): its
//!   owner behind the host, the projection, the breadcrumb and mode
//!   chips, the `nav.*` and `context.*` commands, a plugin;
//! - [`clipboard`]: the clipboard (ADR 0015): its owner behind the
//!   host, components' sources and targets, types, transforms and
//!   sanitisers declared by plugins, the `clipboard.copy` and
//!   `clipboard.paste` steps, the Edit commands and the agents'
//!   `clipboard.*`, a plugin;
//! - [`services`]: typed service tokens;
//! - [`stories`]: gallery stories as a contribution;
//! - [`busy`]: a button bound to a command, showing its busy state (a
//!   delayed spinner while the host has not answered);
//! - [`palette`]: the command palette, a plugin;
//! - [`palettes`]: toolboxes and palettes floating over the interface,
//!   never modal, docking in the workbench's areas;
//! - [`devtools`]: the debug bridge's collectors in the application, a
//!   panel behind a setting, a plugin;
//! - [`dialogs`]: confirmations, forms, choices and notices, a plugin;
//! - [`toasts`]: notices in a corner, with actions, a plugin;
//! - [`jobs`]: the host's background jobs followed from its journal
//!   (each with an estimated overall fraction), their commands and the
//!   `jobs` collector, a plugin; [`jobs_centre`]: the jobs as a panel
//!   and a popover, each with its log console;
//! - [`frame`]: the window's own title bar and edges, a plugin;
//! - [`strips`]: the activity bar and status bar indicators fed by the
//!   host's events, a plugin;
//! - [`arrangements`]: named layouts saved, switched and reset through
//!   the host's store, a plugin;
//! - [`workbench`]: panels in areas over egui_dock, with placement
//!   rules, changed by named steps;
//! - [`window`]: the window's place kept and repaired, full screen with
//!   a way out, a plugin;
//! - [`zoom`]: the interface zoom in steps, its commands and keys;
//! - [`sliders`]: the `slider.change` step, and the hook that runs it
//!   from egui-pop's sliders;
//! - [`settings_window`]: the settings window (search, sections, form)
//!   and the shortcuts editor, a plugin;
//! - [`settings_pages`]: custom pages a plugin draws in the settings
//!   window, placed in its tree, searched, with their reset;
//! - [`system_keys`]: how the shortcuts bound in the system scope (also
//!   outside the window) are registered, as the shortcuts editor shows
//!   it;
//! - [`theme_editor`]: a theme's tokens edited live, by role, with
//!   colour pickers, contrast readouts, undo, export and import, a
//!   plugin;
//! - [`tips`]: a tip of the day and contextual hints, dismissed for good
//!   through the settings, a plugin;
//! - [`strings`]: the interface's own text, by key with English
//!   fallbacks, translated by the installed `egui_pop_core::strings`
//!   provider;
//! - [`mounts`]: the mount contract (ADR 0013): the hosts and their
//!   offers, the components and their needs, `AppHandle::where_can`,
//!   `AppHandle::mount` and the `mounts` collector;
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
pub mod arrangements;
pub mod busy;
pub mod clipboard;
pub mod contexts;
pub mod ctx;
pub mod devtools;
pub mod dialogs;
pub mod files;
pub mod frame;
mod groups;
pub mod history;
pub mod history_view;
pub mod hooks;
pub mod host;
pub mod jobs;
pub mod jobs_centre;
pub mod layout_controls;
pub mod menus;
pub mod mounts;
pub mod palette;
pub mod palettes;
pub mod plugin;
pub mod pointer;
pub mod services;
pub mod settings;
pub mod settings_pages;
pub mod settings_window;
pub mod sliders;
pub mod steps;
pub mod stories;
pub mod strings;
pub mod strips;
pub mod system_keys;
pub mod text;
pub mod theme_editor;
pub mod tips;
pub mod toasts;
pub mod window;
pub mod workbench;
mod zen;
pub mod zones;
pub mod zoom;

pub use app::{
    App, AppBuilder, AppHandle, CMD_THEME_TOGGLE, Call, FOCUS_MODE, MakeError, Request,
    SETTING_ACCENT_COLOR, SETTING_FOCUS, SETTING_THEME_PREFERENCE, THEME, ThemeService,
};
pub use ctx::{Ctx, CtxError, ValueWriter};
pub use groups::{DefaultPlugins, MinimalPlugins, PluginGroup};
pub use plugin::{BuildError, Plugin, PluginError, PluginInfo};

/// eframe, with the `eframe` feature: the version `run_native` uses.
#[cfg(feature = "eframe")]
pub use eframe;
pub use egui_pop_core as core;
pub use egui_pop_model as model;
