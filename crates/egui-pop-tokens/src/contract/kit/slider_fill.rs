//! The contract of the default kit's `slider-fill` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "slider-fill.shape",
        "string",
        "the handler drawing a slider's fill",
    ),
    comp(
        "slider-fill.rest.fill",
        "color",
        "a slider's fill up to the value",
    ),
    comp(
        "slider-fill.rest.radius",
        "dimension",
        "a slider's fill's corner radius",
    ),
];
