//! The contract of the default kit's `popover` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp("popover.shape", "string", "the handler drawing a popover"),
    comp(
        "popover.padding-x",
        "dimension",
        "between a popover's content and its sides",
    ),
    comp(
        "popover.padding-y",
        "dimension",
        "between a popover's content and its top and bottom",
    ),
    comp("popover.rest.fill", "color", "a popover's fill at rest"),
    comp(
        "popover.rest.border-color",
        "color",
        "a popover's border at rest",
    ),
    comp(
        "popover.rest.border-width",
        "dimension",
        "a popover's border width (0: none)",
    ),
    comp(
        "popover.rest.radius",
        "dimension",
        "a popover's corner radius",
    ),
];
