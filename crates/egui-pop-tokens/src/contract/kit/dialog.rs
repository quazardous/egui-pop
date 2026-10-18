//! The contract of the default kit's `dialog` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp("dialog.shape", "string", "the handler drawing a dialog"),
    comp(
        "dialog.padding-x",
        "dimension",
        "between a dialog's content and its sides",
    ),
    comp(
        "dialog.padding-y",
        "dimension",
        "between a dialog's content and its top and bottom",
    ),
    comp("dialog.width", "dimension", "a dialog's width"),
    comp("dialog.rest.fill", "color", "a dialog's fill at rest"),
    comp(
        "dialog.rest.border-color",
        "color",
        "a dialog's border at rest",
    ),
    comp(
        "dialog.rest.border-width",
        "dimension",
        "a dialog's border width (0: none)",
    ),
    comp(
        "dialog.rest.radius",
        "dimension",
        "a dialog's corner radius",
    ),
];
