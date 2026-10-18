//! The contract of the default kit's `area-grip` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "area-grip.shape",
        "string",
        "the handler drawing a side area's grip",
    ),
    comp("area-grip.width", "dimension", "the grip's thickness"),
    comp(
        "area-grip.height",
        "dimension",
        "the grip's length along the edge",
    ),
    comp("area-grip.rest.fill", "color", "the grip at rest"),
    comp("area-grip.rest.radius", "dimension", "the grip's corners"),
];
