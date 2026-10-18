//! The contract of the default kit's `status-indicator` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "status-indicator.shape",
        "string",
        "the handler drawing a status indicator",
    ),
    comp(
        "status-indicator.rest.text",
        "color",
        "a status indicator's text (quiet)",
    ),
    comp(
        "status-indicator.rest.border-color",
        "color",
        "a status indicator's progress track",
    ),
    comp(
        "status-indicator.rest.indicator-color",
        "color",
        "a status indicator's progress fill (quiet)",
    ),
    comp(
        "status-indicator.rest.indicator-size",
        "dimension",
        "a status indicator's progress line thickness",
    ),
    comp(
        "status-indicator.tone.accent.text",
        "color",
        "a status indicator's text in the accent tone",
    ),
    comp(
        "status-indicator.tone.accent.indicator-color",
        "color",
        "a status indicator's progress fill in the accent tone",
    ),
    comp(
        "status-indicator.tone.success.text",
        "color",
        "a status indicator's text in the success tone",
    ),
    comp(
        "status-indicator.tone.success.indicator-color",
        "color",
        "a status indicator's progress fill in the success tone",
    ),
    comp(
        "status-indicator.tone.warning.text",
        "color",
        "a status indicator's text in the warning tone",
    ),
    comp(
        "status-indicator.tone.warning.indicator-color",
        "color",
        "a status indicator's progress fill in the warning tone",
    ),
    comp(
        "status-indicator.tone.danger.text",
        "color",
        "a status indicator's text in the danger tone",
    ),
    comp(
        "status-indicator.tone.danger.indicator-color",
        "color",
        "a status indicator's progress fill in the danger tone",
    ),
];
