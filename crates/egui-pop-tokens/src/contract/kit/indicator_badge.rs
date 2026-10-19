//! The contract of the default kit's `indicator-badge` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "indicator-badge.shape",
        "string",
        "the handler drawing a badge",
    ),
    comp("indicator-badge.height", "dimension", "a badge's height"),
    comp(
        "indicator-badge.padding-x",
        "dimension",
        "either side of a badge's text",
    ),
    comp(
        "indicator-badge.font-size",
        "dimension",
        "a badge's text size",
    ),
    comp("indicator-badge.rest.fill", "color", "a badge's fill"),
    comp(
        "indicator-badge.rest.border-color",
        "color",
        "a badge's border",
    ),
    comp(
        "indicator-badge.rest.border-width",
        "dimension",
        "a badge's border width (0: none)",
    ),
    comp(
        "indicator-badge.rest.radius",
        "dimension",
        "a badge's corner radius",
    ),
    comp(
        "indicator-badge.rest.text",
        "color",
        "a badge's text, on every tone",
    ),
    comp(
        "indicator-badge.tone.accent.fill",
        "color",
        "a badge's fill in the accent tone",
    ),
    comp(
        "indicator-badge.tone.info.fill",
        "color",
        "a badge's fill in the info tone",
    ),
    comp(
        "indicator-badge.tone.success.fill",
        "color",
        "a badge's fill in the success tone",
    ),
    comp(
        "indicator-badge.tone.warning.fill",
        "color",
        "a badge's fill in the warning tone",
    ),
    comp(
        "indicator-badge.tone.danger.fill",
        "color",
        "a badge's fill in the danger tone",
    ),
    comp(
        "indicator-badge.tone.muted.fill",
        "color",
        "a badge's fill in the muted tone",
    ),
    comp(
        "indicator-badge.tone.record.fill",
        "color",
        "a badge's fill in the record tone",
    ),
];
