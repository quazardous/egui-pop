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
//! - [`hooks`]: the application's way into each step (veto, redirect,
//!   replace, extend), traced;
//! - [`debug`]: the `viewport` collector of the debug bridge;
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

pub mod debug;
pub mod demo;
pub mod display;
pub mod gestures;
#[cfg(feature = "wgpu")]
pub mod gpu;
pub mod hooks;
pub mod source;
pub mod tiles;
pub mod tokens;
pub mod view;
pub mod widget;

pub use hooks::{Cause, Hooks, Verdict, ViewChange};
pub use view::{Mapping, View, Zoom};
pub use widget::{Phase, Signal, Viewport, ViewportOutput, ViewportState};
