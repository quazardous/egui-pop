//! The handlers egui-pop ships, one module per family. A module adds its
//! handlers in [`built_in`], one line each.

use std::sync::Arc;

use super::Shapes;

mod card;
mod plain;
mod tab;

pub(crate) use card::CARD;
pub(crate) use plain::Plain;

/// Adds the shipped handlers other than `plain`; `card` is every
/// component's too.
pub(super) fn built_in(shapes: &mut Shapes) {
    for (component, _) in super::COMPONENTS {
        shapes.insert(component, "card", CARD.clone());
    }
    shapes.insert("tab", "box", Arc::new(tab::BoxTab));
    shapes.insert("tab", "underline", Arc::new(tab::UnderlineTab));
}
