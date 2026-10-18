//! The contract of the default kit's `toast` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp("toast.shape", "string", "the handler drawing a toast"),
    comp(
        "toast.padding-x",
        "dimension",
        "between a toast's content and its sides",
    ),
    comp(
        "toast.padding-y",
        "dimension",
        "between a toast's content and its top and bottom",
    ),
    comp("toast.width", "dimension", "a toast's width"),
    comp("toast.rest.fill", "color", "a toast's fill at rest"),
    comp(
        "toast.rest.border-color",
        "color",
        "a toast's border at rest",
    ),
    comp(
        "toast.rest.border-width",
        "dimension",
        "a toast's border width (0: none)",
    ),
    comp("toast.rest.radius", "dimension", "a toast's corner radius"),
    comp("toast.rest.text", "color", "a toast's text at rest"),
];
