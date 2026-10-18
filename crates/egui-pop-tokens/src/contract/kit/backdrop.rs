//! The contract of the default kit's `backdrop` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "backdrop.shape",
        "string",
        "the handler drawing the backdrop",
    ),
    comp(
        "backdrop.opacity",
        "number",
        "how much the backdrop hides what is behind it (0 to 1)",
    ),
    comp("backdrop.rest.fill", "color", "the backdrop's fill at rest"),
];
