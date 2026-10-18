//! The contract of the default kit's `table-header-cell` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "table-header-cell.shape",
        "string",
        "the handler drawing a column's header",
    ),
    comp(
        "table-header-cell.padding-x",
        "dimension",
        "between a header's title and its sides",
    ),
    comp(
        "table-header-cell.align",
        "string",
        "where a header's title goes: start, centre or end",
    ),
    comp("table-header-cell.rest.fill", "color", "a column's header"),
    comp(
        "table-header-cell.rest.border-color",
        "color",
        "a header's border",
    ),
    comp(
        "table-header-cell.rest.border-width",
        "dimension",
        "a header's border width (0: none)",
    ),
    comp(
        "table-header-cell.rest.radius",
        "dimension",
        "a header's corner radius",
    ),
    comp("table-header-cell.rest.text", "color", "a header's title"),
];
