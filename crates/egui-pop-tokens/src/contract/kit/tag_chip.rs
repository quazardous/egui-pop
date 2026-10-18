//! The contract of the default kit's `tag-chip` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp("tag-chip.shape", "string", "the handler drawing a tag chip"),
    comp(
        "tag-chip.padding-x",
        "dimension",
        "between a tag chip's label and its sides",
    ),
    comp("tag-chip.align", "string", "where a tag chip's label goes"),
    comp("tag-chip.rest.fill", "color", "a tag chip's fill at rest"),
    comp("tag-chip.rest.border-color", "color", "a tag chip's border"),
    comp(
        "tag-chip.rest.border-width",
        "dimension",
        "a tag chip's border width (0: none)",
    ),
    comp(
        "tag-chip.rest.radius",
        "dimension",
        "a tag chip's corner radius",
    ),
    comp("tag-chip.rest.text", "color", "a tag chip's text"),
];
