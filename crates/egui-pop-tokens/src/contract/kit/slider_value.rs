//! The contract of the default kit's `slider-value` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "slider-value.shape",
        "string",
        "the handler drawing a slider's value",
    ),
    comp(
        "slider-value.align",
        "string",
        "where a slider's value goes",
    ),
    comp(
        "slider-value.padding-x",
        "dimension",
        "between a slider's value and its ends",
    ),
    comp("slider-value.rest.text", "color", "a slider's value"),
    comp(
        "slider-value.tone.warning.text",
        "color",
        "a slider's value past its soft range",
    ),
];
