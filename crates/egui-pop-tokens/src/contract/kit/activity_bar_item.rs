//! The contract of the default kit's `activity-bar-item` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "activity-bar-item.shape",
        "string",
        "the handler drawing an activity bar item",
    ),
    comp(
        "activity-bar-item.rest.fill",
        "color",
        "an activity bar item at rest",
    ),
    comp(
        "activity-bar-item.rest.border-color",
        "color",
        "an activity bar item's outline",
    ),
    comp(
        "activity-bar-item.rest.border-width",
        "dimension",
        "an activity bar item's outline width",
    ),
    comp(
        "activity-bar-item.rest.radius",
        "dimension",
        "an activity bar item's corners",
    ),
    comp(
        "activity-bar-item.rest.text",
        "color",
        "an activity bar item's icon at rest",
    ),
    comp(
        "activity-bar-item.selected.fill",
        "color",
        "the item of the panel shown",
    ),
    comp(
        "activity-bar-item.hover.fill",
        "color",
        "an activity bar item under the pointer",
    ),
];
