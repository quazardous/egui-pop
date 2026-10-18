//! The contract of the default kit's `dock-preview` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "dock-preview.shape",
        "string",
        "the handler drawing where a drag would dock",
    ),
    comp(
        "dock-preview.opacity",
        "number",
        "how opaque a dock preview's fill is",
    ),
    comp("dock-preview.rest.fill", "color", "where a drag may dock"),
    comp(
        "dock-preview.rest.border-color",
        "color",
        "a dock preview's outline",
    ),
    comp(
        "dock-preview.rest.border-width",
        "dimension",
        "a dock preview's outline width",
    ),
    comp(
        "dock-preview.rest.radius",
        "dimension",
        "a dock preview's corners",
    ),
    comp("dock-preview.rest.text", "color", "why a dock is refused"),
    comp(
        "dock-preview.disabled.fill",
        "color",
        "where a drag may not dock",
    ),
];
