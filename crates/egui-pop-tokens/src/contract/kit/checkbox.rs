//! The contract of the default kit's `checkbox` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "checkbox.shape",
        "string",
        "the handler drawing a checkbox's box",
    ),
    comp("checkbox.rest.fill", "color", "a checkbox at rest"),
    comp(
        "checkbox.rest.border-color",
        "color",
        "a checkbox's outline",
    ),
    comp(
        "checkbox.rest.border-width",
        "dimension",
        "a checkbox's outline width",
    ),
    comp("checkbox.rest.radius", "dimension", "a checkbox's corners"),
    comp("checkbox.rest.text", "color", "a checkbox's mark and label"),
    comp("checkbox.selected.fill", "color", "a checked box"),
];
