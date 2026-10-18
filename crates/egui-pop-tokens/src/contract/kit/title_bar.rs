//! The contract of the default kit's `title-bar` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "title-bar.shape",
        "string",
        "the handler drawing a window's own title bar",
    ),
    comp("title-bar.height", "dimension", "the title bar's height"),
    comp(
        "title-bar.padding-x",
        "dimension",
        "between the title and the bar's items",
    ),
    comp(
        "title-bar.align",
        "string",
        "where the title goes: start, centre or end",
    ),
    comp(
        "title-bar.indicator-side",
        "string",
        "the side of the title bar's line",
    ),
    comp(
        "title-bar.buttons-side",
        "string",
        "where the window buttons go: end or start",
    ),
    comp(
        "title-bar.buttons-hover",
        "string",
        "which window buttons hover: own or group",
    ),
    comp("title-bar.rest.fill", "color", "a window's own title bar"),
    comp("title-bar.rest.text", "color", "the window's title"),
    comp(
        "title-bar.rest.border-color",
        "color",
        "the line along the title bar",
    ),
    comp(
        "title-bar.rest.border-width",
        "dimension",
        "the line's width (0: none)",
    ),
];
