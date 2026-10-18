//! The contract of the default kit's `slider` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "slider.shape",
        "string",
        "the handler drawing a slider's bar",
    ),
    comp("slider.align", "string", "where a slider's label goes"),
    comp(
        "slider.padding-x",
        "dimension",
        "between a slider's label and its ends",
    ),
    comp("slider.rest.fill", "color", "a slider's track"),
    comp("slider.rest.border-color", "color", "a slider's border"),
    comp(
        "slider.rest.border-width",
        "dimension",
        "a slider's border width (0: none)",
    ),
    comp(
        "slider.rest.radius",
        "dimension",
        "a slider's corner radius",
    ),
    comp("slider.rest.text", "color", "a slider's label"),
];
