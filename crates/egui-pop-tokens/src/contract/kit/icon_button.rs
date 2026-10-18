//! The contract of the default kit's `icon-button` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "icon-button.shape",
        "string",
        "the handler drawing a button made of an icon",
    ),
    comp("icon-button.rest.fill", "color", "an icon button's box"),
    comp(
        "icon-button.rest.border-color",
        "color",
        "an icon button's border",
    ),
    comp(
        "icon-button.rest.border-width",
        "dimension",
        "an icon button's border width (0: none)",
    ),
    comp(
        "icon-button.rest.radius",
        "dimension",
        "an icon button's corner radius",
    ),
    comp("icon-button.rest.text", "color", "an icon button's icon"),
    comp(
        "icon-button.tone.accent.text",
        "color",
        "an icon button's icon in the accent tone",
    ),
    comp(
        "icon-button.tone.info.text",
        "color",
        "an icon button's icon in the info tone",
    ),
    comp(
        "icon-button.tone.success.text",
        "color",
        "an icon button's icon in the success tone",
    ),
    comp(
        "icon-button.tone.warning.text",
        "color",
        "an icon button's icon in the warning tone",
    ),
    comp(
        "icon-button.tone.danger.text",
        "color",
        "an icon button's icon in the danger tone",
    ),
    comp(
        "icon-button.tone.muted.text",
        "color",
        "an icon button's icon in the muted tone",
    ),
];
