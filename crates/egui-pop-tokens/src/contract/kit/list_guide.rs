//! The contract of the default kit's `list-guide` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "list-guide.shape",
        "string",
        "the handler drawing a nested row's indent guides",
    ),
    comp("list-guide.rest.border-color", "color", "an indent guide"),
    comp(
        "list-guide.rest.border-width",
        "dimension",
        "an indent guide's width (0: none)",
    ),
];
