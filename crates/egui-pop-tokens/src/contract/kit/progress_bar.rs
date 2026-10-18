//! The contract of the default kit's `progress-bar` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "progress-bar.shape",
        "string",
        "the handler drawing a progress bar",
    ),
    comp(
        "progress-bar.height",
        "dimension",
        "a progress bar's thickness",
    ),
    comp("progress-bar.rest.fill", "color", "a progress bar's track"),
    comp(
        "progress-bar.rest.radius",
        "dimension",
        "a progress bar's corner radius (at least half the height: round ends)",
    ),
];
