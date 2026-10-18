//! The contract of the default kit's glyph roles.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "glyph.disclosure-open",
        "string",
        "the icon of the `disclosure-open` glyph",
    ),
    comp(
        "glyph.disclosure-closed",
        "string",
        "the icon of the `disclosure-closed` glyph",
    ),
    comp(
        "glyph.sort-ascending",
        "string",
        "the icon of the `sort-ascending` glyph",
    ),
    comp(
        "glyph.sort-descending",
        "string",
        "the icon of the `sort-descending` glyph",
    ),
    comp("glyph.check", "string", "the icon of the `check` glyph"),
    comp("glyph.radio", "string", "the icon of the `radio` glyph"),
    comp(
        "glyph.indeterminate",
        "string",
        "the icon of the `indeterminate` glyph",
    ),
    comp("glyph.bullet", "string", "the icon of the `bullet` glyph"),
    comp("glyph.submenu", "string", "the icon of the `submenu` glyph"),
    comp(
        "glyph.chevron-start",
        "string",
        "the icon of the `chevron-start` glyph",
    ),
    comp(
        "glyph.chevron-end",
        "string",
        "the icon of the `chevron-end` glyph",
    ),
    comp("glyph.close", "string", "the icon of the `close` glyph"),
    comp("glyph.more", "string", "the icon of the `more` glyph"),
    comp("glyph.expand", "string", "the icon of the `expand` glyph"),
    comp(
        "glyph.collapse",
        "string",
        "the icon of the `collapse` glyph",
    ),
];
