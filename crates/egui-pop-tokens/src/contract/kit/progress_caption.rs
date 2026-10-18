//! The contract of the default kit's `progress-caption` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "progress-caption.shape",
        "string",
        "the handler drawing a progress caption",
    ),
    comp(
        "progress-caption.align",
        "string",
        "a progress caption's alignment",
    ),
    comp(
        "progress-caption.padding-x",
        "dimension",
        "a progress caption's padding",
    ),
    comp(
        "progress-caption.rest.text",
        "color",
        "a progress caption's name",
    ),
    comp(
        "progress-caption.tone.muted.text",
        "color",
        "a progress caption's value text (the muted tone)",
    ),
];
