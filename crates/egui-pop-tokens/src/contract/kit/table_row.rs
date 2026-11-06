//! The contract of the default kit's `table-row` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "table-row.shape",
        "string",
        "the handler drawing a data table's row",
    ),
    comp("table-row.rest.fill", "color", "a table's row at rest"),
    comp(
        "table-row.rest.border-color",
        "color",
        "a table's row's border",
    ),
    comp(
        "table-row.rest.border-width",
        "dimension",
        "a table's row's border width (0: none)",
    ),
    comp(
        "table-row.rest.radius",
        "dimension",
        "a table's row's corner radius",
    ),
    comp(
        "table-row.rest.text",
        "color",
        "a table's row's text at rest",
    ),
];
