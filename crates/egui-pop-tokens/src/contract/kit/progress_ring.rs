//! The contract of the default kit's `progress-ring` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "progress-ring.shape",
        "string",
        "the handler drawing a progress ring",
    ),
    comp(
        "progress-ring.width",
        "dimension",
        "a progress ring's diameter",
    ),
    comp(
        "progress-ring.rest.fill",
        "color",
        "a progress ring's track",
    ),
    comp(
        "progress-ring.rest.indicator-color",
        "color",
        "a progress ring's arc",
    ),
    comp(
        "progress-ring.tone.accent.indicator-color",
        "color",
        "a progress ring's arc in the accent tone",
    ),
    comp(
        "progress-ring.tone.info.indicator-color",
        "color",
        "a progress ring's arc in the info tone",
    ),
    comp(
        "progress-ring.tone.success.indicator-color",
        "color",
        "a progress ring's arc in the success tone",
    ),
    comp(
        "progress-ring.tone.warning.indicator-color",
        "color",
        "a progress ring's arc in the warning tone",
    ),
    comp(
        "progress-ring.tone.danger.indicator-color",
        "color",
        "a progress ring's arc in the danger tone",
    ),
    comp(
        "progress-ring.tone.muted.indicator-color",
        "color",
        "a progress ring's arc in the muted tone",
    ),
    comp(
        "progress-ring.tone.record.indicator-color",
        "color",
        "a progress ring's arc in the record tone",
    ),
];
