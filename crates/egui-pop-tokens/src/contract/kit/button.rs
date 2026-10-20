//! The contract of the default kit's `button` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp("button.shape", "string", "the handler drawing a button"),
    comp("button.rest.fill", "color", "a button at rest"),
    comp("button.rest.border-color", "color", "a button's outline"),
    comp(
        "button.rest.border-width",
        "dimension",
        "a button's outline width",
    ),
    comp("button.rest.radius", "dimension", "a button's corners"),
    comp("button.rest.text", "color", "a button's label"),
    comp(
        "button.tone.accent.fill",
        "color",
        "a button's fill in the accent tone",
    ),
    comp(
        "button.tone.accent.text",
        "color",
        "a button's label in the accent tone",
    ),
    comp(
        "button.tone.info.fill",
        "color",
        "a button's fill in the info tone",
    ),
    comp(
        "button.tone.info.text",
        "color",
        "a button's label in the info tone",
    ),
    comp(
        "button.tone.success.fill",
        "color",
        "a button's fill in the success tone",
    ),
    comp(
        "button.tone.success.text",
        "color",
        "a button's label in the success tone",
    ),
    comp(
        "button.tone.warning.fill",
        "color",
        "a button's fill in the warning tone",
    ),
    comp(
        "button.tone.warning.text",
        "color",
        "a button's label in the warning tone",
    ),
    comp(
        "button.tone.danger.fill",
        "color",
        "a button's fill in the danger tone",
    ),
    comp(
        "button.tone.danger.text",
        "color",
        "a button's label in the danger tone",
    ),
    comp(
        "button.tone.record.fill",
        "color",
        "a button's fill in the record tone",
    ),
    comp(
        "button.tone.record.text",
        "color",
        "a button's label in the record tone",
    ),
    comp(
        "button.tone.muted.fill",
        "color",
        "a quiet button's fill at rest",
    ),
];
