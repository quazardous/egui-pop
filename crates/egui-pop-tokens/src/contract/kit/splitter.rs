//! The contract of the default kit's `splitter` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp("splitter.shape", "string", "the handler drawing a splitter"),
    comp("splitter.width", "dimension", "a splitter line's thickness"),
    comp("splitter.rest.fill", "color", "a splitter's line at rest"),
    comp(
        "splitter.hover.fill",
        "color",
        "a splitter's line under the pointer",
    ),
    comp(
        "splitter.active.fill",
        "color",
        "a splitter's line while dragged",
    ),
];
