//! Thumbnails and a virtualised grid of them, from images the
//! application supplies. The drawing lives in the egui layer
//! ([`egui_pop_core::thumbnail`]), where the file picker of the
//! application API uses it too (ADR 0001).

pub use egui_pop_core::thumbnail::{
    Thumbnail, ThumbnailGrid, caption_height, fitted, thumbnail_grid,
};
