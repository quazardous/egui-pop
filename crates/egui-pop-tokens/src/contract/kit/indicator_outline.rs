//! The contract of the default kit's `indicator-outline` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "indicator-outline.shape",
        "string",
        "the handler drawing a mark's outline",
    ),
    comp(
        "indicator-outline.rest.ring-color",
        "color",
        "the ring separating a mark from the widget it overhangs",
    ),
    comp(
        "indicator-outline.rest.ring-width",
        "dimension",
        "that ring's width",
    ),
];
