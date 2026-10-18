//! The contract of the default kit's `tab` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp("tab.shape", "string", "the handler drawing a tab"),
    comp(
        "tab.padding-x",
        "dimension",
        "between a tab's label and its sides",
    ),
    comp("tab.height", "dimension", "a tab's height"),
    comp(
        "tab.indicator-side",
        "string",
        "the side of a tab's indicator bar",
    ),
    comp("tab.rest.fill", "color", "a tab at rest"),
    comp("tab.rest.border-color", "color", "a tab's border at rest"),
    comp("tab.rest.border-width", "dimension", "a tab's border width"),
    comp("tab.rest.radius", "dimension", "a tab's corner radius"),
    comp("tab.rest.text", "color", "a tab's label at rest"),
    comp("tab.rest.indicator-color", "color", "a tab's indicator bar"),
    comp(
        "tab.rest.indicator-size",
        "dimension",
        "a tab's indicator bar thickness",
    ),
];
