//! The contract of the default kit's `scroll-bar` component.

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "scroll-bar.shape",
        "string",
        "the handler drawing a scroll bar (egui draws it: `scoped`)",
    ),
    comp(
        "scroll-bar.width",
        "dimension",
        "a scroll bar's width at its widest",
    ),
    comp(
        "scroll-bar.width-rest",
        "dimension",
        "a scroll bar's width at rest, under `visibility: rest`",
    ),
    comp(
        "scroll-bar.opacity-rest",
        "number",
        "how opaque a scroll bar's thumb is at rest, 0 to 1, under `visibility: rest`",
    ),
    comp(
        "scroll-bar.room",
        "dimension",
        "the content's width kept for a scroll bar while it shows, under `visibility: rest`",
    ),
    comp(
        "scroll-bar.min-length",
        "dimension",
        "a scroll bar's thumb at its shortest",
    ),
    comp(
        "scroll-bar.margin",
        "dimension",
        "between a scroll bar and the edge of its scroll area",
    ),
    comp(
        "scroll-bar.placement",
        "string",
        "`over` the content (floating) or `beside` it (taking room)",
    ),
    comp(
        "scroll-bar.visibility",
        "string",
        "when a scroll bar shows: `always`, at `rest` too but thin and quiet, on `hover`, or while it `scroll`s",
    ),
    comp(
        "scroll-bar.track-fill",
        "color",
        "the track behind the thumb (transparent: none)",
    ),
    comp(
        "scroll-bar.rest.fill",
        "color",
        "a scroll bar's thumb at rest",
    ),
    comp(
        "scroll-bar.rest.radius",
        "dimension",
        "the corners of a scroll bar's thumb and track (half the width or more: fully rounded)",
    ),
    comp(
        "scroll-bar.hover.fill",
        "color",
        "a scroll bar's thumb under the pointer",
    ),
    comp(
        "scroll-bar.active.fill",
        "color",
        "a scroll bar's thumb while dragged",
    ),
];
