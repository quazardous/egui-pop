//! The contract of the default kit's `dock-body` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "dock-body.shape",
        "string",
        "the handler drawing a panel body's border in a dock",
    ),
    comp(
        "dock-body.rest.border-color",
        "color",
        "a panel body's border in a dock, at rest",
    ),
    comp(
        "dock-body.rest.border-width",
        "dimension",
        "a panel body's border width in a dock, at rest (0: none)",
    ),
    comp(
        "dock-body.rest.radius",
        "dimension",
        "a panel body's corners in a dock, where they are free",
    ),
];
