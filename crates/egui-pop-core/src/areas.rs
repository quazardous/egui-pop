//! The area registry: what each interactive area writes, declared to the
//! framework, which draws the affordance so a person tells at a glance
//! what only shows from what edits.
//!
//! <div class="warning">Experimental: may change in any release
//! (docs/stability.md).</div>
//!
//! An interactive area registers each pass it is drawn ([`register`],
//! after its widget) with its kind ([`AreaKind`]):
//!
//! - **display**: shows something, writes nothing (a scope's picture);
//! - **navigate**: moves what is shown without touching what is edited
//!   (pan and zoom, the selection, a scope's mode);
//! - **edit**: writes what the application edits (a parameter, the
//!   histogram's drag), with the control it writes ("Exposure"), its value
//!   now ("+0.30 EV") and, if the application has one, its path.
//!
//! For an edit area the framework draws, never the widget:
//!
//! - its **grips at rest** ([`Grip`]), small handles where a drag takes it
//!   (as the handles of a Levels tool): a graph with grips edits, a graph
//!   without is a picture;
//! - the **edit cursor** of its drag ([`AreaSpec::cursor`]);
//! - a **hover card** naming the control and its value, which is also the
//!   area's accessible description.
//!
//! An edit area is **inert** while what it shows is not current: the
//! application side draws a panel whose state is stale, updating or
//! failed inside [`scope`] with `inert`, and a widget asks [`inert`] before
//! it senses its drag (dragging the histogram of the previous photo would
//! edit with a wrong picture). An inert area shows no grip, no card and no
//! edit cursor, and is listed as inert.
//!
//! [`list`] gives the areas of each window's last pass, for agents and
//! tests (the application API carries them on its bus as `ui.areas`).
//!
//! ```
//! use egui_pop_core::areas::{self, AreaSpec, Grip};
//!
//! fn exposure(ui: &mut egui::Ui, ev: &mut f32) {
//!     let inert = areas::inert(ui);
//!     let sense = if inert { egui::Sense::hover() } else { egui::Sense::drag() };
//!     let (rect, r) = ui.allocate_exact_size(egui::vec2(160.0, 40.0), sense);
//!     *ev += r.drag_delta().x / rect.width() * 4.0;
//!     areas::register(
//!         ui,
//!         &r,
//!         AreaSpec::edit("demo.exposure", "Exposure")
//!             .value(format!("{ev:+.2} EV"))
//!             .grip(Grip::horizontal(rect.right_center())),
//!     );
//! }
//! ```

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use egui::{Context, Id, Pos2, Rect, Response, Stroke, StrokeKind, Ui, ViewportId, vec2};
use serde::{Deserialize, Serialize};

use crate::hover_card::{Card, HoverCard};
use crate::pointer::DragKind;
use crate::state::{lock, shared};
use crate::theme::Tokens;

/// What an interactive area writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum AreaKind {
    /// Shows, writes nothing.
    Display,
    /// Moves what is shown (pan, zoom, selection, a mode), never what is
    /// edited.
    Navigate,
    /// Writes what the application edits.
    Edit,
}

impl AreaKind {
    /// The kind as the registry lists it (`edit`).
    pub fn as_str(self) -> &'static str {
        match self {
            AreaKind::Display => "display",
            AreaKind::Navigate => "navigate",
            AreaKind::Edit => "edit",
        }
    }
}

/// Which way a grip's drag goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum GripAxis {
    /// Left and right: a tall handle.
    Horizontal,
    /// Up and down: a wide handle.
    Vertical,
    /// Any way: a round handle.
    Free,
}

/// A grip drawn at rest on an edit area: a small handle centred where a
/// drag takes it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Grip {
    /// Its centre, in points.
    pub at: Pos2,
    pub axis: GripAxis,
}

impl Grip {
    pub fn new(at: Pos2, axis: GripAxis) -> Grip {
        Grip { at, axis }
    }

    /// A grip dragged left and right.
    pub fn horizontal(at: Pos2) -> Grip {
        Grip::new(at, GripAxis::Horizontal)
    }

