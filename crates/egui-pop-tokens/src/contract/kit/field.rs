//! The contract of the default kit's `field` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp("field.shape", "string", "the handler drawing a text field"),
    comp("field.rest.fill", "color", "a text field's background"),
    comp("field.rest.border-color", "color", "a text field's outline"),
    comp(
        "field.rest.border-width",
        "dimension",
        "a text field's outline width",
    ),
    comp("field.rest.radius", "dimension", "a text field's corners"),
    comp("field.rest.text", "color", "a text field's text"),
];
