//! The contract of the default kit's `dialog-header` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "dialog-header.shape",
        "string",
        "the handler drawing a dialog's title row",
    ),
    comp(
        "dialog-header.padding-y",
        "dimension",
        "below a movable dialog's title, inside its band",
    ),
    comp("dialog-header.rest.text", "color", "a dialog's title"),
];
