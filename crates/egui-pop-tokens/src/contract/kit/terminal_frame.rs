//! The contract of the default kit's `terminal-frame` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "terminal-frame.shape",
        "string",
        "the handler drawing a terminal",
    ),
    comp(
        "terminal-frame.padding-x",
        "dimension",
        "between a terminal's content and its sides",
    ),
    comp(
        "terminal-frame.padding-y",
        "dimension",
        "between a terminal's content and its top and bottom",
    ),
    comp(
        "terminal-frame.rest.fill",
        "color",
        "a terminal's fill at rest",
    ),
    comp(
        "terminal-frame.rest.border-color",
        "color",
        "a terminal's border at rest",
    ),
    comp(
        "terminal-frame.rest.border-width",
        "dimension",
        "a terminal's border width (0: none)",
    ),
    comp(
        "terminal-frame.rest.radius",
        "dimension",
        "a terminal's corner radius",
    ),
];
