//! The contract of the default kit's `editor-frame` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "editor-frame.shape",
        "string",
        "the handler drawing an editor",
    ),
    comp(
        "editor-frame.padding-x",
        "dimension",
        "between an editor's content and its sides",
    ),
    comp(
        "editor-frame.padding-y",
        "dimension",
        "between an editor's content and its top and bottom",
    ),
    comp(
        "editor-frame.rest.fill",
        "color",
        "an editor's fill at rest",
    ),
    comp(
        "editor-frame.rest.border-color",
        "color",
        "an editor's border at rest",
    ),
    comp(
        "editor-frame.rest.border-width",
        "dimension",
        "an editor's border width (0: none)",
    ),
    comp(
        "editor-frame.rest.radius",
        "dimension",
        "an editor's corner radius",
    ),
];
