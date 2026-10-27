//! The contract of the default kit's `section-band` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "section-band.shape",
        "string",
        "the handler drawing a section's band",
    ),
    comp(
        "section-band.height",
        "dimension",
        "a section band's height",
    ),
    comp(
        "section-band.padding-x",
        "dimension",
        "between a section band's sides and its mark and count",
    ),
    comp(
        "section-band.font-size",
        "dimension",
        "a section's title size",
    ),
    comp(
        "section-band.align",
        "string",
        "where a section's title goes: start, centre or end",
    ),
    comp("section-band.rest.fill", "color", "a section band at rest"),
    comp(
        "section-band.rest.border-color",
        "color",
        "the lines above and under a section band",
    ),
    comp(
        "section-band.rest.border-width",
        "dimension",
        "the width of the lines above and under a section band",
    ),
    comp(
        "section-band.rest.radius",
        "dimension",
        "a section band's corners",
    ),
    comp("section-band.rest.text", "color", "a section's title"),
    comp(
        "section-band.hover.fill",
        "color",
        "a section band under the pointer",
    ),
];
