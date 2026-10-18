//! The contract of the default kit's `empty-state` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "empty-state.shape",
        "string",
        "the handler drawing an empty state's box",
    ),
    comp(
        "empty-state.padding-y",
        "dimension",
        "above and below an empty state's content",
    ),
    comp("empty-state.rest.fill", "color", "an empty state's box"),
    comp(
        "empty-state.rest.border-color",
        "color",
        "an empty state's border",
    ),
    comp(
        "empty-state.rest.border-width",
        "dimension",
        "an empty state's border width (0: none)",
    ),
    comp(
        "empty-state.rest.radius",
        "dimension",
        "an empty state's corner radius",
    ),
    comp(
        "empty-state.rest.text",
        "color",
        "an empty state's icon and sentence",
    ),
    comp(
        "empty-state.tone.danger.text",
        "color",
        "an empty state's icon when something went wrong",
    ),
];
