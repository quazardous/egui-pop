//! A generic image viewport for egui-pop: pan and zoom measured in
//! physical pixels, tiles from an asynchronous source, overlays in image
//! coordinates.
//!
//! - [`view`]: the view model (zoom in physical pixels per image pixel,
//!   the zoom ladder, fit) and [`view::Mapping`], the one mapping between
//!   image and screen that drawing, hit tests and the readout share;
//! - [`gestures`]: the gestures as data, remappable per profile;
//! - [`widget`]: the [`Viewport`] widget and the [`Signal`]s it emits;
//! - [`display`]: the display target of each window and the display
//!   transform hook where the application plugs its colour management;
//! - [`source`]: the [`TileSource`](source::TileSource) an application
//!   implements (asynchronous, live) and the sink it delivers to;
//! - [`tiles`]: the tile cache (proxy then exact, cancellation, a
//!   texture budget, tiles within the device's largest texture);
//! - `gpu` (feature `wgpu`): scene tiles as half-float textures, the
//!   display transform in a shader;
//! - [`demo`]: a procedural source for galleries and examples;
//! - [`overlay`]: overlays in image coordinates and the handle kit;
//! - [`thumbnail`] and [`histogram`]: thumbnails in a virtualised grid
//!   and a histogram, from images and bins the application supplies
//!   (with clipping markers, and a drag handed back to the application);
//! - [`curve`]: a curve editor (points added, moved and deleted, a
//!   spline the application supplies, a histogram behind, the value
//!   picked in the image);
//! - [`wheel`]: colour wheels (lift, gamma, gain) and a hue and
//!   saturation picker, each with a luminance slider;
//! - [`navigator`]: a zoom control and a navigator (a minimap), sharing
//!   the viewport's state;
//! - [`change`]: what the image tools share, the change they propose
//!   and the hook that sees it (a veto, a redirect), and the form their
//!   values take in a step;
//! - [`hooks`]: the application's way into each step (veto, redirect,
//!   replace, extend), traced;
//! - [`debug`]: the `viewport` collector of the debug bridge;
//! - [`strings`]: the text it shows, translatable;
//! - [`tokens`]: the component tokens it draws with.
//!
//! Layer: a component crate on top of `egui-pop-core` (beside
//! `egui-pop-api`, see ADR 0001). Nothing here knows about raw files,
//! colour science or an engine: an application supplies the pixels.
//!
//! ```
//! use egui_pop_viewport::{View, Viewport, ViewportState};
//!
//! fn ui(ui: &mut egui::Ui, state: &mut ViewportState, view: &mut View) {
//!     let out = Viewport::new("main").image_size([6000, 4000]).show(ui, state, view);
//!     for signal in out.signals {
//!         // Record the view, keep a gesture in one undo step...
//!         let _ = signal;
//!     }
//! }
//! ```

pub mod change;
pub mod curve;
pub mod debug;
pub mod demo;
pub mod display;
pub mod gestures;
#[cfg(feature = "wgpu")]
pub mod gpu;
pub mod histogram;
pub mod hooks;
pub mod navigator;
pub mod overlay;
pub mod source;
pub mod strings;
pub mod thumbnail;
pub mod tiles;
pub mod tokens;
pub mod view;
pub mod wheel;
pub mod widget;

pub use hooks::{Cause, Hooks, Verdict, ViewChange};
pub use view::{Mapping, View, Zoom};
pub use widget::{Compare, Phase, Signal, Viewport, ViewportOutput, ViewportState};
