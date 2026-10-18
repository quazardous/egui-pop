//! The contract of the default kit's `window` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp("window.grip", "dimension", "the resize edges' width"),
    comp(
        "window.corners",
        "string",
        "a window's corners where the system rounds them: round, small or square",
    ),
    comp(
        "window.rest.border-width",
        "dimension",
        "the line around a window drawn by the application (0: none)",
    ),
    comp(
        "window.rest.border-color",
        "color",
        "the line around a window drawn by the application",
    ),
];
