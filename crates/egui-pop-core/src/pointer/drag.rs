//! Drag cursors: the cursor a draggable shows, hovered and while it is
//! dragged, keyed by what the drag means ([`DragKind`]) rather than by
//! widget.
//!
//! egui resets the cursor every pass, and the pointer leaves a widget
//! while it drags it: a cursor set only when the widget is hovered, or
//! only where the pointer is over it, comes back to the arrow mid-drag.
//! [`drag_cursor`] asks the drag's cursor every pass the response is
//! dragged, wherever the pointer is, and the hover cursor only while
//! nothing else is dragged.
//!
//! **The hover rule.** By default a draggable shows its hover cursor
//! only when dragging is its only action ([`HoverPolicy::OnlyAction`]):
//! a splitter, a resize grip, an item's handle. Where a click or a
//! double click does something (select, set a value, move the playhead),
//! the arrow stays until the drag starts. Whether a response clicks is
//! read from its sense, unless the widget says ([`DragCursorFor::clicks`]).
//!
//! **Three levels**, the most specific winning: the widget
//! ([`DragCursorFor::hover`], [`DragCursorFor::dragging`],
//! [`DragCursorFor::hover_policy`]), the pointer profile
//! ([`PointerMap::drag_cursor_over`](egui_pop_model::pointer::PointerMap::drag_cursor_over)),
//! the kit (`drag-cursor.<kind>.hover`, `.drag`, `.policy`:
//! [`DragCursorKit`](crate::kit::DragCursorKit)), over the kind's own
//! ([`DragKind::cursors`]).
//!
//! ```
//! use egui_pop_core::pointer::{self, DragKind, HoverPolicy};
//!
//! let ctx = egui::Context::default();
//! let mut out = ctx.run_ui(Default::default(), |ui| {
//!     let (_, splitter) = ui.allocate_exact_size(egui::vec2(4.0, 100.0), egui::Sense::drag());
//!     pointer::drag_cursor(&splitter, DragKind::SplitColumns);
//!     // A click selects it: the arrow until it is dragged, unless asked.
//!     let (_, clip) = ui.allocate_exact_size(egui::vec2(100.0, 20.0), egui::Sense::click_and_drag());
//!     pointer::drag_cursor(&clip, DragKind::Move).hover_policy(HoverPolicy::Always);
//! });
//! out.textures_delta.clear();
//! assert_eq!(
//!     pointer::drag_cursor_icons(&ctx, DragKind::Pan),
//!     (egui::CursorIcon::Grab, egui::CursorIcon::Grabbing)
//! );
//! ```

use egui::{Context, CursorIcon, Response};

pub use egui_pop_model::pointer::{CURSOR_NAMES, DragCursor, DragKind, HoverPolicy};

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

/// A drag kind's cursors in a context, with its hover policy: the
/// kit's ([`DragCursorKit`](crate::kit::DragCursorKit)), under the
/// pointer map's ([`super::map`]).
pub fn resolve_drag_cursor(ctx: &Context, kind: DragKind) -> DragCursor {
    let kit = crate::kit::Kit::of(ctx).drag_cursors.get(kind);
    super::map(ctx).drag_cursor_over(kind, kit)
}

/// A drag kind's cursors in a context, hovered and dragged: the kit's
/// under the pointer map's ([`resolve_drag_cursor`]).
pub fn drag_cursor_icons(ctx: &Context, kind: DragKind) -> (CursorIcon, CursorIcon) {
    let c = resolve_drag_cursor(ctx, kind);
    (
        cursor_icon(&c.hover).unwrap_or_default(),
        cursor_icon(&c.drag).unwrap_or_default(),
    )
}

