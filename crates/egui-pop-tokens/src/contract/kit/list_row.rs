//! The contract of the default kit's `list-row` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "list-row.shape",
        "string",
        "the handler drawing a tree's, a list's or a table's row",
    ),
    comp("list-row.rest.fill", "color", "a row at rest"),
    comp("list-row.rest.border-color", "color", "a row's border"),
    comp(
        "list-row.rest.border-width",
        "dimension",
        "a row's border width (0: none)",
    ),
    comp("list-row.rest.radius", "dimension", "a row's corner radius"),
    comp("list-row.rest.text", "color", "a row's text at rest"),
];
