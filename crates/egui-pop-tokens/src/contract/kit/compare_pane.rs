//! The contract of the default kit's `compare-pane` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "compare-pane.shape",
        "string",
        "the handler drawing a comparison pane",
    ),
    comp(
        "compare-pane.rest.fill",
        "color",
        "a comparison pane's fill at rest",
    ),
    comp(
        "compare-pane.rest.border-color",
        "color",
        "a comparison pane's border",
    ),
    comp(
        "compare-pane.rest.border-width",
        "dimension",
        "a comparison pane's border width (0: none)",
    ),
    comp(
        "compare-pane.rest.radius",
        "dimension",
        "a comparison pane's corner radius",
    ),
    comp(
        "compare-pane.rest.text",
        "color",
        "a comparison pane's image name",
    ),
];
