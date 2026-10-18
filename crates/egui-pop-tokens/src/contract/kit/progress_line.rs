//! The contract of the default kit's `progress-line` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "progress-line.shape",
        "string",
        "the handler drawing a progress line",
    ),
    comp(
        "progress-line.height",
        "dimension",
        "a thin progress line's thickness",
    ),
    comp(
        "progress-line.rest.fill",
        "color",
        "a thin progress line's track",
    ),
    comp(
        "progress-line.rest.radius",
        "dimension",
        "a progress line's corner radius (at least half the height: round ends)",
    ),
];
