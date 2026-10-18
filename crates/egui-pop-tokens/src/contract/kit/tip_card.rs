//! The contract of the default kit's `tip-card` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp("tip-card.shape", "string", "the handler drawing a tip card"),
    comp(
        "tip-card.padding-x",
        "dimension",
        "between a tip card's content and its sides",
    ),
    comp(
        "tip-card.padding-y",
        "dimension",
        "between a tip card's content and its top and bottom",
    ),
    comp("tip-card.rest.fill", "color", "a tip card's fill at rest"),
    comp(
        "tip-card.rest.border-color",
        "color",
        "a tip card's border at rest",
    ),
    comp(
        "tip-card.rest.border-width",
        "dimension",
        "a tip card's border width (0: none)",
    ),
    comp(
        "tip-card.rest.radius",
        "dimension",
        "a tip card's corner radius",
    ),
];
