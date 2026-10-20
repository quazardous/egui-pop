//! The egui layer of egui-pop (layer L2, see ADR 0001): what an
//! application framework needs from egui, below the application API.
//!
//! - [`theme`]: design tokens into egui's style, typed tokens in the
//!   context;
//! - [`focus`]: focus scopes and the focus chain, rebuilt each pass;
//! - [`frame`]: a window's own title bar and resize edges, publishing
//!   their hit regions in physical pixels;
//! - [`focus_mode`]: whether the pointer gives the keyboard (click or
//!   hover, as the window manager does or as the user chose);
//! - [`keys`]: the key dispatcher (chords, consumption, traces);
//! - [`motion`]: the motion clock (frozen in tests, still under reduced
//!   motion) and the delayed busy indicator;
//! - [`overlay`]: the overlay stack (top-layer dismissal, modality,
//!   focus restore);
//! - [`band`]: the header band a movable dialog or a palette is dragged
//!   by;
//! - [`field`]: a field's state (modified, required, invalid, disabled
//!   with a reason, read-only, pending) drawn, announced and reported;
//! - [`input`]: the decorated input (placeholder, slots, clear, attached
//!   and split buttons, a dropdown) and the split button;
//! - [`tab`]: Tab order in focus scopes (reading order, indices,
//!   groups; cycling in a modal) and F6 between panels;
//! - [`param`]: the editor a value descriptor calls for, in the three
//!   binding modes;
//! - [`slider`]: darktable-style sliders (soft and hard range, fine
//!   and coarse, reset, typed expressions, a quad button), logarithmic,
//!   on a gradient track, or a range with two thumbs; a hook on every
//!   change;
//! - [`progress`]: progress bars, rings, meters and spinners (delayed),
//!   on the motion clock, named for assistive technologies;
//! - [`placeholder`]: skeletons standing in for loading content, and
//!   empty states;
//! - [`placement`]: floating elements placed with flip and shift;
//! - [`scale`]: the window's scales, crisp strokes, the scale-changed
//!   signal, the interface zoom's steps;
//! - [`a11y`]: accessible names and an announcer;
//! - [`strings`]: the library's own strings, by key, with English
//!   fallbacks;
//! - [`icon`]: icons by role, sized and coloured from tokens;
//! - [`indicator`]: micro-indicators (dots and LEDs, badges, pips,
//!   flags, swatches, icons by state) drawn by the kit, passive or
//!   buttons, inline or on a widget, recorded for agents;
//! - [`persist`]: documents written atomically, with a backup and
//!   migrations;
//! - [`fault`]: fault boundaries showing an error card on panic;
//! - [`binding`]: plain, bound and watch-only values;
//! - [`state`]: typed slots for the state kept in egui's memory,
//!   listed by the debug bridge;
//! - [`debug`]: the debug bridge and its collectors.

pub mod a11y;
pub mod band;
pub mod binding;
pub mod debug;
pub mod fault;
pub mod field;
pub mod focus;
pub mod focus_mode;
pub mod fonts;
pub mod frame;
pub mod icon;
pub mod icon_set;
pub mod indicator;
pub mod input;
pub mod keys;
pub mod kit;
pub mod motion;
pub mod overlay;
pub mod param;
pub mod persist;
pub mod placeholder;
pub mod placement;
pub mod progress;
pub mod scale;
pub mod slider;
pub mod strings;
pub mod tab;
pub mod theme;

pub mod state;

pub use egui;
pub use egui_pop_model as model;
pub use egui_pop_tokens as tokens;
pub use state::lock;
