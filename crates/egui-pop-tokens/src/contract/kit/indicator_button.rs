//! The contract of the default kit's `indicator-button` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "indicator-button.shape",
        "string",
        "the handler drawing an interactive mark's target",
    ),
    comp(
        "indicator-button.width",
        "dimension",
        "an interactive mark's smallest target width",
    ),
    comp(
        "indicator-button.height",
        "dimension",
        "an interactive mark's smallest target height",
    ),
    comp(
        "indicator-button.rest.fill",
        "color",
        "an interactive mark's target at rest",
    ),
    comp("indicator-button.rest.border-color", "color", "its border"),
    comp(
        "indicator-button.rest.border-width",
        "dimension",
        "its border width (0: none)",
    ),
    comp(
        "indicator-button.rest.radius",
        "dimension",
        "its corner radius",
    ),
    comp(
        "indicator-button.rest.text",
        "color",
        "an icon mark's colour",
    ),
    comp(
        "indicator-button.tone.accent.text",
        "color",
        "an icon mark in the accent tone",
    ),
    comp(
        "indicator-button.tone.info.text",
        "color",
        "an icon mark in the info tone",
    ),
    comp(
        "indicator-button.tone.success.text",
        "color",
        "an icon mark in the success tone",
    ),
    comp(
        "indicator-button.tone.warning.text",
        "color",
        "an icon mark in the warning tone",
    ),
    comp(
        "indicator-button.tone.danger.text",
        "color",
        "an icon mark in the danger tone",
    ),
    comp(
        "indicator-button.tone.muted.text",
        "color",
        "an icon mark in the muted tone",
    ),
    comp(
        "indicator-button.tone.record.text",
        "color",
        "an icon mark in the record tone",
    ),
];