    /// A grip dragged up and down.
    pub fn vertical(at: Pos2) -> Grip {
        Grip::new(at, GripAxis::Vertical)
    }

    /// A grip dragged any way.
    pub fn free(at: Pos2) -> Grip {
        Grip::new(at, GripAxis::Free)
    }

    /// The handle's rectangle.
    pub fn rect(&self) -> Rect {
        let size = match self.axis {
            GripAxis::Horizontal => vec2(GRIP_THIN, GRIP_LONG),
            GripAxis::Vertical => vec2(GRIP_LONG, GRIP_THIN),
            GripAxis::Free => vec2(GRIP_THIN + 2.0, GRIP_THIN + 2.0),
        };
        Rect::from_center_size(self.at, size)
    }
}

/// A grip's short side, in points.
const GRIP_THIN: f32 = 7.0;
/// A grip's long side, in points.
const GRIP_LONG: f32 = 18.0;

/// An area declared to the registry ([`register`]).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct AreaSpec {
    pub id: String,
    pub kind: AreaKind,
    /// For an edit area: the control it writes, in the person's words.
    pub control: Option<String>,
    /// For an edit area: the control's value now, formatted.
    pub value: Option<String>,
    /// For an edit area: what it writes, as the application names it (a
    /// parameter's path).
    pub path: Option<String>,
    pub grips: Vec<Grip>,
    /// The cursor of its drag, shown by the framework.
    pub cursor: Option<DragKind>,
}

impl AreaSpec {
    pub fn new(id: impl Into<String>, kind: AreaKind) -> AreaSpec {
        AreaSpec {
            id: id.into(),
            kind,
            control: None,
            value: None,
            path: None,
            grips: Vec::new(),
            cursor: None,
        }
    }

    /// An area that only shows.
    pub fn display(id: impl Into<String>) -> AreaSpec {
        AreaSpec::new(id, AreaKind::Display)
    }

    /// An area that moves what is shown.
    pub fn navigate(id: impl Into<String>) -> AreaSpec {
        AreaSpec::new(id, AreaKind::Navigate)
    }

    /// An area that edits `control` ("Exposure").
    pub fn edit(id: impl Into<String>, control: impl Into<String>) -> AreaSpec {
        let mut a = AreaSpec::new(id, AreaKind::Edit);
        a.control = Some(control.into());
        a
    }

    /// The control's value now ("+0.30 EV").
    pub fn value(mut self, value: impl Into<String>) -> AreaSpec {
        self.value = Some(value.into());
        self
    }

    /// What it writes, as the application names it (`exposure`).
    pub fn path(mut self, path: impl Into<String>) -> AreaSpec {
        self.path = Some(path.into());
        self
    }

    /// A grip drawn at rest.
    pub fn grip(mut self, grip: Grip) -> AreaSpec {
        self.grips.push(grip);
        self
    }

    /// The cursor of its drag (the framework shows it while the area is
    /// hovered or dragged).
    pub fn cursor(mut self, kind: DragKind) -> AreaSpec {
        self.cursor = Some(kind);
        self
    }

    /// What its hover card says: the control and its value; empty for an
    /// area that does not edit.
    pub fn card(&self) -> Card {
        match (self.kind, &self.control) {
            (AreaKind::Edit, Some(control)) => {
                let text = match &self.value {
                    Some(v) => format!("**{control}** {v}"),
                    None => format!("**{control}**"),
                };
                Card::new().text(text)
            }
            _ => Card::new(),
        }
    }
}

/// An area as the registry lists it, from a window's last pass.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct AreaRecord {
    pub id: String,
    pub kind: AreaKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Where it is, in points.
    pub rect: Rect,
    /// The panel it is drawn in ([`scope`]), if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panel: Option<String>,
    /// Its window, as egui's debug text names its viewport (`ROOT`).
    pub window: String,
    /// It edits but is inert now (its panel is not current).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub inert: bool,
    /// The grips it shows now (none while inert).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub grips: Vec<Grip>,
}

