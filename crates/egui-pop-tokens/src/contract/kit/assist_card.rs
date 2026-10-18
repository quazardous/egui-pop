//! The contract of the default kit's `assist-card` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "assist-card.shape",
        "string",
        "the handler drawing a code assist card",
    ),
    comp(
        "assist-card.padding-x",
        "dimension",
        "between a code assist card's content and its sides",
    ),
    comp(
        "assist-card.padding-y",
        "dimension",
        "between a code assist card's content and its top and bottom",
    ),
    comp(
        "assist-card.rest.fill",
        "color",
        "a code assist card's fill at rest",
    ),
    comp(
        "assist-card.rest.border-color",
        "color",
        "a code assist card's border at rest",
    ),
    comp(
        "assist-card.rest.border-width",
        "dimension",
        "a code assist card's border width (0: none)",
    ),
    comp(
        "assist-card.rest.radius",
        "dimension",
        "a code assist card's corner radius",
    ),
];
