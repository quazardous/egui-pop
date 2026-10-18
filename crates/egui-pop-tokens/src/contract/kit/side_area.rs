//! The contract of the default kit's `side-area` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "side-area.shape",
        "string",
        "the handler drawing a side area",
    ),
    comp(
        "side-area.padding-x",
        "dimension",
        "between a side area's start and end edges and its panels",
    ),
    comp(
        "side-area.padding-y",
        "dimension",
        "between a side area's top and bottom edges and its panels",
    ),
    comp(
        "side-area.shadow-offset-x",
        "dimension",
        "the shadow of a side area lying over the centre: offset towards the centre",
    ),
    comp(
        "side-area.shadow-offset-y",
        "dimension",
        "the shadow of a side area lying over the centre: offset down",
    ),
    comp(
        "side-area.shadow-blur",
        "dimension",
        "the shadow of a side area lying over the centre: penumbra width",
    ),
    comp(
        "side-area.shadow-spread",
        "dimension",
        "the shadow of a side area lying over the centre: growth on every side",
    ),
    comp(
        "side-area.shadow-color",
        "color",
        "the shadow of a side area lying over the centre: colour",
    ),
    comp(
        "side-area.rest.border-width",
        "dimension",
        "a side area's separator line (0: none)",
    ),
];
