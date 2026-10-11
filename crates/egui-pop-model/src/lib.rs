//! The logic of egui-pop applications, without egui (layer L1, see
//! ADR 0001): tested fast and headless, and untouched by egui bumps.
//!
//! - [`ids`]: typed ids, one per namespace, serialized as strings;
//! - [`diagnostics`]: what registration ignored or changed, said out loud;
//! - [`registry`]: items tagged with their owner, removable by owner;
//! - [`when`]: context clauses (`listFocus && !textInputFocus`);
//! - [`commands`]: every action named, run the same way from anywhere;
//! - [`keymap`]: keys bound to commands in layers, resolved by focus
//!   depth, with removal, unbinding and a resolution trace;
//! - [`menus`]: menus declared by id, their items commands;
//! - [`panels`] and [`layout`]: declared panels, a saved arrangement and
//!   its repair;
//! - [`values`]: the value store (descriptors, writes with origins,
//!   gestures, undo, listeners, settings);
//! - [`doc`]: versioned documents and their migrations;
//! - [`order`]: plugin ordering.

pub mod commands;
pub mod diagnostics;
pub mod doc;
pub mod ids;
pub mod keymap;
pub mod layout;
pub mod menus;
pub mod order;
pub mod panels;
pub mod registry;
pub mod values;
pub mod when;

/// The `keymap` document's schema.
pub fn keymap_schema() -> doc::Schema {
    doc::Schema::new("keymap", 1).migration(0, doc::same)
}

pub use serde_json;
