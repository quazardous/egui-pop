//! Data views for egui-pop: a live tree, a list, a data table and charts.
//!
//! <div class="warning">Experimental: may change in any release
//! (docs/stability.md).</div>
//!
//! - [`tree`]: a tree over a source the application implements, with
//!   lazy, asynchronous branches and edits at any time;
//! - [`list`]: a list of rows the application draws;
//! - [`table`]: a data table (on `egui_table`), sorted and resized
//!   through the host's value store, cells drawn by the application;
//! - [`text_log`]: a text log on the data table: time, entity, level and
//!   body columns, filters by level and text, the tail followed, the rows
//!   at a time cursor highlighted and a click moving the cursor;
//! - [`item`]: the dense row they draw with, also a widget of its own
//!   ([`item::ListItem`]: an icon, a label, weak trailing text, action
//!   buttons shown on hover, indent guides, an arrow, a drag handle);
//! - [`property`]: property rows, a name and a value, their names
//!   aligned;
//! - [`plot`]: charts (on `egui_plot`): time series, scatters and bars
//!   in the kit's colours and the categorical palette, with an
//!   accessible summary, a time cursor and the `plots` collector;
//! - [`select`]: the selection by key they share; the tree and the list
//!   also bind to the application-wide selection
//!   (`egui_pop_api::selection`) with `shared`;
//! - [`library`]: a photo library's widgets: a thumbnail grid and a
//!   filmstrip, ratings, colour labels and flags, a comparison, tag
//!   chips and input, a search field with filter chips, their steps and
//!   commands;
//! - [`mounts`]: what they need (the mount contract, ADR 0013);
//! - [`steps`]: their moments as steps, and [`ViewsPlugin`] that
//!   declares them;
//! - [`strings`]: the text they show, translatable;
//! - [`timeline`]: a timeline over seconds, frames (timecode) or bars and
//!   beats: an adaptive ruler, a playhead to scrub, a range that loops,
//!   markers and cue points, snapping, the J/K/L transport keys, each
//!   change of its document a step of the history, and the commands and
//!   collector agents drive it with;
//! - [`video`]: the video widget, a local file played in place with full,
//!   compact or no controls (GStreamer with the `video` feature, a
//!   placeholder with Open outside without it), and the commands and
//!   collector agents drive it with;
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
//! ## Without the application
//!
//! The `api` feature (on by default) brings the application layer: each
//! view's `.app(&AppHandle)`, `.shared(..)`, and the plugins. Turned off
//! (`default-features = false`), a view is a plain egui widget over
//! egui-pop's core and model, for any egui or eframe application; what
//! the user did comes back in its output.
//!
//! ```
//! use egui_pop_views::list::{ListRows, ListState, ListView};
//!
//! struct Files(Vec<String>);
//!
//! impl ListRows<usize> for Files {
//!     fn len(&self) -> usize {
//!         self.0.len()
//!     }
//!     fn key(&self, row: usize) -> usize {
//!         row
//!     }
//!     fn label(&self, row: usize) -> String {
//!         self.0[row].clone()
//!     }
//! }
//!
//! let ctx = egui::Context::default();
//! let mut state = ListState::<usize>::new();
//! let mut files = Files(vec!["a.txt".into(), "b.txt".into()]);
//! let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
//!     let out = ListView::new("files").label("Files").show(ui, &mut state, &mut files);
//!     for _signal in out.signals { /* the user selected or opened a row */ }
//! });
//! out.textures_delta.clear();
//! ```
//!
//! Layer: a component on the application API (L4, ADR 0001); without
//! `api`, a component on core and model only.

mod app_link;
pub mod item;
pub mod library;
pub mod list;
pub mod mounts;
mod nav;
pub mod plot;
pub mod property;
mod rows;
pub mod select;
mod shared;
pub mod steps;
pub mod strings;
pub mod table;
pub mod text_log;
pub mod timeline;
pub mod tree;
pub mod video;
#[cfg(feature = "markdown")]
pub mod viewer;

pub use nav::TYPE_AHEAD_DELAY;
pub use select::{Mode, Selection};
#[cfg(feature = "api")]
pub use steps::ViewsPlugin;
