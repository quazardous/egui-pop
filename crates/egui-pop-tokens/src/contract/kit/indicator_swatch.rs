//! The contract of the default kit's `indicator-swatch` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "indicator-swatch.shape",
        "string",
        "the handler drawing a swatch",
    ),
    comp("indicator-swatch.width", "dimension", "a swatch's side"),
    comp(
        "indicator-swatch.rest.fill",
        "color",
        "over a swatch's colour (transparent)",
    ),
    comp(
        "indicator-swatch.rest.border-color",
        "color",
        "a swatch's outline",
    ),
    comp(
        "indicator-swatch.rest.border-width",
        "dimension",
        "a swatch's outline width",
    ),
    comp(
        "indicator-swatch.rest.radius",
        "dimension",
        "a swatch's corners",
    ),
    comp(
        "indicator-swatch.rest.text",
        "color",
        "the slash of no colour, the dash of mixed colours",
    ),
];
