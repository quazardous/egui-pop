//! The contract of the default kit's `meter` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp("meter.shape", "string", "the handler drawing a meter"),
    comp("meter.height", "dimension", "a meter's thickness"),
    comp("meter.rest.fill", "color", "a meter's track"),
    comp(
        "meter.rest.radius",
        "dimension",
        "a meter's corner radius (at least half the height: round ends)",
    ),
];
