//! The contract of the default kit's `fault-card` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "fault-card.shape",
        "string",
        "the handler drawing a fault card",
    ),
    comp(
        "fault-card.padding-x",
        "dimension",
        "between a fault card's content and its sides",
    ),
    comp(
        "fault-card.padding-y",
        "dimension",
        "between a fault card's content and its top and bottom",
    ),
    comp(
        "fault-card.rest.fill",
        "color",
        "a fault card's fill at rest",
    ),
    comp(
        "fault-card.rest.border-color",
        "color",
        "a fault card's border at rest",
    ),
    comp(
        "fault-card.rest.border-width",
        "dimension",
        "a fault card's border width (0: none)",
    ),
    comp(
        "fault-card.rest.radius",
        "dimension",
        "a fault card's corner radius",
    ),
];
