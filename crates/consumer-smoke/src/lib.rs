//! The Rust code blocks of the README and of the guides in `docs/`,
//! compiled and run as doctests against the `egui-pop` crate only, as a
//! user depends on it.
//!
//! Run with `cargo test -p consumer-smoke --doc`.

// The pages' links are relative to the repository, not items.
#![allow(rustdoc::broken_intra_doc_links)]
#![doc = include_str!("../../../README.md")]

#[doc = include_str!("../../../docs/getting-started.md")]
pub mod getting_started {}

#[doc = include_str!("../../../docs/themes.md")]
pub mod themes {}

#[doc = include_str!("../../../docs/theme-editor.md")]
pub mod theme_editor {}

#[doc = include_str!("../../../docs/tips.md")]
pub mod tips {}

#[doc = include_str!("../../../docs/plugins.md")]
pub mod plugins {}

#[doc = include_str!("../../../docs/contexts.md")]
pub mod contexts {}

#[doc = include_str!("../../../docs/viewport.md")]
pub mod viewport {}

#[doc = include_str!("../../../docs/sliders.md")]
pub mod sliders {}

#[doc = include_str!("../../../docs/inputs.md")]
pub mod inputs {}

#[doc = include_str!("../../../docs/image-tools.md")]
pub mod image_tools {}

#[doc = include_str!("../../../docs/cursors.md")]
pub mod cursors {}

#[doc = include_str!("../../../docs/node-graph.md")]
pub mod node_graph {}

#[doc = include_str!("../../../docs/tree.md")]
pub mod tree {}

#[doc = include_str!("../../../docs/table.md")]
pub mod table {}

#[doc = include_str!("../../../docs/editors.md")]
pub mod editors {}

#[doc = include_str!("../../../docs/progress.md")]
pub mod progress {}

#[doc = include_str!("../../../docs/workbench.md")]
pub mod workbench {}

#[doc = include_str!("../../../docs/debugging.md")]
pub mod debugging {}

#[doc = include_str!("../../../docs/i18n.md")]
pub mod i18n {}

#[doc = include_str!("../../../docs/instance.md")]
pub mod instance {}

#[doc = include_str!("../../../docs/notifications.md")]
pub mod notifications {}

#[doc = include_str!("../../../docs/tray.md")]
pub mod tray {}

#[doc = include_str!("../../../docs/global-shortcuts.md")]
pub mod global_shortcuts {}

#[doc = include_str!("../../../docs/logs.md")]
pub mod logs {}

#[doc = include_str!("../../../docs/focus-mode.md")]
pub mod focus_mode {}

#[doc = include_str!("../../../docs/packaging.md")]
pub mod packaging {}

#[doc = include_str!("../../../docs/files.md")]
pub mod files {}
