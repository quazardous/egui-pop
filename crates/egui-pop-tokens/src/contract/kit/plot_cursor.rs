//! The contract of the default kit's `plot-cursor` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "plot-cursor.shape",
        "string",
        "the handler drawing a plot's time cursor",
    ),
    comp(
        "plot-cursor.width",
        "dimension",
        "a plot's time cursor width",
    ),
    comp(
        "plot-cursor.head",
        "string",
        "the mark at the top of a plot's time cursor: none or triangle",
    ),
    comp(
        "plot-cursor.head-size",
        "dimension",
        "the width of a plot's time cursor head",
    ),
    comp("plot-cursor.rest.fill", "color", "a plot's time cursor"),
    comp(
        "plot-cursor.hover.fill",
        "color",
        "a plot's time cursor under the pointer or dragged",
    ),
];
