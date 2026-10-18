//! The contract of the default kit's `pill` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp("pill.shape", "string", "the handler drawing a pill"),
    comp(
        "pill.padding-x",
        "dimension",
        "between a pill's label and its sides",
    ),
    comp(
        "pill.padding-y",
        "dimension",
        "between a pill's content and its top and bottom",
    ),
    comp(
        "pill.rest.fill",
        "color",
        "a pill's fill at rest (a surface at 85 %)",
    ),
    comp("pill.rest.border-color", "color", "a pill's border"),
    comp(
        "pill.rest.border-width",
        "dimension",
        "a pill's border width (0: none)",
    ),
    comp("pill.rest.radius", "dimension", "a pill's corner radius"),
    comp("pill.rest.text", "color", "a pill's text"),
    comp("pill.tone.info.fill", "color", "an info badge's fill"),
    comp("pill.tone.info.text", "color", "an info badge's text"),
    comp("pill.tone.success.fill", "color", "a success badge's fill"),
    comp("pill.tone.success.text", "color", "a success badge's text"),
    comp("pill.tone.warning.fill", "color", "a warning badge's fill"),
    comp("pill.tone.warning.text", "color", "a warning badge's text"),
    comp("pill.tone.danger.fill", "color", "a danger badge's fill"),
    comp("pill.tone.danger.text", "color", "a danger badge's text"),
];
