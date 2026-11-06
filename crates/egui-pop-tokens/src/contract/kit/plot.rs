//! The contract of the default kit's `plot` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp("plot.shape", "string", "the handler drawing a plot's area"),
    comp("plot.rest.fill", "color", "a plot's background"),
    comp("plot.rest.border-color", "color", "a plot's frame"),
    comp(
        "plot.rest.border-width",
        "dimension",
        "a plot's frame width (0: none)",
    ),
    comp("plot.rest.radius", "dimension", "a plot's corner radius"),
    comp(
        "plot.rest.text",
        "color",
        "a plot's axis text and its crosshair",
    ),
    comp(
        "plot.grid-color",
        "color",
        "a plot's grid lines, at their strongest (decoration)",
    ),
    comp(
        "plot.grid-fade",
        "number",
        "how much fainter a plot's denser grid lines are, 0 (as strong as the sparsest) to 1",
    ),
];
