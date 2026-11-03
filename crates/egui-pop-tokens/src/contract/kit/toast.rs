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
    comp(
        "toast.shadow-offset-x",
        "dimension",
        "the shadow a toast casts: offset right",
    ),
    comp(
        "toast.shadow-offset-y",
        "dimension",
        "the shadow a toast casts: offset down",
    ),
    comp(
        "toast.shadow-blur",
        "dimension",
        "the shadow a toast casts: penumbra width",
    ),
    comp(
        "toast.shadow-spread",
        "dimension",
        "the shadow a toast casts: growth on every side",
    ),
    comp(
        "toast.shadow-color",
        "color",
        "the shadow a toast casts: colour",
    ),
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
