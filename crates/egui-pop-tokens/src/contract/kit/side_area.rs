//! The contract of the default kit's `side-area` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "side-area.shape",
        "string",
        "the handler drawing a side area",
    ),
    comp(
        "side-area.rest.border-width",
        "dimension",
        "a side area's separator line (0: none)",
    ),
];
