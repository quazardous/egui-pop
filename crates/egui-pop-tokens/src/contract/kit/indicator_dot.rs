//! The contract of the default kit's `indicator-dot` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "indicator-dot.shape",
        "string",
        "the handler drawing a dot or an LED",
    ),
    comp(
        "indicator-dot.width",
        "dimension",
        "a dot's or an LED's diameter",
    ),
    comp("indicator-dot.rest.fill", "color", "a dot's colour"),
    comp(
        "indicator-dot.rest.border-color",
        "color",
        "the ring of an LED that is off",
    ),
    comp(
        "indicator-dot.rest.border-width",
        "dimension",
        "the ring of an LED that is off, and of a pulse",
    ),
    comp(
        "indicator-dot.tone.accent.fill",
        "color",
        "a dot in the accent tone",
    ),
    comp(
        "indicator-dot.tone.info.fill",
        "color",
        "a dot in the info tone",
    ),
    comp(
        "indicator-dot.tone.success.fill",
        "color",
        "a dot in the success tone",
    ),
    comp(
        "indicator-dot.tone.warning.fill",
        "color",
        "a dot in the warning tone",
    ),
    comp(
        "indicator-dot.tone.danger.fill",
        "color",
        "a dot in the danger tone",
    ),
    comp(
        "indicator-dot.tone.muted.fill",
        "color",
        "a dot in the muted tone",
    ),
    comp(
        "indicator-dot.tone.record.fill",
        "color",
        "a dot in the record tone",
    ),
];