/// A panel being drawn: its id, and whether its edit areas are inert.
#[derive(Clone, Debug)]
struct Scope {
    panel: String,
    inert: bool,
}

/// One window's areas.
#[derive(Debug, Default)]
struct Window {
    /// A pass draws now.
    drawing: bool,
    /// The areas of the pass drawing, or of the last one once it ended.
    current: Vec<AreaRecord>,
    /// The areas of the pass before the one drawing.
    last: Vec<AreaRecord>,
    /// The panels being drawn, innermost last.
    scopes: Vec<Scope>,
}

#[derive(Debug, Default)]
struct State {
    windows: HashMap<ViewportId, Window>,
}

fn state(ctx: &Context) -> Arc<Mutex<State>> {
    shared::<State>(ctx, "core.areas")
}

/// Turns each window's list over at the start of its pass.
struct Hook;

impl egui::Plugin for Hook {
    fn debug_name(&self) -> &'static str {
        "egui_pop_areas"
    }

    fn on_begin_pass(&mut self, ui: &mut Ui) {
        let viewport = ui.ctx().viewport_id();
        let st = state(ui.ctx());
        let mut s = lock(&st);
        let w = s.windows.entry(viewport).or_default();
        w.last = std::mem::take(&mut w.current);
        w.drawing = true;
        w.scopes.clear();
    }

    fn on_end_pass(&mut self, ui: &mut Ui) {
        let viewport = ui.ctx().viewport_id();
        let st = state(ui.ctx());
        if let Some(w) = lock(&st).windows.get_mut(&viewport) {
            w.drawing = false;
        }
    }
}

/// Draws `add` as the content of `panel`: the areas it registers name the
/// panel, and its edit areas are inert when `inert` (the panel's state is
/// not current).
pub fn scope<R>(ui: &mut Ui, panel: &str, inert: bool, add: impl FnOnce(&mut Ui) -> R) -> R {
    let ctx = ui.ctx().clone();
    let viewport = ctx.viewport_id();
    let st = state(&ctx);
    lock(&st)
        .windows
        .entry(viewport)
        .or_default()
        .scopes
        .push(Scope {
            panel: panel.to_owned(),
            inert,
        });
    // Popped even when `add` panics (a fault boundary inside catches it).
    struct Pop(Arc<Mutex<State>>, ViewportId);
    impl Drop for Pop {
        fn drop(&mut self) {
            if let Some(w) = lock(&self.0).windows.get_mut(&self.1) {
                w.scopes.pop();
            }
        }
    }
    let _pop = Pop(st, viewport);
    add(ui)
}

/// Whether an edit area drawn in `ui` now is inert: it senses no edit,
/// shows no grip. Ask it before sensing the drag.
pub fn inert(ui: &Ui) -> bool {
    let ctx = ui.ctx();
    let viewport = ctx.viewport_id();
    lock(&state(ctx))
        .windows
        .get(&viewport)
        .and_then(|w| w.scopes.last())
        .is_some_and(|s| s.inert)
}

/// Registers an area drawn this pass, after its widget (`response`), and
/// draws what the framework draws for it: an edit area's grips, its
/// cursor and its hover card, unless it is inert. Returns whether it is
/// inert.
pub fn register(ui: &Ui, response: &Response, area: AreaSpec) -> bool {
    let ctx = ui.ctx().clone();
    ctx.add_plugin(Hook);
    let viewport = ctx.viewport_id();
    let st = state(&ctx);
    let (panel, inert) = {
        let s = lock(&st);
        let scope = s.windows.get(&viewport).and_then(|w| w.scopes.last());
        (
            scope.map(|s| s.panel.clone()),
            area.kind == AreaKind::Edit && scope.is_some_and(|s| s.inert),
        )
    };
    let edits = area.kind == AreaKind::Edit && !inert;
    if edits {
        let active = response.hovered() || response.dragged() || response.has_focus();
        for g in &area.grips {
            paint_grip(ui, g, active);
        }
        if let Some(kind) = area.cursor {
            crate::pointer::drag_cursor(response, kind);
        }
        // Not while dragging: the drag's own Escape gives the gesture up.
        let card = if response.dragged() {
            Card::new()
        } else {
            area.card()
        };
        HoverCard::new(("egui_pop.area", response.id)).show(ui, response, &card);
    }
    let record = AreaRecord {
        id: area.id,
        kind: area.kind,
        control: area.control,
        value: area.value,
        path: area.path,
        rect: response.rect,
        panel,
        window: format!("{viewport:?}"),
        inert,
        grips: if edits { area.grips } else { Vec::new() },
    };
    lock(&st)
        .windows
        .entry(viewport)
        .or_default()
        .current
        .push(record);
    inert
}

