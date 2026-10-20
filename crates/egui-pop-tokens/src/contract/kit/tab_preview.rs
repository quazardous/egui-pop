//! The contract of the default kit's `tab-preview` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "tab-preview.shape",
        "string",
        "the handler drawing a dragged tab's ghost",
    ),
    comp(
        "tab-preview.padding-x",
        "dimension",
        "between a tab ghost's label and its sides",
    ),
    comp("tab-preview.height", "dimension", "a tab ghost's height"),
    comp(
        "tab-preview.rest.fill",
        "color",
        "a dragged tab's ghost where it would land",
    ),
    comp(
        "tab-preview.rest.border-color",
        "color",
        "a tab ghost's border",
    ),
    comp(
        "tab-preview.rest.border-width",
        "dimension",
        "a tab ghost's border width",
    ),
    comp(
        "tab-preview.rest.radius",
        "dimension",
        "a tab ghost's corners",
    ),
    comp("tab-preview.rest.text", "color", "a tab ghost's label"),
];
