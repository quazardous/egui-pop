//! The contract of the default kit's `viewport-focus` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "viewport-focus.shape",
        "string",
        "the handler drawing a viewport's focus outline",
    ),
    comp(
        "viewport-focus.rest.fill",
        "color",
        "over a viewport (transparent)",
    ),
];
