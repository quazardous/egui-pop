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

#[doc = include_str!("../../../docs/plugins.md")]
pub mod plugins {}

#[doc = include_str!("../../../docs/viewport.md")]
pub mod viewport {}

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
