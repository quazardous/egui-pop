//! The contract of the default kit's `card` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp("card.shape", "string", "the handler drawing a card"),
    comp(
        "card.padding-x",
        "dimension",
        "between a card's content and its sides",
    ),
    comp(
        "card.padding-y",
        "dimension",
        "between a card's content and its top and bottom",
    ),
    comp("card.rest.fill", "color", "a card's fill"),
    comp("card.rest.border-color", "color", "a card's border"),
    comp(
        "card.rest.border-width",
        "dimension",
        "a card's border width (0: none)",
    ),
    comp("card.rest.radius", "dimension", "a card's corner radius"),
    comp(
        "card.tone.accent.border-color",
        "color",
        "a card's edge in the accent tone",
    ),
    comp(
        "card.tone.info.border-color",
        "color",
        "a card's edge in the info tone",
    ),
    comp(
        "card.tone.success.border-color",
        "color",
        "a card's edge in the success tone",
    ),
    comp(
        "card.tone.warning.border-color",
        "color",
        "a card's edge in the warning tone",
    ),
    comp(
        "card.tone.danger.border-color",
        "color",
        "a card's edge in the danger tone",
    ),
    comp(
        "card.tone.record.border-color",
        "color",
        "a card's edge in the record tone",
    ),
    comp(
        "card.tone.muted.fill",
        "color",
        "a muted card's fill (a well)",
    ),
];
