//! egui-pop: themable, accessible, agent-drivable components for egui.
//!
//! Early days: this release holds the design tokens and the theme. Themes
//! are DTCG token files (see [`tokens`]) in three tiers (palette, roles,
//! components), composed from a base theme (dark, light, high contrast), a
//! density, an accent and overrides, and turned into egui's own style so
//! egui's widgets and other crates' widgets match. Custom widgets read the
//! typed [`theme::Tokens`] from the context.
//!
//! ```
//! use egui_pop::theme::{self, Density, ThemeSpec};
//!
//! let ctx = egui::Context::default();
//! // At start-up, or whenever the user picks another theme:
//! let tokens = theme::install(
//!     &ctx,
//!     &ThemeSpec::new("pop-dark").density(Density::Compact).accent("blue"),
//! );
//! assert!(tokens.resolved().problems().is_empty());
//!
//! // Then draw as usual: egui's widgets use the theme, custom painting
//! // reads the typed tokens.
//! fn draw(ui: &mut egui::Ui) {
//!     let tokens = theme::Tokens::of(ui.ctx());
//!     ui.colored_label(tokens.status.success, "Saved");
//!     let _ = ui.button("A themed button");
//! }
//! ```
//!
//! `egui` is re-exported: use `egui_pop::egui` to be sure to get the
//! version egui-pop was built for (one egui-pop minor per egui minor).

pub use egui;
/// eframe (feature `eframe`): the version applications run with.
#[cfg(feature = "eframe")]
pub use egui_pop_api::eframe;
/// The token parser and resolver (no egui dependency).
pub use egui_pop_tokens as tokens;

/// The logic without egui: ids, commands, keymap, menus, panels, layouts,
/// the value store, documents.
pub use egui_pop_model as model;

/// The egui layer: focus, keys, overlays, placement, accessibility,
/// strings, icons, persistence, fault boundaries, binding.
pub use egui_pop_core as core;

/// The application API: plugins, the app builder, zones and blocks,
/// hooks, services, stories.
pub use egui_pop_api as api;

/// The image viewport, thumbnails and the histogram (feature
/// `viewport`).
#[cfg(feature = "viewport")]
pub use egui_pop_viewport as viewport;

/// Translations: catalogues, the language setting, locale formatting,
/// the key extractor (feature `i18n`).
#[cfg(feature = "i18n")]
pub use egui_pop_i18n as i18n;

/// Platform services as opt-in plugins: a single instance and deep
/// links, notifications, a tray icon (feature `platform`, or
/// `platform-<service>` for one).
#[cfg(any(
    feature = "platform-instance",
    feature = "platform-notify",
    feature = "platform-tray"
))]
pub use egui_pop_platform as platform;

/// Themes: tokens into egui's style (from `egui-pop-core`).
pub use egui_pop_core::theme;

/// What most applications use.
pub mod prelude {
    pub use egui_pop_api::hooks::{Hook, On};
    pub use egui_pop_api::services::ServiceToken;
    pub use egui_pop_api::steps::{Before, Outcome, StepSpec};
    pub use egui_pop_api::stories::Story;
    pub use egui_pop_api::zones::{self, Block, Place};
    pub use egui_pop_api::{
        App, AppBuilder, AppHandle, Call, Ctx, DefaultPlugins, MinimalPlugins, Plugin, PluginError,
    };
    pub use egui_pop_core::binding::{Binding, SharedStore};
    pub use egui_pop_core::theme::{Density, ThemeSpec, Tokens};
    pub use egui_pop_model::commands::{CommandSpec, Origin};
    pub use egui_pop_model::ids::{
        BlockId, CommandId, HookId, MenuId, PanelId, PluginId, ServiceId, SettingId, StepId,
        StoryId, ZoneId,
    };
    pub use egui_pop_model::values::Descriptor;
}
