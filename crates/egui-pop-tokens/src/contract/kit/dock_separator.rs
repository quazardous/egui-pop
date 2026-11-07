//! The contract of the default kit's `dock-separator` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "dock-separator.shape",
        "string",
        "the handler drawing the line between two panels of a dock",
    ),
    comp(
        "dock-separator.width",
        "dimension",
        "the thickness of the line between two panels of a dock",
    ),
    comp(
        "dock-separator.rest.fill",
        "color",
        "the line between two panels of a dock, at rest",
    ),
    comp(
        "dock-separator.hover.fill",
        "color",
        "the line between two panels of a dock, under the pointer",
    ),
    comp(
        "dock-separator.active.fill",
        "color",
        "the line between two panels of a dock, while dragged",
    ),
];
