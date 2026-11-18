//! The contract of the default kit's `switch` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "switch.shape",
        "string",
        "the handler drawing a switch's track",
    ),
    comp("switch.rest.fill", "color", "a switch's track while off"),
    comp("switch.rest.border-color", "color", "a switch's outline"),
    comp(
        "switch.rest.border-width",
        "dimension",
        "a switch's outline width",
    ),
    comp(
        "switch.rest.radius",
        "dimension",
        "a switch's track's corners",
    ),
    comp("switch.rest.text", "color", "a switch's thumb and label"),
    comp("switch.selected.fill", "color", "a switch's track while on"),
    comp("switch.selected.text", "color", "a switch's thumb while on"),
];
