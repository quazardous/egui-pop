//! The contract of the default kit's drag cursors (`drag-cursor.<kind>`).

use super::super::{Required, comp};

pub(crate) const CONTRACT: &[Required] = &[
    comp(
        "drag-cursor.pan.hover",
        "string",
        "a `pan` drag: the cursor hovered",
    ),
    comp(
        "drag-cursor.pan.drag",
        "string",
        "a `pan` drag: the cursor while dragged",
    ),
    comp(
        "drag-cursor.pan.policy",
        "string",
        "a `pan` drag: when the hover cursor shows",
    ),
    comp(
        "drag-cursor.scrub.hover",
        "string",
        "a `scrub` drag: the cursor hovered",
    ),
    comp(
        "drag-cursor.scrub.drag",
        "string",
        "a `scrub` drag: the cursor while dragged",
    ),
    comp(
        "drag-cursor.scrub.policy",
        "string",
        "a `scrub` drag: when the hover cursor shows",
    ),
    comp(
        "drag-cursor.move.hover",
        "string",
        "a `move` drag: the cursor hovered",
    ),
    comp(
        "drag-cursor.move.drag",
        "string",
        "a `move` drag: the cursor while dragged",
    ),
    comp(
        "drag-cursor.move.policy",
        "string",
        "a `move` drag: when the hover cursor shows",
    ),
    comp(
        "drag-cursor.carry.hover",
        "string",
        "a `carry` drag: the cursor hovered",
    ),
    comp(
        "drag-cursor.carry.drag",
        "string",
        "a `carry` drag: the cursor while dragged",
    ),
    comp(
        "drag-cursor.carry.policy",
        "string",
        "a `carry` drag: when the hover cursor shows",
    ),
    comp(
        "drag-cursor.value.hover",
        "string",
        "a `value` drag: the cursor hovered",
    ),
    comp(
        "drag-cursor.value.drag",
        "string",
        "a `value` drag: the cursor while dragged",
    ),
    comp(
        "drag-cursor.value.policy",
        "string",
        "a `value` drag: when the hover cursor shows",
    ),
    comp(
        "drag-cursor.point.hover",
        "string",
        "a `point` drag: the cursor hovered",
    ),
    comp(
        "drag-cursor.point.drag",
        "string",
        "a `point` drag: the cursor while dragged",
    ),
    comp(
        "drag-cursor.point.policy",
        "string",
        "a `point` drag: when the hover cursor shows",
    ),
    comp(
        "drag-cursor.select.hover",
        "string",
        "a `select` drag: the cursor hovered",
    ),
    comp(
        "drag-cursor.select.drag",
        "string",
        "a `select` drag: the cursor while dragged",
    ),
    comp(
        "drag-cursor.select.policy",
        "string",
        "a `select` drag: when the hover cursor shows",
    ),
    comp(
        "drag-cursor.text.hover",
        "string",
        "a `text` drag: the cursor hovered",
    ),
    comp(
        "drag-cursor.text.drag",
        "string",
        "a `text` drag: the cursor while dragged",
    ),
    comp(
        "drag-cursor.text.policy",
        "string",
        "a `text` drag: when the hover cursor shows",
    ),
    comp(
        "drag-cursor.resize-horizontal.hover",
        "string",
        "a `resize-horizontal` drag: the cursor hovered",
    ),
    comp(
        "drag-cursor.resize-horizontal.drag",
        "string",
        "a `resize-horizontal` drag: the cursor while dragged",
    ),
    comp(
        "drag-cursor.resize-horizontal.policy",
        "string",
        "a `resize-horizontal` drag: when the hover cursor shows",
    ),
    comp(
        "drag-cursor.resize-vertical.hover",
        "string",
        "a `resize-vertical` drag: the cursor hovered",
    ),
    comp(
        "drag-cursor.resize-vertical.drag",
        "string",
        "a `resize-vertical` drag: the cursor while dragged",
    ),
    comp(
        "drag-cursor.resize-vertical.policy",
        "string",
        "a `resize-vertical` drag: when the hover cursor shows",
    ),
    comp(
        "drag-cursor.resize-nw-se.hover",
        "string",
        "a `resize-nw-se` drag: the cursor hovered",
    ),
    comp(
        "drag-cursor.resize-nw-se.drag",
        "string",
        "a `resize-nw-se` drag: the cursor while dragged",
    ),
    comp(
        "drag-cursor.resize-nw-se.policy",
        "string",
        "a `resize-nw-se` drag: when the hover cursor shows",
    ),
    comp(
        "drag-cursor.resize-ne-sw.hover",
        "string",
        "a `resize-ne-sw` drag: the cursor hovered",
    ),
    comp(
        "drag-cursor.resize-ne-sw.drag",
        "string",
        "a `resize-ne-sw` drag: the cursor while dragged",
    ),
    comp(
        "drag-cursor.resize-ne-sw.policy",
        "string",
        "a `resize-ne-sw` drag: when the hover cursor shows",
    ),
    comp(
        "drag-cursor.split-columns.hover",
        "string",
        "a `split-columns` drag: the cursor hovered",
    ),
    comp(
        "drag-cursor.split-columns.drag",
        "string",
        "a `split-columns` drag: the cursor while dragged",
    ),
    comp(
        "drag-cursor.split-columns.policy",
        "string",
        "a `split-columns` drag: when the hover cursor shows",
    ),
    comp(
        "drag-cursor.split-rows.hover",
        "string",
        "a `split-rows` drag: the cursor hovered",
    ),
    comp(
        "drag-cursor.split-rows.drag",
        "string",
        "a `split-rows` drag: the cursor while dragged",
    ),
    comp(
        "drag-cursor.split-rows.policy",
        "string",
        "a `split-rows` drag: when the hover cursor shows",
    ),
];
