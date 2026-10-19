//! The contract of the default kit's `spinner` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp("spinner.shape", "string", "the handler drawing a spinner"),
    comp("spinner.width", "dimension", "a spinner's diameter"),
    comp("spinner.rest.fill", "color", "a spinner's track"),
    comp("spinner.rest.indicator-color", "color", "a spinner's arc"),
    comp("spinner.rest.text", "color", "a spinner's label"),
    comp(
        "spinner.tone.accent.indicator-color",
        "color",
        "a spinner's arc in the accent tone",
    ),
    comp(
        "spinner.tone.info.indicator-color",
        "color",
        "a spinner's arc in the info tone",
    ),
    comp(
        "spinner.tone.success.indicator-color",
        "color",
        "a spinner's arc in the success tone",
    ),
    comp(
        "spinner.tone.warning.indicator-color",
        "color",
        "a spinner's arc in the warning tone",
    ),
    comp(
        "spinner.tone.danger.indicator-color",
        "color",
        "a spinner's arc in the danger tone",
    ),
    comp(
        "spinner.tone.muted.indicator-color",
        "color",
        "a spinner's arc in the muted tone",
    ),
    comp(
        "spinner.tone.record.indicator-color",
        "color",
        "a spinner's arc in the record tone",
    ),
];
