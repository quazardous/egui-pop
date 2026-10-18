//! The contract of the default kit's `command-palette-row` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "command-palette-row.shape",
        "string",
        "the handler drawing a command palette row",
    ),
    comp(
        "command-palette-row.rest.fill",
        "color",
        "a command palette row at rest",
    ),
    comp(
        "command-palette-row.rest.border-color",
        "color",
        "a command palette row's outline",
    ),
    comp(
        "command-palette-row.rest.border-width",
        "dimension",
        "a command palette row's outline width",
    ),
    comp(
        "command-palette-row.rest.radius",
        "dimension",
        "a command palette row's corners",
    ),
    comp(
        "command-palette-row.rest.text",
        "color",
        "a command's title in the palette",
    ),
    comp(
        "command-palette-row.selected.fill",
        "color",
        "the command Enter runs",
    ),
    comp(
        "command-palette-row.tone.accent.text",
        "color",
        "the characters a query matched",
    ),
    comp(
        "command-palette-row.tone.muted.text",
        "color",
        "a command's keys in the palette",
    ),
];
