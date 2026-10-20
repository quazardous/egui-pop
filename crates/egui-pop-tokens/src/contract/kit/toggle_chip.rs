//! The contract of the default kit's `toggle-chip` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "toggle-chip.shape",
        "string",
        "the handler drawing a toggle chip",
    ),
    comp("toggle-chip.rest.fill", "color", "a toggle chip while off"),
    comp(
        "toggle-chip.rest.border-color",
        "color",
        "a toggle chip's outline",
    ),
    comp(
        "toggle-chip.rest.border-width",
        "dimension",
        "a toggle chip's outline width (0: none)",
    ),
    comp(
        "toggle-chip.rest.radius",
        "dimension",
        "a toggle chip's corners",
    ),
    comp(
        "toggle-chip.rest.text",
        "color",
        "a toggle chip's text or glyph",
    ),
    comp(
        "toggle-chip.selected.fill",
        "color",
        "a toggle chip while on",
    ),
];
