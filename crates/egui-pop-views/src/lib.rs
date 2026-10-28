//! Data views for egui-pop: a live tree, a list and a data table.
//!
//! - [`tree`]: a tree over a source the application implements, with
//!   lazy, asynchronous branches and edits at any time;
//! - [`list`]: a list of rows the application draws;
//! - [`table`]: a data table (on `egui_table`), sorted and resized
//!   through the host's value store, cells drawn by the application;
//! - [`select`]: the selection by key they share;
//! - [`library`]: a photo library's widgets: a thumbnail grid and a
//!   filmstrip, ratings, colour labels and flags, a comparison, tag
//!   chips and input, a search field with filter chips, their steps and
//!   commands;
//! - [`mounts`]: what they need (the mount contract, ADR 0013);
//! - [`steps`]: their moments as steps, and [`ViewsPlugin`] that
//!   declares them;
//! - [`strings`]: the text they show, translatable;
//! - `viewer` (feature `markdown`): the in-app viewer of ADR 0017, a
//!   panel showing `docs:` pages, markdown and images with back and
//!   forward, an address bar, find in page and zoom, the links service's
//!   way inside.
//!
//! All three are virtualised (only the rows in view are built), take
//! the keyboard focus as a whole (arrows, Page Up/Down, Home/End,
//! type-ahead), and report what happened as signals in their output and
//! as steps a plugin may veto, replace or extend (ADR 0011). The
//! application owns its data: the views show it and say what the user
//! asked for.
//!
//! Layer: a component on the application API (L4, ADR 0001).

pub mod library;
pub mod list;
pub mod mounts;
mod nav;
mod rows;
pub mod select;
pub mod steps;
pub mod strings;
pub mod table;
pub mod tree;
#[cfg(feature = "markdown")]
pub mod viewer;

pub use nav::TYPE_AHEAD_DELAY;
pub use select::{Mode, Selection};
pub use steps::ViewsPlugin;
