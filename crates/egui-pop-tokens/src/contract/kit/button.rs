//! The contract of the default kit's `button` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp("button.shape", "string", "the handler drawing a button"),
    comp("button.rest.fill", "color", "a button at rest"),
    comp("button.rest.border-color", "color", "a button's outline"),
    comp(
        "button.rest.border-width",
        "dimension",
        "a button's outline width",
    ),
    comp("button.rest.radius", "dimension", "a button's corners"),
    comp("button.rest.text", "color", "a button's label"),
];