/// Paints a grip, in the theme's colours: the raised surface, the strong
/// border (the accent while its area is hovered, dragged or focused) and
/// ridges in the muted text colour.
pub fn paint_grip(ui: &Ui, grip: &Grip, active: bool) {
    let t = Tokens::of(ui.ctx());
    let fill = t.surface.raised;
    let border = if active {
        t.accent.default
    } else {
        t.border.strong
    };
    let line = t.text.muted;
    let rect = grip.rect();
    let painter = ui.painter();
    let radius = rect.width().min(rect.height()) / 2.0;
    painter.rect_filled(rect, radius, fill);
    painter.rect_stroke(rect, radius, Stroke::new(1.0, border), StrokeKind::Inside);
    // Two short lines across its long side, as a handle's ridges.
    let ridge = Stroke::new(1.0, line);
    match grip.axis {
        GripAxis::Horizontal => {
            for dy in [-2.0, 2.0] {
                let y = rect.center().y + dy;
                painter.line_segment(
                    [
                        Pos2::new(rect.left() + 2.0, y),
                        Pos2::new(rect.right() - 2.0, y),
                    ],
                    ridge,
                );
            }
        }
        GripAxis::Vertical => {
            for dx in [-2.0, 2.0] {
                let x = rect.center().x + dx;
                painter.line_segment(
                    [
                        Pos2::new(x, rect.top() + 2.0),
                        Pos2::new(x, rect.bottom() - 2.0),
                    ],
                    ridge,
                );
            }
        }
        GripAxis::Free => {
            painter.circle_filled(rect.center(), 1.5, line);
        }
    }
}

/// The areas of each window's last pass (the pass drawing, once it ended;
/// the one before while it draws), the main window's first.
pub fn list(ctx: &Context) -> Vec<AreaRecord> {
    let st = state(ctx);
    let s = lock(&st);
    let mut windows: Vec<_> = s.windows.iter().collect();
    windows.sort_by_key(|(id, _)| (**id != ViewportId::ROOT, format!("{id:?}")));
    windows
        .into_iter()
        .flat_map(|(_, w)| if w.drawing { &w.last } else { &w.current })
        .cloned()
        .collect()
}

/// The area under `id` in the last pass, if registered.
pub fn find(ctx: &Context, id: &str) -> Option<AreaRecord> {
    list(ctx).into_iter().find(|a| a.id == id)
}

/// The hover card's id of the area drawn by `response`'s widget id, for
/// [`crate::hover_card::open`].
pub fn card_id(widget: Id) -> Id {
    HoverCard::new(("egui_pop.area", widget)).id()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_edit_card_names_the_control_and_its_value() {
        let a = AreaSpec::edit("h", "Exposure").value("+0.30 EV");
        assert_eq!(a.card().plain_text(), "Exposure +0.30 EV");
        assert!(AreaSpec::display("w").card().is_empty());
        assert_eq!(AreaKind::Navigate.as_str(), "navigate");
    }

    #[test]
    fn a_grip_is_tall_when_dragged_sideways() {
        let g = Grip::horizontal(Pos2::new(10.0, 10.0));
        assert!(g.rect().height() > g.rect().width());
        assert_eq!(g.rect().center(), Pos2::new(10.0, 10.0));
    }
}