/// Shows the cursor of a drag of `kind` for a response: its drag cursor
/// every pass it is dragged, wherever the pointer is; its hover cursor
/// while it is hovered and nothing is dragged, as the hover policy says
/// ([`HoverPolicy`]). Called each pass the widget is drawn, after
/// anything else that sets the cursor for it.
///
/// It returns what to show, applied when dropped (at the end of the
/// statement): the widget may change it first.
///
/// ```
/// # use egui_pop_core::pointer::{self, DragKind, HoverPolicy};
/// # let ctx = egui::Context::default();
/// # let mut out = ctx.run_ui(Default::default(), |ui| {
/// # let (_, response) = ui.allocate_exact_size(egui::vec2(100.0, 20.0), egui::Sense::click_and_drag());
/// // The response senses clicks only to take the focus: dragging is
/// // its only action.
/// pointer::drag_cursor(&response, DragKind::SplitRows).clicks(false);
/// # });
/// # out.textures_delta.clear();
/// ```
pub fn drag_cursor(response: &Response, kind: DragKind) -> DragCursorFor<'_> {
    DragCursorFor {
        response,
        kind,
        clicks: None,
        hover: None,
        dragging: None,
        policy: None,
    }
}

/// The cursor a draggable asks for ([`drag_cursor`]), shown when it is
/// dropped. Its settings are the most specific level: they win over the
/// pointer profile's and the kit's.
#[derive(Debug)]
pub struct DragCursorFor<'a> {
    response: &'a Response,
    kind: DragKind,
    clicks: Option<bool>,
    hover: Option<CursorIcon>,
    dragging: Option<CursorIcon>,
    policy: Option<HoverPolicy>,
}

impl DragCursorFor<'_> {
    /// Whether a click or a double click on it does something (by
    /// default, whether the response senses clicks): with
    /// [`HoverPolicy::OnlyAction`], the hover cursor shows only when it
    /// does not.
    pub fn clicks(mut self, clicks: bool) -> Self {
        self.clicks = Some(clicks);
        self
    }

    /// The cursor shown hovered, when the policy shows one.
    pub fn hover(mut self, cursor: CursorIcon) -> Self {
        self.hover = Some(cursor);
        self
    }

    /// The cursor shown while it is dragged.
    pub fn dragging(mut self, cursor: CursorIcon) -> Self {
        self.dragging = Some(cursor);
        self
    }

    /// When the hover cursor shows.
    pub fn hover_policy(mut self, policy: HoverPolicy) -> Self {
        self.policy = Some(policy);
        self
    }
}

impl Drop for DragCursorFor<'_> {
    fn drop(&mut self) {
        let r = self.response;
        let ctx = &r.ctx;
        if r.dragged() {
            let icon = self
                .dragging
                .unwrap_or_else(|| drag_cursor_icons(ctx, self.kind).1);
            ctx.set_cursor_icon(icon);
        } else if r.hovered() && ctx.dragged_id().is_none() {
            let c = resolve_drag_cursor(ctx, self.kind);
            let policy = self.policy.unwrap_or_else(|| c.hover_policy());
            let clicks = self.clicks.unwrap_or_else(|| r.sense.senses_click());
            if policy.shows(clicks) {
                let icon = self
                    .hover
                    .unwrap_or_else(|| cursor_icon(&c.hover).unwrap_or_default());
                if icon != CursorIcon::Default {
                    ctx.set_cursor_icon(icon);
                }
            }
        }
    }
}

/// Shows the drag cursor of `kind` this pass: for a drag a widget
/// follows itself (a state of its own rather than one response).
pub fn hold_drag_cursor(ctx: &Context, kind: DragKind) {
    ctx.set_cursor_icon(drag_cursor_icons(ctx, kind).1);
}

/// Shows the hover cursor of `kind` this pass over what a widget hovers
/// itself, if the hover policy in force shows it for an element that
/// does something on a click (`clicks`) or not, and unless it is the
/// arrow (which leaves the cursor to whatever asked for one).
pub fn hover_drag_cursor(ctx: &Context, kind: DragKind, clicks: bool) {
    let c = resolve_drag_cursor(ctx, kind);
    if !c.hover_policy().shows(clicks) {
        return;
    }
    let hover = cursor_icon(&c.hover).unwrap_or_default();
    if hover != CursorIcon::Default {
        ctx.set_cursor_icon(hover);
    }
}
