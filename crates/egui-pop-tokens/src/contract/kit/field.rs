//! The contract of the default kit's `field` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp("field.shape", "string", "the handler drawing a text field"),
    comp("field.rest.fill", "color", "a text field's background"),
    comp("field.rest.border-color", "color", "a text field's outline"),
    comp(
        "field.rest.border-width",
        "dimension",
        "a text field's outline width",
    ),
    comp("field.rest.radius", "dimension", "a text field's corners"),
    comp("field.rest.text", "color", "a text field's text"),
    comp(
        "field.hover.border-color",
        "color",
        "a text field's outline under the pointer",
    ),
    comp(
        "field.tone.muted.fill",
        "color",
        "a read-only field's fill: selectable, not editable",
    ),
    comp(
        "field.tone.muted.text",
        "color",
        "the hint an empty field shows",
    ),
    comp(
        "field.tone.accent.border-color",
        "color",
        "the edge and marker of a field changed from its saved or default value",
    ),
    comp(
        "field.tone.warning.border-color",
        "color",
        "the marker of a field that must be given",
    ),
    comp(
        "field.tone.danger.border-color",
        "color",
        "the edge, icon and message of a refused value",
    ),
    comp(
        "field.tone.info.border-color",
        "color",
        "the edge and icon of a value being checked",
    ),
];
