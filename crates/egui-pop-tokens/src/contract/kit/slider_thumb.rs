//! The contract of the default kit's `slider-thumb` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "slider-thumb.shape",
        "string",
        "the handler drawing a slider's thumb",
    ),
    comp(
        "slider-thumb.width",
        "dimension",
        "a slider's thumb's thickness",
    ),
    comp("slider-thumb.rest.fill", "color", "a slider's thumb"),
];
