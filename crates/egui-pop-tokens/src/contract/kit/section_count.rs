//! The contract of the default kit's `section-count` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "section-count.shape",
        "string",
        "the handler drawing a section's count",
    ),
    comp(
        "section-count.height",
        "dimension",
        "a section count's height",
    ),
    comp(
        "section-count.padding-x",
        "dimension",
        "either side of a section count's text",
    ),
    comp(
        "section-count.font-size",
        "dimension",
        "a section count's text size",
    ),
    comp("section-count.rest.fill", "color", "a section count's fill"),
    comp(
        "section-count.rest.border-color",
        "color",
        "a section count's outline",
    ),
    comp(
        "section-count.rest.border-width",
        "dimension",
        "a section count's outline width (0: none)",
    ),
    comp(
        "section-count.rest.radius",
        "dimension",
        "a section count's corner radius",
    ),
    comp("section-count.rest.text", "color", "a section count's text"),
];
