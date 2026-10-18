//! The contract of the default kit's `tab-bar` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp("tab-bar.shape", "string", "the handler drawing a tab bar"),
    comp("tab-bar.height", "dimension", "a tab bar's height"),
    comp(
        "tab-bar.indicator-side",
        "string",
        "the side of a tab bar's line",
    ),
    comp("tab-bar.rest.fill", "color", "behind a row of tabs"),
    comp(
        "tab-bar.rest.border-color",
        "color",
        "the line along a tab bar",
    ),
    comp("tab-bar.rest.border-width", "dimension", "the line's width"),
];
