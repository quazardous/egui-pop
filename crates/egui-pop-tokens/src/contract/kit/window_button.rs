//! The contract of the default kit's `window-button` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "window-button.shape",
        "string",
        "the handler drawing minimize and maximize",
    ),
    comp(
        "window-button.width",
        "dimension",
        "a window button's width",
    ),
    comp(
        "window-button.rest.fill",
        "color",
        "a window button at rest",
    ),
    comp(
        "window-button.rest.text",
        "color",
        "a window button's glyph at rest",
    ),
    comp(
        "window-button.hover.fill",
        "color",
        "a window button under the pointer",
    ),
    comp(
        "window-button.hover.text",
        "color",
        "its glyph under the pointer",
    ),
];
