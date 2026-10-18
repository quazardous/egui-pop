//! The handlers egui-pop ships, one module per family. A module adds its
//! handlers in [`built_in`], one line each.

use std::sync::Arc;

use super::Shapes;

mod band;
mod card;
mod cell;
mod outline;
mod pill;
mod plain;
mod progress;
mod tab;
mod traffic_light;

pub(crate) use card::CARD;
pub(crate) use plain::Plain;

/// Adds the shipped handlers other than `plain`; `card` is every
/// component's too.
pub(super) fn built_in(shapes: &mut Shapes) {
    shapes.insert("activity-bar-item", "box", Arc::new(cell::BoxCell));
    shapes.insert("area-grip", "box", Arc::new(cell::BoxCell));
    shapes.insert("band-icon-button", "box", Arc::new(cell::BoxCell));
    shapes.insert("button", "box", Arc::new(cell::BoxCell));
    shapes.insert("command-button", "box", Arc::new(cell::BoxCell));
    shapes.insert("command-palette-row", "box", Arc::new(cell::BoxCell));
    shapes.insert("dock-preview", "box", Arc::new(cell::BoxCell));
    shapes.insert("compare-pane", "box", Arc::new(cell::BoxCell));
    shapes.insert("field", "box", Arc::new(cell::BoxCell));
    shapes.insert("find-bar-button", "box", Arc::new(cell::BoxCell));
    shapes.insert("header-band", "band", Arc::new(band::Band));
    shapes.insert("icon-button", "box", Arc::new(cell::BoxCell));
    shapes.insert("list-row", "box", Arc::new(cell::BoxCell));
    shapes.insert("mark-toggle", "box", Arc::new(cell::BoxCell));
    shapes.insert("menu-item", "box", Arc::new(cell::BoxCell));
    shapes.insert("skeleton", "box", Arc::new(cell::BoxCell));
    shapes.insert("skeleton-shimmer", "box", Arc::new(cell::BoxCell));
    shapes.insert("slider", "box", Arc::new(cell::BoxCell));
    shapes.insert("slider-toggle", "box", Arc::new(cell::BoxCell));
    shapes.insert("pill", "box", Arc::new(cell::BoxCell));
    shapes.insert("pill", "pill", Arc::new(pill::Pill));
    shapes.insert("table-header-cell", "box", Arc::new(cell::BoxCell));
    shapes.insert("tag-chip", "box", Arc::new(cell::BoxCell));
    shapes.insert("tag-chip", "pill", Arc::new(pill::Pill));
    shapes.insert("thumb-cell", "box", Arc::new(cell::BoxCell));
    shapes.insert("thumb-layer", "box", Arc::new(cell::BoxCell));
    shapes.insert("thumb-overlay-badge", "box", Arc::new(cell::BoxCell));
    shapes.insert("thumbnail", "box", Arc::new(cell::BoxCell));
    shapes.insert("meter", "track", Arc::new(progress::Track));
    shapes.insert("meter-tick", "tick", Arc::new(progress::Tick));
    shapes.insert("progress-bar", "track", Arc::new(progress::Track));
    shapes.insert("progress-fill", "span", Arc::new(progress::Span));
    shapes.insert("progress-line", "track", Arc::new(progress::Track));
    shapes.insert("slider-fill", "track", Arc::new(progress::Track));
    shapes.insert("slider-thumb", "tick", Arc::new(progress::Tick));
    for (component, _) in super::COMPONENTS {
        shapes.insert(component, "card", CARD.clone());
    }
    shapes.insert("tab", "box", Arc::new(tab::BoxTab));
    shapes.insert("tab", "underline", Arc::new(tab::UnderlineTab));
    shapes.insert("tree-drop-marker", "outline", Arc::new(outline::Outline));
    shapes.insert(
        "window-button",
        "traffic-light",
        Arc::new(traffic_light::TrafficLight),
    );
    shapes.insert(
        "window-close",
        "traffic-light",
        Arc::new(traffic_light::TrafficLight),
    );
}
