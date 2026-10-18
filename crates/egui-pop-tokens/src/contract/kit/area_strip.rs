//! The contract of the default kit's `area-strip` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "area-strip.shape",
        "string",
        "the handler drawing a folded area's strip",
    ),
    comp(
        "area-strip.width",
        "dimension",
        "a folded area's strip width",
    ),
    comp("area-strip.rest.fill", "color", "a folded area's strip"),
    comp(
        "area-strip.rest.border-color",
        "color",
        "the strip's line on the centre's side",
    ),
    comp(
        "area-strip.rest.border-width",
        "dimension",
        "that line's width",
    ),
    comp("area-strip.rest.text", "color", "the strip's grip glyph"),
];
