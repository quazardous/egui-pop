//! A photo library's widgets, for importing, culling and sorting
//! images: the application owns its images, their marks and tags; the
//! widgets show them and say what the user asked for.
//!
//! - [`grid`]: a virtualised thumbnail grid and the filmstrip (one row,
//!   sharing the grid's selection), thumbnails supplied asynchronously,
//!   overlays, stacks, the culling keys, a rubber band;
//! - [`marks`]: ratings, colour labels and pick or reject flags, their
//!   widgets, and the culling keymap (Lightroom's keys, auto-advance);
//! - [`compare`]: one to many images, a loupe or side by side, zoom and
//!   pan synced, a "best" marker, images dragged in and out;
//! - `mode` (with the `api` feature): the library grid as a file
//!   browser's view mode, [`LibraryGridMode`];
//! - `thumbs` (with the `api` feature): the file browser's thumbnail
//!   provider as a [`ThumbSource`], and a [`Badge`] as its chip;
//! - [`tags`]: tag chips and a tag input with completion;
//! - [`filter`]: a search field with filter chips, giving a structured
//!   filter;
//! - [`plugin`]: the steps (select, rate, label, flag, tag, stack,
//!   filter), the commands agents run, and [`LibraryPlugin`].
//!
//! Guide: `docs/library.md`.

pub mod compare;
pub mod filter;
pub mod grid;
pub mod marks;
#[cfg(feature = "api")]
pub mod mode;
mod paint;
pub mod plugin;
pub mod tags;
#[cfg(feature = "api")]
pub mod thumbs;

#[cfg(feature = "api")]
pub use grid::change;
pub use grid::{
    Badge, Change, GridResize, LibraryItems, LibraryOutput, LibraryState, Signal, Stack, Thumb,
    ThumbGrid, ThumbSource, Tone, Zoom,
};
pub use marks::{ColorLabel, CullAction, CullKeymap, Flag, Labels, Marks};
#[cfg(feature = "api")]
pub use mode::{LibraryGridMode, LibrarySignals};
pub use paint::{LabelStyle, LibraryTokens};
#[cfg(feature = "api")]
pub use plugin::LibraryPlugin;

use egui::{Rect, vec2};

/// The largest rectangle of aspect `w:h` inside `outer`, centred.
pub fn fitted(outer: Rect, size: [u32; 2]) -> Rect {
    if size[0] == 0 || size[1] == 0 {
        return outer;
    }
    let a = size[0] as f32 / size[1] as f32;
    let (w, h) = if outer.width() / outer.height() > a {
        (outer.height() * a, outer.height())
    } else {
        (outer.width(), outer.width() / a)
    };
    Rect::from_center_size(outer.center(), vec2(w, h))
}
