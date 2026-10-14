//! The logic of egui-pop applications, without egui (layer L1, see
//! ADR 0001): tested fast and headless, and untouched by egui bumps.
//!
//! - [`ids`]: typed ids, one per namespace, serialized as strings;
//! - [`diagnostics`]: what registration ignored or changed, said out loud;
//! - [`registry`]: items tagged with their owner, removable by owner;
//! - [`when`]: context clauses (`listFocus && !textInputFocus`);
//! - [`clock`]: where "now" comes from, injected so tests fix it;
//! - [`commands`]: every action named, run the same way from anywhere;
//! - [`context`]: the context stack (places and modes, ADR 0012): its
//!   frames, its state, a pure `apply` and `restore`, its documents;
//! - [`field`]: a field's state (modified, required, invalid, disabled
//!   with a reason, read-only, pending);
//! - [`fuzzy`]: fuzzy matching and ranking for search fields;
//! - [`keymap`]: keys bound to commands in layers, resolved by focus
//!   depth, with removal, unbinding and a resolution trace;
//! - [`menus`]: menus declared by id, their items commands;
//! - [`panels`] and [`layout`]: declared panels, a saved arrangement and
//!   its repair;
//! - [`values`]: the value store (descriptors, writes with origins,
//!   gestures, undo, listeners, settings);
//! - [`doc`]: versioned documents and their migrations;
//! - [`jobs`]: the host's background jobs, folded from its `job.*`
//!   events, each with an estimated overall fraction;
//! - [`order`]: plugin ordering.

pub mod clock;
pub mod commands;
pub mod context;
pub mod diagnostics;
pub mod doc;
pub mod field;
pub mod fuzzy;
pub mod ids;
pub mod jobs;
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
    doc::Schema::new("keymap", 2)
        .migration(0, doc::same)
        .migration(1, keymap::drop_placeholder_keys)
}

pub use serde_json;
