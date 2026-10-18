//! The contract of the default kit's `thumb-overlay-badge` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "thumb-overlay-badge.shape",
        "string",
        "the handler drawing an overlay badge",
    ),
    comp(
        "thumb-overlay-badge.rest.fill",
        "color",
        "an overlay badge's fill at rest",
    ),
    comp(
        "thumb-overlay-badge.rest.border-color",
        "color",
        "an overlay badge's border",
    ),
    comp(
        "thumb-overlay-badge.rest.border-width",
        "dimension",
        "an overlay badge's border width (0: none)",
    ),
    comp(
        "thumb-overlay-badge.rest.radius",
        "dimension",
        "an overlay badge's corner radius",
    ),
    comp(
        "thumb-overlay-badge.rest.text",
        "color",
        "an overlay badge's text",
    ),
];
