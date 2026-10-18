//! The contract of the default kit's `search-field` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "search-field.shape",
        "string",
        "the handler drawing the search field",
    ),
    comp(
        "search-field.padding-x",
        "dimension",
        "between the search field's label and its sides",
    ),
    comp(
        "search-field.padding-y",
        "dimension",
        "between the search field's content and its top and bottom",
    ),
    comp(
        "search-field.rest.fill",
        "color",
        "the search field's fill at rest",
    ),
    comp(
        "search-field.rest.border-color",
        "color",
        "the search field's border",
    ),
    comp(
        "search-field.rest.border-width",
        "dimension",
        "the search field's border width (0: none)",
    ),
    comp(
        "search-field.rest.radius",
        "dimension",
        "the search field's corner radius",
    ),
];
