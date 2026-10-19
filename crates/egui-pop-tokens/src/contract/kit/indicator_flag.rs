//! The contract of the default kit's `indicator-flag` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "indicator-flag.shape",
        "string",
        "the handler drawing a flag",
    ),
    comp("indicator-flag.width", "dimension", "a flag's size"),
    comp("indicator-flag.rest.fill", "color", "a flag's colour"),
    comp(
        "indicator-flag.tone.accent.fill",
        "color",
        "a flag in the accent tone",
    ),
    comp(
        "indicator-flag.tone.info.fill",
        "color",
        "a flag in the info tone",
    ),
    comp(
        "indicator-flag.tone.success.fill",
        "color",
        "a flag in the success tone",
    ),
    comp(
        "indicator-flag.tone.warning.fill",
        "color",
        "a flag in the warning tone",
    ),
    comp(
        "indicator-flag.tone.danger.fill",
        "color",
        "a flag in the danger tone",
    ),
    comp(
        "indicator-flag.tone.muted.fill",
        "color",
        "a flag in the muted tone",
    ),
    comp(
        "indicator-flag.tone.record.fill",
        "color",
        "a flag in the record tone",
    ),
];
