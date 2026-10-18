//! The handlers egui-pop ships, one module per family. A module adds its
//! handlers in [`built_in`], one line each.

use std::sync::Arc;

use super::Shapes;

mod card;
mod cell;
mod outline;
mod plain;
mod progress;
mod tab;
mod traffic_light;

pub(crate) use card::CARD;
pub(crate) use plain::Plain;

/// Adds the shipped handlers other than `plain`; `card` is every
/// component's too.
pub(super) fn built_in(shapes: &mut Shapes) {
    shapes.insert("button", "box", Arc::new(cell::BoxCell));
    shapes.insert("field", "box", Arc::new(cell::BoxCell));
    shapes.insert("list-row", "box", Arc::new(cell::BoxCell));
    shapes.insert("menu-item", "box", Arc::new(cell::BoxCell));
    shapes.insert("table-header-cell", "box", Arc::new(cell::BoxCell));
    shapes.insert("meter", "track", Arc::new(progress::Track));
    shapes.insert("meter-tick", "tick", Arc::new(progress::Tick));
    shapes.insert("progress-bar", "track", Arc::new(progress::Track));
    shapes.insert("progress-fill", "span", Arc::new(progress::Span));
    shapes.insert("progress-line", "track", Arc::new(progress::Track));
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
