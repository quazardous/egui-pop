//! The contract of the default kit's `window-close` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "window-close.shape",
        "string",
        "the handler drawing the close button",
    ),
    comp(
        "window-close.width",
        "dimension",
        "the close button's width",
    ),
    comp(
        "window-close.rest.fill",
        "color",
        "the close button at rest",
    ),
    comp(
        "window-close.rest.text",
        "color",
        "the close button's glyph at rest",
    ),
    comp(
        "window-close.hover.fill",
        "color",
        "the close button under the pointer",
    ),
    comp(
        "window-close.hover.text",
        "color",
        "its glyph under the pointer",
    ),
];
