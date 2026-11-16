//! A tab drag seen from the windows it did not begin in.
//!
//! egui keeps each window's input apart, and the drag lives in the window
//! it began in (the main one). While the button is held, the system gives
//! every pointer event to that window (Wayland's and X11's implicit grab,
//! the capture on Windows), whose pointer keeps following the drag past
//! its edges. The other windows learn of the drag from its session: the
//! dragged panel, the source window's pointer, whether it left the source
//! window, and the source's place on the screen when the system tells it.
//!
//! A session carries a tab or a whole tab group (all its tabs, dragged
//! by the empty part of its tab bar, its shown tab kept). This is the
//! panels' case of a drag across windows; a drag of any payload across
//! windows can take the same shape.

use egui::{Pos2, Vec2};
use egui_pop_model::ids::PanelId;

/// What a drag in the main window carries: one tab, or a whole tab
/// group.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Dragged {
    /// One tab.
    Tab(PanelId),
    /// A tab group, all its tabs in order and the one it shows.
    Group {
        panels: Vec<PanelId>,
        shown: PanelId,
    },
}

impl Dragged {
    /// The panel that names the drag: the tab, or the group's shown one.
    pub(crate) fn lead(&self) -> &PanelId {
        match self {
            Dragged::Tab(p) => p,
            Dragged::Group { shown, .. } => shown,
        }
    }

    /// The panels it moves, in order.
    pub(crate) fn panels(&self) -> &[PanelId] {
        match self {
            Dragged::Tab(p) => std::slice::from_ref(p),
            Dragged::Group { panels, .. } => panels,
        }
    }

    /// Whether it moves a whole group.
    pub(crate) fn is_group(&self) -> bool {
        matches!(self, Dragged::Group { .. })
    }

    /// Whether it moves `panel`.
    pub(crate) fn holds(&self, panel: &PanelId) -> bool {
        self.panels().contains(panel)
    }
}

/// A tab or a tab group being dragged in the main window, as the other
/// windows see it in one pass of the main window.
#[derive(Clone, Debug)]
pub(crate) struct DragSession {
    /// What is dragged.
    pub(crate) dragged: Dragged,
    /// The source window's pointer, in its points.
    at: Pos2,
    /// The pointer left the source window.
    outside: bool,
    /// The source window's top-left on the screen, in pixels, and its
    /// scale; `None` where the system hides window positions (Wayland).
    origin: Option<(Vec2, f32)>,
}

impl DragSession {
    /// The session of a tab or a group dragged in the main window
    /// (`ctx`), with the main window's pointer `at`.
    pub(crate) fn of(ctx: &egui::Context, dragged: Dragged, at: Pos2) -> Self {
        let outside = !ctx.input(|i| i.viewport_rect()).contains(at);
        let origin = ctx.input(|i| i.viewport().inner_rect).map(|r| {
            (
                r.min.to_vec2() * ctx.pixels_per_point(),
                ctx.pixels_per_point(),
            )
        });
        Self {
            dragged,
            at,
            outside,
            origin,
        }
    }

    /// Where the pointer is over another window (`vctx`), in that
    /// window's points, if it can be known.
    ///
    /// A window embedded in the main one (kittest, a backend without
    /// windows of its own) sees the main one's pointer. An OS window of
    /// its own sees the pointer only once the drag lets it go; until then
    /// the source's pointer is carried over the screen, when both windows
    /// know where they are (not on Wayland).
    pub(crate) fn pointer_in(&self, vctx: &egui::Context, own_window: bool) -> Option<Pos2> {
        if let Some(p) = vctx.input(|i| i.pointer.hover_pos()) {
            return Some(p);
        }
        if !own_window {
            return Some(self.at);
        }
        let (origin, ppp) = self.origin?;
        let inner = vctx.input(|i| i.viewport().inner_rect)?;
        let screen = origin + self.at.to_vec2() * ppp;
        Some((screen / vctx.pixels_per_point() - inner.min.to_vec2()).to_pos2())
    }

    /// What another window can show of the drag: a drop zone where the
    /// pointer is known to be over its `body`, an outline where it cannot
    /// be known (the pointer left the source window, the system hides
    /// where windows are), nothing otherwise.
    pub(crate) fn over(&self, vctx: &egui::Context, own_window: bool, body: egui::Rect) -> Over {
        match self.pointer_in(vctx, own_window) {
            Some(p) if body.contains(p) => Over::At(p),
            Some(_) => Over::Away,
            None if self.outside => Over::Unknown,
            None => Over::Away,
        }
    }
}

/// Where a dragged tab or group is for a window it did not begin in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Over {
    /// Over its body, at this point.
    At(Pos2),
    /// Out of the source window, the window cannot tell whether over it.
    Unknown,
    /// Not over it (or still in the source window).
    Away,
}
