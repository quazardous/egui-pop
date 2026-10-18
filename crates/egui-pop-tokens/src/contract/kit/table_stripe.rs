//! The contract of the default kit's `table-stripe` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "table-stripe.shape",
        "string",
        "the handler drawing a table's stripes",
    ),
    comp(
        "table-stripe.rest.fill",
        "color",
        "behind every other row of a table",
    ),
];
