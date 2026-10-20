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
    comp(
        "glyph.drag-handle",
        "string",
        "the icon of the `drag-handle` glyph",
    ),
    comp(
        "glyph.dropdown",
        "string",
        "the icon of the `dropdown` glyph",
    ),
    comp("glyph.expand", "string", "the icon of the `expand` glyph"),
    comp(
        "glyph.collapse",
        "string",
        "the icon of the `collapse` glyph",
    ),
    comp(
        "glyph.window-minimize",
        "string",
        "the icon of the `window-minimize` glyph",
    ),
    comp(
        "glyph.window-maximize",
        "string",
        "the icon of the `window-maximize` glyph",
    ),
    comp(
        "glyph.window-restore",
        "string",
        "the icon of the `window-restore` glyph",
    ),
    comp(
        "glyph.window-close",
        "string",
        "the icon of the `window-close` glyph",
    ),
    comp(
        "glyph.window-exit-fullscreen",
        "string",
        "the icon of the `window-exit-fullscreen` glyph",
    ),
    comp("glyph.led-off", "string", "the icon of the `led-off` glyph"),
    comp("glyph.led-on", "string", "the icon of the `led-on` glyph"),
    comp(
        "glyph.led-busy",
        "string",
        "the icon of the `led-busy` glyph",
    ),
    comp(
        "glyph.led-warning",
        "string",
        "the icon of the `led-warning` glyph",
    ),
    comp(
        "glyph.led-error",
        "string",
        "the icon of the `led-error` glyph",
    ),
    comp(
        "glyph.flag-modified",
        "string",
        "the icon of the `flag-modified` glyph",
    ),
    comp(
        "glyph.flag-pinned",
        "string",
        "the icon of the `flag-pinned` glyph",
    ),
    comp(
        "glyph.flag-required",
        "string",
        "the icon of the `flag-required` glyph",
    ),
    comp(
        "glyph.flag-invalid",
        "string",
        "the icon of the `flag-invalid` glyph",
    ),
];
