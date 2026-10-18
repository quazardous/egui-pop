//! The contract of the default kit's `meter-tick` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "meter-tick.shape",
        "string",
        "the handler drawing a meter tick",
    ),
    comp(
        "meter-tick.width",
        "dimension",
        "a meter's threshold line width",
    ),
    comp("meter-tick.rest.fill", "color", "a meter's threshold line"),
];
