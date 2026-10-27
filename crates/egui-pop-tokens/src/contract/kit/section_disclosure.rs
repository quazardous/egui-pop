//! The contract of the default kit's `section-disclosure` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "section-disclosure.shape",
        "string",
        "the handler drawing a section's disclosure mark",
    ),
    comp(
        "section-disclosure.width",
        "dimension",
        "a section's disclosure mark, across",
    ),
    comp(
        "section-disclosure.rest.fill",
        "color",
        "the box under a section's disclosure mark",
    ),
    comp(
        "section-disclosure.rest.border-color",
        "color",
        "the outline of a section disclosure mark's box",
    ),
    comp(
        "section-disclosure.rest.border-width",
        "dimension",
        "the outline width of a section disclosure mark's box",
    ),
    comp(
        "section-disclosure.rest.radius",
        "dimension",
        "the corners of a section disclosure mark's box",
    ),
    comp(
        "section-disclosure.rest.text",
        "color",
        "a section's disclosure mark",
    ),
];
