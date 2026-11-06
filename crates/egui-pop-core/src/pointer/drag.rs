//! Drag cursors: the cursor a draggable shows, hovered and while it is
//! dragged, keyed by what the drag means ([`DragKind`]) rather than by
//! widget, and changed by the pointer profile in force
//! ([`PointerMap::drag_cursor`](egui_pop_model::pointer::PointerMap::drag_cursor)).
//!
//! egui resets the cursor every pass, and the pointer leaves a widget
//! while it drags it: a cursor set only when the widget is hovered, or
//! only where the pointer is over it, comes back to the arrow mid-drag.
//! [`drag_cursor`] asks the drag's cursor every pass the response is
//! dragged, wherever the pointer is, and the hover cursor only while
//! nothing else is dragged.
//!
//! ```
//! use egui_pop_core::pointer::{self, DragKind};
//!
//! let ctx = egui::Context::default();
//! let mut out = ctx.run_ui(Default::default(), |ui| {
//!     let (_, response) = ui.allocate_exact_size(egui::vec2(100.0, 4.0), egui::Sense::drag());
//!     pointer::drag_cursor(&response, DragKind::ResizeVertical);
//! });
//! out.textures_delta.clear();
//! assert_eq!(
//!     pointer::drag_cursor_icons(&ctx, DragKind::Pan),
//!     (egui::CursorIcon::Grab, egui::CursorIcon::Grabbing)
//! );
//! ```

use egui::{Context, CursorIcon, Response};

pub use egui_pop_model::pointer::{CURSOR_NAMES, DragCursor, DragKind};

/// egui's cursor of a CSS cursor name ([`CURSOR_NAMES`]).
pub fn cursor_icon(name: &str) -> Option<CursorIcon> {
    Some(match name {
        "alias" => CursorIcon::Alias,
        "all-scroll" => CursorIcon::AllScroll,
        "cell" => CursorIcon::Cell,
        "context-menu" => CursorIcon::ContextMenu,
        "copy" => CursorIcon::Copy,
        "crosshair" => CursorIcon::Crosshair,
        "default" => CursorIcon::Default,
        "grab" => CursorIcon::Grab,
        "grabbing" => CursorIcon::Grabbing,
        "help" => CursorIcon::Help,
        "move" => CursorIcon::Move,
        "no-drop" => CursorIcon::NoDrop,
        "none" => CursorIcon::None,
        "not-allowed" => CursorIcon::NotAllowed,
        "pointer" => CursorIcon::PointingHand,
        "progress" => CursorIcon::Progress,
        "ew-resize" => CursorIcon::ResizeHorizontal,
        "nesw-resize" => CursorIcon::ResizeNeSw,
        "nwse-resize" => CursorIcon::ResizeNwSe,
        "ns-resize" => CursorIcon::ResizeVertical,
        "e-resize" => CursorIcon::ResizeEast,
        "se-resize" => CursorIcon::ResizeSouthEast,
        "s-resize" => CursorIcon::ResizeSouth,
        "sw-resize" => CursorIcon::ResizeSouthWest,
        "w-resize" => CursorIcon::ResizeWest,
        "nw-resize" => CursorIcon::ResizeNorthWest,
        "n-resize" => CursorIcon::ResizeNorth,
        "ne-resize" => CursorIcon::ResizeNorthEast,
        "col-resize" => CursorIcon::ResizeColumn,
        "row-resize" => CursorIcon::ResizeRow,
        "text" => CursorIcon::Text,
        "vertical-text" => CursorIcon::VerticalText,
        "wait" => CursorIcon::Wait,
        "zoom-in" => CursorIcon::ZoomIn,
        "zoom-out" => CursorIcon::ZoomOut,
        _ => return None,
    })
}

/// A drag kind's cursors in a context, hovered and dragged: the pointer
/// map's ([`super::map`]).
pub fn drag_cursor_icons(ctx: &Context, kind: DragKind) -> (CursorIcon, CursorIcon) {
    let c = super::map(ctx).drag_cursor(kind);
    (
        cursor_icon(&c.hover).unwrap_or_default(),
        cursor_icon(&c.drag).unwrap_or_default(),
    )
}

/// Shows the cursor of a drag of `kind` for a response: its drag cursor
/// every pass it is dragged, wherever the pointer is; its hover cursor
/// while it is hovered and nothing is dragged. Called each pass the
/// widget is drawn, after anything else that sets the cursor for it.
pub fn drag_cursor(response: &Response, kind: DragKind) {
    let ctx = &response.ctx;
    if response.dragged() {
        hold_drag_cursor(ctx, kind);
    } else if response.hovered() && ctx.dragged_id().is_none() {
        hover_drag_cursor(ctx, kind);
    }
}

/// Shows the drag cursor of `kind` this pass: for a drag a widget
/// follows itself (a state of its own rather than one response).
pub fn hold_drag_cursor(ctx: &Context, kind: DragKind) {
    ctx.set_cursor_icon(drag_cursor_icons(ctx, kind).1);
}

/// Shows the hover cursor of `kind` this pass, unless it is the arrow
/// (which leaves the cursor to whatever asked for one).
pub fn hover_drag_cursor(ctx: &Context, kind: DragKind) {
    let hover = drag_cursor_icons(ctx, kind).0;
    if hover != CursorIcon::Default {
        ctx.set_cursor_icon(hover);
    }
}
