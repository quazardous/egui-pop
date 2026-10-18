//! The contract of the default kit's `list-view` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "list-view.shape",
        "string",
        "the handler drawing a data view's outline",
    ),
    comp(
        "list-view.rest.fill",
        "color",
        "over a data view's rows (transparent)",
    ),
];
