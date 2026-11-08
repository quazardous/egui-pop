//! The contract of the default kit's `tooltip` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp("tooltip.shape", "string", "the handler drawing a tooltip"),
    comp(
        "tooltip.placement",
        "string",
        "where a tooltip goes: under the widget or under the pointer",
    ),
    comp(
        "tooltip.offset",
        "dimension",
        "the gap between a tooltip and what it hangs from",
    ),
    comp(
        "tooltip.padding-x",
        "dimension",
        "between a tooltip's text and its sides",
    ),
    comp(
        "tooltip.padding-y",
        "dimension",
        "between a tooltip's text and its top and bottom",
    ),
    comp("tooltip.rest.fill", "color", "a tooltip's fill"),
    comp("tooltip.rest.border-color", "color", "a tooltip's border"),
    comp(
        "tooltip.rest.border-width",
        "dimension",
        "a tooltip's border width (0: none)",
    ),
    comp(
        "tooltip.rest.radius",
        "dimension",
        "a tooltip's corner radius",
    ),
    comp("tooltip.rest.text", "color", "a tooltip's text"),
];
