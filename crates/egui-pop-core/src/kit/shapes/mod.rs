//! The handlers egui-pop ships, one module per family. A module adds its
//! handlers in [`built_in`], one line each.

use std::sync::Arc;

use super::Shapes;

mod cell;
mod outline;
mod plain;
mod progress;
mod tab;

pub(crate) use plain::Plain;

/// Adds the shipped handlers other than `plain` (which every component
/// has).
pub(super) fn built_in(shapes: &mut Shapes) {
    shapes.insert("list-row", "box", Arc::new(cell::BoxCell));
    shapes.insert("table-header-cell", "box", Arc::new(cell::BoxCell));
    shapes.insert("meter", "track", Arc::new(progress::Track));
    shapes.insert("meter-tick", "tick", Arc::new(progress::Tick));
    shapes.insert("progress-bar", "track", Arc::new(progress::Track));
    shapes.insert("progress-fill", "span", Arc::new(progress::Span));
    shapes.insert("progress-line", "track", Arc::new(progress::Track));
    shapes.insert("tab", "box", Arc::new(tab::BoxTab));
    shapes.insert("tab", "underline", Arc::new(tab::UnderlineTab));
    shapes.insert("tree-drop-marker", "outline", Arc::new(outline::Outline));
}
