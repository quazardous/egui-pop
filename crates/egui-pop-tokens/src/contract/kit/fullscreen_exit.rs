//! The contract of the default kit's `fullscreen-exit` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "fullscreen-exit.shape",
        "string",
        "the handler drawing the full screen strip",
    ),
    comp(
        "fullscreen-exit.padding-x",
        "dimension",
        "between the full screen strip's content and its sides",
    ),
    comp(
        "fullscreen-exit.padding-y",
        "dimension",
        "between the full screen strip's content and its top and bottom",
    ),
    comp(
        "fullscreen-exit.rest.fill",
        "color",
        "the full screen strip's fill at rest",
    ),
    comp(
        "fullscreen-exit.rest.border-color",
        "color",
        "the full screen strip's border at rest",
    ),
    comp(
        "fullscreen-exit.rest.border-width",
        "dimension",
        "the full screen strip's border width (0: none)",
    ),
    comp(
        "fullscreen-exit.rest.radius",
        "dimension",
        "the full screen strip's corner radius",
    ),
];
