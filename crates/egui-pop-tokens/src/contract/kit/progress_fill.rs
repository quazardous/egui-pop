//! The contract of the default kit's `progress-fill` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "progress-fill.shape",
        "string",
        "the handler drawing a progress fill",
    ),
    comp(
        "progress-fill.rest.fill",
        "color",
        "a progress fill while running",
    ),
    comp(
        "progress-fill.rest.radius",
        "dimension",
        "a progress fill's corner radius",
    ),
    comp(
        "progress-fill.tone.accent.fill",
        "color",
        "a progress fill in the accent tone",
    ),
    comp(
        "progress-fill.tone.info.fill",
        "color",
        "a progress fill in the info tone",
    ),
    comp(
        "progress-fill.tone.success.fill",
        "color",
        "a progress fill in the success tone",
    ),
    comp(
        "progress-fill.tone.warning.fill",
        "color",
        "a progress fill in the warning tone",
    ),
    comp(
        "progress-fill.tone.danger.fill",
        "color",
        "a progress fill in the danger tone",
    ),
    comp(
        "progress-fill.tone.muted.fill",
        "color",
        "a progress fill in the muted tone",
    ),
];
