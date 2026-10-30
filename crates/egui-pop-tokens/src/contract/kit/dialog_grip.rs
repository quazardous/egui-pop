//! The contract of the default kit's `dialog-grip` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "dialog-grip.shape",
        "string",
        "the handler drawing a resizable dialog's corner handle",
    ),
    comp(
        "dialog-grip.width",
        "dimension",
        "how far a resizable dialog's edges take the pointer, on either side of them",
    ),
    comp(
        "dialog-grip.height",
        "dimension",
        "the size of the corner handle's mark",
    ),
    comp(
        "dialog-grip.rest.text",
        "color",
        "the corner handle's lines at rest",
    ),
    comp(
        "dialog-grip.rest.border-width",
        "dimension",
        "the width of the corner handle's lines",
    ),
    comp(
        "dialog-grip.hover.text",
        "color",
        "the corner handle's lines under the pointer",
    ),
    comp(
        "dialog-grip.active.text",
        "color",
        "the corner handle's lines while dragged",
    ),
];
