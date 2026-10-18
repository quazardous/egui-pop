//! The contract of the default kit's `find-bar` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp("find-bar.shape", "string", "the handler drawing a find bar"),
    comp(
        "find-bar.padding-x",
        "dimension",
        "between a find bar's content and its sides",
    ),
    comp(
        "find-bar.padding-y",
        "dimension",
        "between a find bar's content and its top and bottom",
    ),
    comp("find-bar.rest.fill", "color", "a find bar's fill at rest"),
    comp(
        "find-bar.rest.border-color",
        "color",
        "a find bar's border at rest",
    ),
    comp(
        "find-bar.rest.border-width",
        "dimension",
        "a find bar's border width (0: none)",
    ),
    comp(
        "find-bar.rest.radius",
        "dimension",
        "a find bar's corner radius",
    ),
];
