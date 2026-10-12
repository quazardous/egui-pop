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
//! - [`overlay`]: the overlay stack (top-layer dismissal, modality,
//!   focus restore);
//! - [`param`]: the editor a value descriptor calls for, in the three
//!   binding modes;
//! - [`slider`]: a darktable-style slider (soft and hard range, fine
//!   and coarse, reset, typed entry);
//! - [`placement`]: floating elements placed with flip and shift;
//! - [`a11y`]: accessible names and an announcer;
//! - [`strings`]: the library's own strings, by key, with English
//!   fallbacks;
//! - [`icon`]: icons by role, sized and coloured from tokens;
//! - [`persist`]: documents written atomically, with a backup and
//!   migrations;
//! - [`fault`]: fault boundaries showing an error card on panic;
//! - [`binding`]: plain, bound and watch-only values;
//! - [`debug`]: the debug bridge and its collectors.

pub mod a11y;
pub mod binding;
pub mod debug;
pub mod fault;
pub mod focus;
pub mod focus_mode;
pub mod frame;
pub mod icon;
pub mod keys;
pub mod overlay;
pub mod param;
pub mod persist;
pub mod placement;
pub mod slider;
pub mod strings;
pub mod theme;

mod state;

pub use egui;
pub use egui_pop_model as model;
pub use egui_pop_tokens as tokens;
pub use state::lock;
