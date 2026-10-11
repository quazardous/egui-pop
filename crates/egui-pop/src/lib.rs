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
/// The token parser and resolver (no egui dependency).
pub use egui_pop_tokens as tokens;

pub mod theme;
