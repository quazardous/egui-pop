//! The layout changed, on the bus (ADR 0029, "Panels and layout").
//!
//! The workbench's layout (its areas, tabs and splits, detached and
//! floating panels, closed and folded areas) changes in a few ways: a
//! workbench step (a panel docked, detached, floated, moved, shown or
//! hidden, an area folded, a flyout resized), a layout put in
//! ([`AppHandle::set_layout`]) or repaired against panels declared again,
//! a gesture in a dock (a tab moved among its tabs, a split resized), a
//! palette or a detached window moved or resized. Each of them notes the
//! change ([`AppHandle::note_layout`]); none says it.
//!
//! The main window's pass says it, **once**, as `pop.layout.changed`
//! ([`LayoutChanged`], a [`LayoutChange`] naming its causes), in
//! [`AppHandle::settle_layout`], the one place that does:
//!
//! - not while a gesture holds (a button down in the main window, a tab
//!   or a palette dragged, a palette resized): a drag in progress says
//!   nothing until it is let go;
//! - a window's or a palette's place, which follows the system's moves
//!   with no button egui sees, once it stayed still for [`SETTLE`];
//! - only when the layout differs from the one said, loaded or saved
//!   last (a step that changed nothing is not a change).
//!
//! A button let go or a key pressed in any window is noted too: what a
//! dock changed by itself is read back from it then, and said if it
//! differs; nothing compares the layout on a pass where nobody did
//! anything.
//!
//! egui-pop's listener, local: [`LAYOUT_SAVE`] saves the layout to the
//! document its host names, through the `layout.save` step. An
//! application keeping a copy of the layout elsewhere listens after it.

use bus_contract::{Point, PointKind};
use egui::Context;
use serde::Serialize;

use super::emit_with;
use crate::app::AppHandle;

/// What changed the layout: the workbench step's id (`workbench.move`,
/// `workbench.detach`, ...), `layout.set` (a layout put in),
/// `layout.repair` (repaired against the panels declared),
/// `pop.window.place` (a detached window moved or resized),
/// `palette.place` (a palette following the window's edges), `input`
/// (a button let go or a key pressed: what a dock changed).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[non_exhaustive]
pub struct LayoutChange {
    /// Its causes since the last time, each once, in the order noted.
    pub causes: Vec<String>,
}

/// The layout changed (`pop.layout.changed`), said once per real change,
/// once the gesture that made it ended.
#[derive(Debug)]
pub struct LayoutChanged;
impl Point for LayoutChanged {
    const KIND: PointKind = PointKind::Event;
    const ID: &'static str = "pop.layout.changed";
    type Input = LayoutChange;
    type Value = ();
}

/// The layout points, in the order [`super::order_dump`] prints them.
pub const LAYOUT_POINTS: [&str; 1] = [LayoutChanged::ID];

/// egui-pop's listener saving the layout to the host's document (the
/// `layout.save` step). An application's copy of the layout runs after
/// it.
pub const LAYOUT_SAVE: &str = "pop.layout.save";

/// How long a place followed from the system (a window, a palette) stays
/// still before its change is said, in seconds.
pub const SETTLE: f64 = 0.3;

/// Changes noted and not said yet.
#[derive(Debug, Default)]
pub(crate) struct Unsaid {
    causes: Vec<String>,
    /// When a place followed from the system last changed: said
    /// [`SETTLE`] after.
    moving: Option<f64>,
}

/// Causes that follow the system's moves, with no button egui sees.
fn follows_the_system(cause: &str) -> bool {
    matches!(cause, "window.place" | "palette.place")
}

impl AppHandle {
    /// Notes that `cause` may have changed the layout: the main window's
    /// next pass says it, once, if it did ([`AppHandle::settle_layout`]).
    pub(crate) fn note_layout(&self, cause: &str) {
        let ctx = self.egui();
        let now = egui_pop_core::motion::now(&ctx);
        {
            let mut wb = self.workbench_mut();
            let unsaid = wb.unsaid.get_or_insert_with(Unsaid::default);
            if !unsaid.causes.iter().any(|c| c == cause) {
                unsaid.causes.push(cause.to_string());
            }
            if follows_the_system(cause) {
                unsaid.moving = Some(now);
            }
        }
        ctx.request_repaint();
    }

    /// The layout as it stands is the one on disk (loaded or saved): no
    /// change noted until now is said.
    pub(crate) fn layout_said_now(&self) {
        let now = self.saved_layout();
        let mut wb = self.workbench_mut();
        wb.said = Some(now);
        wb.unsaid = None;
    }

    /// Says `pop.layout.changed` once for the changes noted, when no gesture
    /// holds and the layout differs from the one said last: its listeners
    /// run now, on this thread. Then notes a button let go or a key
    /// pressed in the main window, said at the next pass, once the pass
    /// that follows it drew what it changed. Called once per main
    /// window's pass.
    pub(crate) fn settle_layout(&self, ctx: &Context) {
        let first = self.workbench_mut().said.is_none();
        if first {
            // What the application started with is not a change.
            self.layout_said_now();
            return;
        }
        self.say_layout(ctx);
        if ctx.input(|i| {
            i.pointer.any_released()
                || i.events
                    .iter()
                    .any(|e| matches!(e, egui::Event::Key { pressed: true, .. }))
        }) {
            self.note_layout("input");
        }
    }

    fn say_layout(&self, ctx: &Context) {
        let held = ctx.input(|i| i.pointer.any_down()) || {
            let wb = self.workbench_mut();
            wb.dragging.is_some() || wb.palette_drag.is_some() || wb.palette_resize.is_some()
        };
        let now = egui_pop_core::motion::now(ctx);
        let causes = {
            let mut wb = self.workbench_mut();
            let Some(unsaid) = wb.unsaid.as_ref() else {
                return;
            };
            if held {
                return;
            }
            if let Some(at) = unsaid.moving
                && now < at + SETTLE
            {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(
                    (at + SETTLE - now).max(0.05),
                ));
                return;
            }
            wb.unsaid.take().map(|u| u.causes).unwrap_or_default()
        };
        let layout = self.saved_layout();
        {
            let mut wb = self.workbench_mut();
            if wb.said.as_ref() == Some(&layout) {
                return;
            }
            wb.said = Some(layout);
        }
        emit_with::<LayoutChanged>(self, LayoutChange { causes }, ());
    }
}
