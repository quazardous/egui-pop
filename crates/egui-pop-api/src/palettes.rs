//! Palettes: toolboxes, colour palettes, navigators floating over the
//! interface (not the command palette, [`crate::palette`]).
//!
//! A palette is a panel ([`PanelSpec::palette`]) placed floating instead
//! of docked: one model, two placements. It opens floating, is dragged by
//! its header band ([`egui_pop_core::band`], the movable dialogs' band),
//! clamped inside the workbench, snapping to the workbench's edges and to
//! the other palettes within the `palette.magnet` token. Dropped on an
//! edge of the workbench, it is anchored there ([`anchor_of`]) and keeps
//! its distance to it as the window is resized ([`anchored_pos`]); dragged
//! away, it no longer is. Dropped on a
//! side or bottom area (or a closed one's edge, with the workbench's drop
//! hint), it docks there; a docked floatable panel floats again from its
//! tab menu (Float). Its band carries Close, Roll up (down to the band)
//! and Pin (kept in place, in sight and opaque); a double click on the
//! band rolls it up or down.
//!
//! **Never modal.** A palette never blocks the interface: anything that
//! must be answered first is a dialog ([`crate::dialogs`]). It takes the
//! keyboard only when clicked into; Escape there gives it back to the
//! panel that had it.
//!
//! **Behaviour.** The `palettes.toggle` command (Tab, rebindable) hides
//! and shows the unpinned palettes. Tab is the toggle only while some
//! panel floats, no widget has the keyboard and no overlay is open:
//! otherwise it moves the keyboard to the next widget, as always. While a stroke is in progress
//! ([`set_stroke`]) the palettes fade when the `palettes.fade_on_stroke`
//! setting is on; `palettes.idle_opacity` makes them translucent while
//! the pointer is elsewhere. `palettes.reset` puts them back where they
//! open first, rolled down and sized by their content.
//!
//! **Groups.** Palettes float in groups ([`FloatingGroup`]), a palette
//! alone being a group of one. A palette dropped on another one's top or
//! bottom quarter ([`join_at`], with the drop hint the tabs use) stacks
//! above or below it, in a column where each keeps its own band and
//! roll-up. The group's band (over its members') drags, pins and anchors
//! the whole group, and docks every member over an area, as tabs
//! (floating one of them again gives the group back); a member's own
//! band drags it out of the group. Dropped on the middle of another
//! palette, a palette joins its group as a tab: the group's band then
//! holds the tabs, and the shown member its own band.
//!
//! **Sizes.** A palette is sized by its content until the user resizes
//! it: its right edge, its bottom edge and the corner between them drag
//! its size (the `palette.grip` token's width), kept between the
//! `palette.min-width` and `palette.max-width` tokens (and its panels'
//! own least width), `palette.min-height` and `palette.max-height`, and
//! inside the workbench. The size is the group's
//! ([`FloatingGroup::size`], saved in the layout): a group keeps it as
//! palettes join it, a palette coming out of it is sized by its content
//! again. Its content scrolls when it is taller than the room left; a
//! rolled-up palette keeps its width and only its band's height. A
//! double click on an edge gives the content its size back. Resized
//! against an edge of the workbench, a palette is anchored there as when
//! it is dropped.
//!
//! **Steps** (ADR 0011): `palette.open` (`{panel}`), `palette.float`
//! (`{panel, pos}`), `palette.dock` (`{panel, area}`, `group: true` for
//! the whole group), `palette.move` (`{panel, pos}`, once a drag is
//! dropped), `palette.resize` (`{panel, size}`, `size` being
//! `[width, height]` or `null` for the content's, once an edge is
//! dropped), `palette.roll` (`{panel, rolled}`), `palette.pin`
//! (`{panel, pinned}`), `palette.group` (`{panels, into, at}`) and
//! `palette.ungroup` (`{panel, pos}`); `pos` is the top-left corner in
//! points from the workbench's top-left. Each done change is published
//! as `palette.opened`, `palette.floated`, `palette.docked`,
//! `palette.moved`, `palette.resized`, `palette.rolled`, `palette.pinned`,
//! `palette.grouped` or `palette.ungrouped`.
//!
//! Not built yet: a palette outside the main window (a native utility
//! window through egui's viewports).
//!
//! [`PanelSpec::palette`]: egui_pop_model::panels::PanelSpec::palette

use std::collections::BTreeMap;

use egui::{Id, Key, Modifiers, Pos2, Rect, Ui, Vec2};
use egui_pop_core::band::{self, BandStyle};
use egui_pop_core::icon::Icon;
use egui_pop_core::kit::{Glyph, Kit, KitCx, Part, State};
use egui_pop_core::lock;
use egui_pop_core::strings;
use egui_pop_core::theme::Tokens;
use egui_pop_core::widgets;
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::ids::{CommandId, PanelId, SettingId, StepId};
use egui_pop_model::keymap::BindOptions;
use egui_pop_model::layout::{Anchor, AnchorEdge, Arrangement, FloatingGroup, Join, Member};
use egui_pop_model::values::Descriptor;
use serde_json::{Value as Json, json};

use crate::app::AppHandle;
use crate::steps::{Outcome, StepSpec};
use crate::strings as txt;
use crate::workbench::Change;
use crate::{Ctx, CtxError};

/// A palette opens (`{panel}`).
pub const STEP_OPEN: StepId = StepId::from_static("palette.open");
/// A panel floats over the interface (`{panel, pos}`).
pub const STEP_FLOAT: StepId = StepId::from_static("palette.float");
/// A floating panel docks in an area (`{panel, area}`).
pub const STEP_DOCK: StepId = StepId::from_static("palette.dock");
/// A floating panel is moved: dropped after a drag, reset
/// (`{panel, pos}`).
pub const STEP_MOVE: StepId = StepId::from_static("palette.move");
/// A floating panel's group is resized, once an edge is dropped, or
/// sized by its content again (`{panel, size}`, `size` being
/// `[width, height]` in points or `null`).
pub const STEP_RESIZE: StepId = StepId::from_static("palette.resize");
/// A floating panel rolls up or down (`{panel, rolled}`).
pub const STEP_ROLL: StepId = StepId::from_static("palette.roll");
/// A floating panel is pinned or unpinned (`{panel, pinned}`).
pub const STEP_PIN: StepId = StepId::from_static("palette.pin");
/// Floating panels join another one's group (`{panels, into, at}`, `at`
/// being `above`, `below` or `tab`).
pub const STEP_GROUP: StepId = StepId::from_static("palette.group");
/// A floating panel comes out of its group, alone (`{panel, pos}`).
pub const STEP_UNGROUP: StepId = StepId::from_static("palette.ungroup");

/// Hides or shows the unpinned palettes (Tab).
pub const CMD_TOGGLE: CommandId = CommandId::from_static("palettes.toggle");
/// Puts the palettes back where they first open.
pub const CMD_RESET: CommandId = CommandId::from_static("palettes.reset");

/// The setting: palettes fade while a stroke is in progress.
pub const SETTING_FADE: SettingId = SettingId::from_static("palettes.fade_on_stroke");
/// The setting: how opaque a palette is while the pointer is elsewhere.
pub const SETTING_IDLE_OPACITY: SettingId = SettingId::from_static("palettes.idle_opacity");

/// The context key saying a stroke is in progress ([`set_stroke`]).
pub const CONTEXT_STROKE: &str = "strokeActive";
/// The context key saying the palettes are hidden (`palettes.toggle`).
pub const CONTEXT_HIDDEN: &str = "palettesHidden";
/// The context key saying some panel floats (shown or hidden): only
/// then is Tab the palettes' toggle.
pub const CONTEXT_FLOATING: &str = "palettesFloat";

/// What is left of a palette while a stroke is in progress.
const STROKE_OPACITY: f32 = 0.15;

/// Says a stroke (a brush, a drag on the canvas) starts or ends: the
/// palettes fade meanwhile when the `palettes.fade_on_stroke` setting is
/// on.
pub fn set_stroke(app: &AppHandle, on: bool) {
    app.set_context(CONTEXT_STROKE, if on { "true" } else { "false" });
    app.egui().request_repaint();
}

/// Whether the unpinned palettes are hidden (`palettes.toggle`).
pub fn hidden(app: &AppHandle) -> bool {
    app.context().get(CONTEXT_HIDDEN) == "true"
}

/// Hides or shows the unpinned palettes.
pub fn set_hidden(app: &AppHandle, hidden: bool) {
    app.set_context(CONTEXT_HIDDEN, if hidden { "true" } else { "false" });
    app.publish(
        "egui_pop",
        None,
        "palette.toggled",
        json!({ "hidden": hidden }),
    );
    app.egui().request_repaint();
}

/// The palettes' steps, settings and commands, as the framework declares
/// them.
pub(crate) fn declare(ctx: &mut Ctx<'_>) -> Result<(), CtxError> {
    let panel = json!({ "panel": "app.tools" });
    for (step, about, args) in [
        (
            STEP_OPEN,
            "A palette opens, floating (`panel`).",
            panel.clone(),
        ),
        (
            STEP_FLOAT,
            "A panel floats over the interface (`panel`, `pos`).",
            json!({ "panel": "app.tools", "pos": [16.0, 16.0] }),
        ),
        (
            STEP_DOCK,
            "A floating panel docks in an area (`panel`, `area`); with `group`, every member of its group does, as tabs.",
            json!({ "panel": "app.tools", "area": "start_side" }),
        ),
        (
            STEP_GROUP,
            "Floating panels join another one's group, above it, below it or as a tab (`panels`, `into`, `at`).",
            json!({ "panels": ["app.colours"], "into": "app.tools", "at": "below" }),
        ),
        (
            STEP_UNGROUP,
            "A floating panel comes out of its group, alone at a place (`panel`, `pos`).",
            json!({ "panel": "app.colours", "pos": [16.0, 300.0] }),
        ),
        (
            STEP_MOVE,
            "A floating panel is moved, once dropped (`panel`, `pos`).",
            json!({ "panel": "app.tools", "pos": [120.0, 40.0] }),
        ),
        (
            STEP_RESIZE,
            "A floating panel's group is resized, once an edge is dropped (`panel`, `size` as `[width, height]`, or `null` for its content's size).",
            json!({ "panel": "app.tools", "size": [240.0, 320.0] }),
        ),
        (
            STEP_ROLL,
            "A floating panel rolls up to its band, or down (`panel`, `rolled`).",
            json!({ "panel": "app.tools", "rolled": true }),
        ),
        (
            STEP_PIN,
            "A floating panel is pinned in place, or unpinned (`panel`, `pinned`).",
            json!({ "panel": "app.tools", "pinned": true }),
        ),
    ] {
        ctx.step(StepSpec::new(step, about).args(args))?;
    }
    ctx.setting(
        SETTING_FADE,
        Descriptor::bool("Fade palettes while drawing", false)
            .description("Palettes fade while a stroke is in progress on the canvas."),
    )?;
    ctx.setting(
        SETTING_IDLE_OPACITY,
        Descriptor::float("Palette opacity away from the pointer", 0.2, 1.0, 1.0)
            .description(
                "How opaque a palette is while the pointer is elsewhere; 1 keeps it opaque.",
            )
            .step(0.05),
    )?;
    ctx.command(
        CommandSpec::new(CMD_TOGGLE, "Hide or show palettes", "View")
            .description("Hides the palettes that are not pinned, or shows them again.")
            .when("palettesFloat && !widgetFocus && !overlayOpen")
            .key_with("Tab", BindOptions::global()),
        |call| {
            // Tab is this command here, not a move to the next widget.
            call.app
                .egui()
                .memory_mut(|m| m.move_focus(egui::FocusDirection::None));
            let now = !hidden(call.app);
            set_hidden(call.app, now);
            Ok(json!({ "hidden": now }))
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_RESET, "Reset palettes", "View")
            .description("Puts the floating palettes back where they first open, rolled down and sized by their content."),
        |call| {
            reset(call.app);
            Ok(Json::Null)
        },
    )?;
    Ok(())
}

/// Puts the floating palettes back where they first open, rolled down
/// and sized by their content,
/// each through its steps.
pub fn reset(app: &AppHandle) {
    let floating: Vec<FloatingGroup> = app.workbench_mut().layout.floating.clone();
    for (n, g) in floating.iter().enumerate() {
        app.change_workbench(Change::MovePalette(
            g.lead().clone(),
            FloatingGroup::cascade(n),
        ));
        for m in g.members.iter().filter(|m| m.rolled) {
            app.change_workbench(Change::Roll(m.panel.clone(), false));
        }
        if g.size.is_some() {
            app.change_workbench(Change::ResizePalette(g.lead().clone(), None));
        }
    }
    if hidden(app) {
        set_hidden(app, false);
    }
}

/// A rectangle snapped to the bounds' edges and to the others' edges
/// (side by side or aligned) when within `magnet` of them; the nearest
/// edge wins on each axis.
///
/// ```
/// use egui::{Rect, pos2, vec2};
/// use egui_pop_api::palettes::snap;
///
/// let bounds = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0));
/// let r = Rect::from_min_size(pos2(7.0, 300.0), vec2(100.0, 50.0));
/// assert_eq!(snap(r, bounds, &[], 12.0).min, pos2(0.0, 300.0));
/// let other = Rect::from_min_size(pos2(200.0, 290.0), vec2(100.0, 80.0));
/// let r = Rect::from_min_size(pos2(95.0, 300.0), vec2(100.0, 50.0));
/// assert_eq!(snap(r, bounds, &[other], 12.0).max.x, 200.0);
/// ```
pub fn snap(rect: Rect, bounds: Rect, others: &[Rect], magnet: f32) -> Rect {
    fn best(cur: &mut Option<f32>, d: f32, magnet: f32) {
        if d.abs() <= magnet && cur.is_none_or(|c| d.abs() < c.abs()) {
            *cur = Some(d);
        }
    }
    let (mut dx, mut dy) = (None, None);
    best(&mut dx, bounds.left() - rect.left(), magnet);
    best(&mut dx, bounds.right() - rect.right(), magnet);
    best(&mut dy, bounds.top() - rect.top(), magnet);
    best(&mut dy, bounds.bottom() - rect.bottom(), magnet);
    for o in others {
        let beside = rect.top() < o.bottom() + magnet && o.top() < rect.bottom() + magnet;
        let above = rect.left() < o.right() + magnet && o.left() < rect.right() + magnet;
        if beside {
            for d in [
                o.left() - rect.right(),
                o.right() - rect.left(),
                o.left() - rect.left(),
                o.right() - rect.right(),
            ] {
                best(&mut dx, d, magnet);
            }
        }
        if above {
            for d in [
                o.top() - rect.bottom(),
                o.bottom() - rect.top(),
                o.top() - rect.top(),
                o.bottom() - rect.bottom(),
            ] {
                best(&mut dy, d, magnet);
            }
        }
    }
    rect.translate(egui::vec2(dx.unwrap_or(0.0), dy.unwrap_or(0.0)))
}

/// How near a workbench edge a palette's side lies to be on it, in
/// points: snapped there or clamped against it.
const ON_EDGE: f32 = 0.5;

/// The workbench edges a palette's `rect` lies on, within half a point:
/// its anchor. A side off the edges (dragged away, or snapped to another
/// palette) is not anchored; on both edges of an axis (as large as the
/// workbench), the start wins.
///
/// ```
/// use egui::{Rect, pos2, vec2};
/// use egui_pop_api::palettes::anchor_of;
/// use egui_pop_model::layout::AnchorEdge;
///
/// let bounds = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0));
/// let r = Rect::from_min_size(pos2(700.0, 200.0), vec2(100.0, 50.0));
/// let a = anchor_of(r, bounds);
/// assert_eq!((a.x, a.y), (AnchorEdge::End, AnchorEdge::None));
/// ```
pub fn anchor_of(rect: Rect, bounds: Rect) -> Anchor {
    fn axis(lo: f32, hi: f32, start: f32, end: f32) -> (AnchorEdge, f32) {
        let (before, after) = (lo - start, end - hi);
        if before.abs() <= ON_EDGE {
            (AnchorEdge::Start, before.max(0.0))
        } else if after.abs() <= ON_EDGE {
            (AnchorEdge::End, after.max(0.0))
        } else {
            (AnchorEdge::None, 0.0)
        }
    }
    let (x, ox) = axis(rect.left(), rect.right(), bounds.left(), bounds.right());
    let (y, oy) = axis(rect.top(), rect.bottom(), bounds.top(), bounds.bottom());
    Anchor {
        x,
        y,
        offset: [ox, oy],
    }
}

/// Where a palette's top-left corner goes in `bounds`: an anchored axis
/// keeps its distance to its edge, the other keeps `pos` (from the
/// bounds' top-left); kept inside either way.
///
/// ```
/// use egui::{Rect, pos2, vec2};
/// use egui_pop_api::palettes::anchored_pos;
/// use egui_pop_model::layout::{Anchor, AnchorEdge};
///
/// let right = Anchor { x: AnchorEdge::End, ..Anchor::default() };
/// let wider = Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, 600.0));
/// let at = anchored_pos([700.0, 200.0], vec2(100.0, 50.0), wider, &right);
/// assert_eq!(at, pos2(900.0, 200.0));
/// ```
pub fn anchored_pos(pos: [f32; 2], size: Vec2, bounds: Rect, anchor: &Anchor) -> Pos2 {
    let axis = |edge: AnchorEdge, at: f32, offset: f32, start: f32, end: f32, len: f32| match edge {
        AnchorEdge::None => start + at,
        AnchorEdge::Start => start + offset,
        AnchorEdge::End => end - offset - len,
    };
    let x = axis(
        anchor.x,
        pos[0],
        anchor.offset[0],
        bounds.left(),
        bounds.right(),
        size.x,
    );
    let y = axis(
        anchor.y,
        pos[1],
        anchor.offset[1],
        bounds.top(),
        bounds.bottom(),
        size.y,
    );
    clamp(Pos2::new(x, y), size, bounds)
}

/// A top-left corner keeping a box of `size` inside `bounds` (at its
/// top-left when it is larger).
fn clamp(min: Pos2, size: Vec2, bounds: Rect) -> Pos2 {
    let max = (bounds.max - size).max(bounds.min);
    min.clamp(bounds.min, max)
}

fn setting_bool(app: &AppHandle, id: SettingId) -> bool {
    lock(&app.store())
        .get(&format!("settings/{id}"))
        .and_then(|e| e.value.as_bool())
        .unwrap_or(false)
}

fn setting_f32(app: &AppHandle, id: SettingId) -> Option<f32> {
    lock(&app.store())
        .get(&format!("settings/{id}"))
        .and_then(|e| e.value.as_f64())
        .map(|v| v as f32)
}

/// A palette's area id.
pub fn area_id(panel: &PanelId) -> Id {
    Id::new(("egui_pop.palette", panel.as_str()))
}

/// A palette's band being dragged.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PaletteDrag {
    /// The palette whose band is dragged: its group's first member when
    /// the group's band is.
    pub panel: PanelId,
    /// The group's band (the whole group moves), or a member's own band
    /// (it comes out of its group).
    pub whole: bool,
    /// Where the dragged palettes were when the drag started, from the
    /// workbench's top-left.
    pub start: [f32; 2],
    /// Where they are now (snapped, kept inside).
    pub pos: [f32; 2],
    /// The pointer.
    pub at: Pos2,
}

/// A palette's edge being dragged.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PaletteResize {
    /// Its group's first member.
    pub panel: PanelId,
    /// The group's size before the drag (`None`: its content's).
    pub saved: Option<[f32; 2]>,
    /// The size drawn when the drag started.
    pub from: [f32; 2],
    /// The edges dragged: the right one, the bottom one.
    pub edges: [bool; 2],
}

/// A palette's size once its right edge (`edges[0]`) and bottom edge
/// (`edges[1]`) are dragged by `delta` from `start`, kept between `least`
/// and `most`.
///
/// ```
/// use egui::vec2;
/// use egui_pop_api::palettes::resize_by;
///
/// let (least, most) = (vec2(120.0, 64.0), vec2(640.0, 900.0));
/// let start = vec2(200.0, 300.0);
/// assert_eq!(resize_by(start, vec2(40.0, 25.0), [true, false], least, most), vec2(240.0, 300.0));
/// assert_eq!(resize_by(start, vec2(40.0, 25.0), [true, true], least, most), vec2(240.0, 325.0));
/// assert_eq!(resize_by(start, vec2(-500.0, 0.0), [true, true], least, most), vec2(120.0, 300.0));
/// ```
pub fn resize_by(start: Vec2, delta: Vec2, edges: [bool; 2], least: Vec2, most: Vec2) -> Vec2 {
    let w = if edges[0] { start.x + delta.x } else { start.x };
    let h = if edges[1] { start.y + delta.y } else { start.y };
    Vec2::new(w, h).clamp(least, most.max(least))
}

/// A palette dimension token (`palette.grip`...), `fallback` when the
/// theme has none.
fn palette_dimension(ctx: &egui::Context, path: &str, fallback: f32) -> f32 {
    Tokens::of(ctx)
        .resolved()
        .dimension(path)
        .unwrap_or(fallback)
}

/// The least and the largest size of a group drawn at `pos`: the
/// `palette.min-*` and `palette.max-*` tokens, at least `least_width`
/// wide (its panels' least width and the frame), and inside the
/// workbench.
fn size_limits(ctx: &egui::Context, least_width: f32, pos: Pos2, content: Rect) -> (Vec2, Vec2) {
    let least = Vec2::new(
        palette_dimension(ctx, "palette.min-width", 120.0).max(least_width),
        palette_dimension(ctx, "palette.min-height", 64.0),
    );
    let most = Vec2::new(
        palette_dimension(ctx, "palette.max-width", 640.0),
        palette_dimension(ctx, "palette.max-height", 900.0),
    )
    .min(content.max - pos)
    .max(least);
    (least, most)
}

/// Draws `add` in a vertical scroll area `height` high, as wide as the
/// room: a resized palette's content. Its band drags the palette, so the
/// area scrolls by its bar and the wheel, not by a drag.
fn scrolled(ui: &mut Ui, id: Id, height: f32, add: impl FnOnce(&mut Ui)) {
    use egui::scroll_area::ScrollSource;
    egui::ScrollArea::vertical()
        .id_salt(id.with("scroll"))
        .scroll_source(ScrollSource::SCROLL_BAR | ScrollSource::MOUSE_WHEEL)
        .auto_shrink([false, false])
        .max_height(height)
        .min_scrolled_height(height)
        .show(ui, add);
}

/// A palette's resize edges, over its frame drawn at `rect`, `grip`
/// wide: the right edge, and when it is not rolled up (`vertical`) the
/// bottom edge and the corner between them; each with the edges it
/// drags.
fn resize_edges(
    ui: &mut Ui,
    id: Id,
    rect: Rect,
    grip: f32,
    vertical: bool,
) -> Vec<(egui::Response, [bool; 2])> {
    let half = grip.max(2.0) / 2.0;
    let sense = egui::Sense::CLICK | egui::Sense::DRAG;
    let right = Rect::from_x_y_ranges(
        rect.right() - half..=rect.right() + half,
        rect.top()..=rect.bottom(),
    );
    let mut out = vec![(
        ui.interact(right, id.with("resize.right"), sense),
        [true, false],
    )];
    if vertical {
        let bottom = Rect::from_x_y_ranges(
            rect.left()..=rect.right(),
            rect.bottom() - half..=rect.bottom() + half,
        );
        let corner = Rect::from_center_size(rect.right_bottom(), Vec2::splat(4.0 * half));
        out.push((
            ui.interact(bottom, id.with("resize.bottom"), sense),
            [false, true],
        ));
        out.push((
            ui.interact(corner, id.with("resize.corner"), sense),
            [true, true],
        ));
    }
    for (r, edges) in &out {
        if r.hovered() || r.dragged() {
            ui.ctx().set_cursor_icon(match edges {
                [true, true] => egui::CursorIcon::ResizeNwSe,
                [true, false] => egui::CursorIcon::ResizeHorizontal,
                _ => egui::CursorIcon::ResizeVertical,
            });
        }
    }
    out
}

/// Follows a palette's resize edges: a drag resizes its group live
/// (between `limits`), the drop resizes it through `palette.resize`
/// (anchoring it again); a double click gives it its content's size back.
fn follow_resize(
    app: &AppHandle,
    ctx: &egui::Context,
    g: &FloatingGroup,
    rect: Rect,
    edges: &[(egui::Response, [bool; 2])],
    (least, most): (Vec2, Vec2),
) {
    let lead = g.lead().clone();
    if edges.iter().any(|(r, _)| r.double_clicked()) {
        if g.size.is_some() {
            run(app, &lead, Some(Change::ResizePalette(lead.clone(), None)));
            ctx.request_repaint();
        }
        return;
    }
    let mine = app
        .workbench_mut()
        .palette_resize
        .clone()
        .filter(|r| r.panel == lead);
    if let Some((_, e)) = edges.iter().find(|(r, _)| r.dragged()) {
        let (origin, now) = ctx.input(|i| (i.pointer.press_origin(), i.pointer.latest_pos()));
        let (Some(origin), Some(now)) = (origin, now) else {
            return;
        };
        let from = mine
            .as_ref()
            .map_or([rect.width(), rect.height()], |r| r.from);
        let size = resize_by(Vec2::from(from), now - origin, *e, least, most);
        let mut wb = app.workbench_mut();
        let mut changed = false;
        if let Some(fl) = wb.layout.floating_mut(&lead) {
            changed = fl.size != Some([size.x, size.y]);
            fl.size = Some([size.x, size.y]);
        }
        if mine.is_none() {
            wb.palette_resize = Some(PaletteResize {
                panel: lead,
                saved: g.size,
                from,
                edges: *e,
            });
        }
        if changed {
            ctx.request_repaint();
        }
        return;
    }
    let Some(r) = mine else {
        return;
    };
    // Dropped: back to the size it had, then the step decides.
    let to = {
        let mut wb = app.workbench_mut();
        wb.palette_resize = None;
        wb.layout
            .floating_mut(&lead)
            .and_then(|fl| std::mem::replace(&mut fl.size, r.saved))
    };
    if to.is_some() && to != r.saved {
        run(app, &lead, Some(Change::ResizePalette(lead.clone(), to)));
    }
    ctx.request_repaint();
}

/// Where dropping the dragged palettes would group them, while they are
/// dragged over another palette: what [`group_hint`] reports.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupHint {
    /// The palettes dropped: a whole group, or one member of a group.
    pub panels: Vec<PanelId>,
    /// The palette they would join.
    pub into: PanelId,
    /// Above it, below it, or as a tab.
    pub at: Join,
    /// Where the hint is drawn, in points.
    pub rect: Rect,
}

/// Where dropping the palettes dragged now would group them, if they
/// are dragged over another palette.
pub fn group_hint(app: &AppHandle) -> Option<GroupHint> {
    app.workbench_mut().group_hint.clone()
}

/// Where a palette dropped with the pointer at `at`, over another
/// palette drawn at `rect`, joins its group: on its top quarter above
/// it, on its bottom quarter below it, in between as a tab; outside it,
/// none.
///
/// ```
/// use egui::{Rect, pos2, vec2};
/// use egui_pop_api::palettes::join_at;
/// use egui_pop_model::layout::Join;
///
/// let r = Rect::from_min_size(pos2(100.0, 100.0), vec2(200.0, 200.0));
/// assert_eq!(join_at(r, pos2(150.0, 110.0)), Some(Join::Above));
/// assert_eq!(join_at(r, pos2(150.0, 290.0)), Some(Join::Below));
/// assert_eq!(join_at(r, pos2(150.0, 200.0)), Some(Join::Tab));
/// assert_eq!(join_at(r, pos2(50.0, 110.0)), None);
/// ```
pub fn join_at(rect: Rect, at: Pos2) -> Option<Join> {
    if !rect.contains(at) {
        return None;
    }
    let edge = rect.height() / 4.0;
    if at.y < rect.top() + edge {
        Some(Join::Above)
    } else if at.y > rect.bottom() - edge {
        Some(Join::Below)
    } else {
        Some(Join::Tab)
    }
}

/// Where a group hint is drawn over the palette joined: its top half,
/// its bottom half, or all of it for a tab.
fn join_rect(rect: Rect, at: Join) -> Rect {
    match at {
        Join::Above => Rect::from_min_max(rect.min, egui::pos2(rect.max.x, rect.center().y)),
        Join::Below => Rect::from_min_max(egui::pos2(rect.min.x, rect.center().y), rect.max),
        Join::Tab => rect,
    }
}

/// The palette the pointer at `at` drops the dragged ones on, and where,
/// skipping the panels `skip` says (the dragged ones).
fn join_target(
    members: &BTreeMap<PanelId, Rect>,
    at: Pos2,
    skip: impl Fn(&PanelId) -> bool,
) -> Option<(PanelId, Join, Rect)> {
    members
        .iter()
        .filter(|(p, _)| !skip(p))
        .find_map(|(p, r)| join_at(*r, at).map(|j| (p.clone(), j, join_rect(*r, j))))
}

/// What a palette's band buttons asked for.
#[derive(Default)]
struct Asked {
    close: Option<PanelId>,
    roll: Option<PanelId>,
    pin: bool,
    /// The tab to show, in a group of tabs.
    show: Option<usize>,
}

/// A member's band as laid out, with where the member is drawn
/// vertically (its band's top to its content's bottom).
struct MemberBand {
    panel: PanelId,
    band: band::Band,
    top: f32,
    bottom: f32,
}

/// Draws the floating palettes over the workbench, each group as one
/// unit, and follows their drags: a group's band moves the group, a
/// member's band takes it out of its group; dropped over another palette
/// they join its group, over an area they dock there, elsewhere they
/// move through `palette.move` or `palette.ungroup`.
pub(crate) fn show(app: &AppHandle, ctx: &egui::Context) {
    let (floating, content, before, members_before) = {
        let mut wb = app.workbench_mut();
        if wb.palette_drag.is_none() {
            wb.group_hint = None;
        }
        (
            wb.layout.floating.clone(),
            wb.content,
            wb.palette_rects.clone(),
            wb.member_rects.clone(),
        )
    };
    let any = if floating.is_empty() { "false" } else { "true" };
    if app.context().get(CONTEXT_FLOATING) != any {
        app.set_context(CONTEXT_FLOATING, any);
    }
    if floating.is_empty() || !content.is_positive() {
        let mut wb = app.workbench_mut();
        wb.palette_rects.clear();
        wb.member_rects.clear();
        return;
    }
    let t = Tokens::of(ctx);
    let hide = hidden(app);
    let stroke = app.context().get(CONTEXT_STROKE) == "true" && setting_bool(app, SETTING_FADE);
    let idle = setting_f32(app, SETTING_IDLE_OPACITY)
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    let gap = group_gap(ctx);
    let pointer = ctx.input(|i| i.pointer.latest_pos());
    let mut rects = BTreeMap::new();
    let mut members = BTreeMap::new();
    for g in floating {
        if hide && !g.pinned {
            continue;
        }
        let specs: Vec<_> = g
            .panels()
            .filter_map(|p| app.workbench_mut().spec(p).cloned())
            .collect();
        if specs.len() != g.members.len() {
            continue;
        }
        let lead = g.lead().clone();
        let id = area_id(&lead);
        let drawn = before.get(&lead).map(|r| r.size());
        let size = drawn.unwrap_or(Vec2::ZERO);
        let size = g.size.map_or(size, Vec2::from);
        let dragging = app
            .workbench_mut()
            .palette_drag
            .as_ref()
            .is_some_and(|d| d.whole && d.panel == lead);
        let resizing = app
            .workbench_mut()
            .palette_resize
            .as_ref()
            .is_some_and(|r| r.panel == lead);
        // Inside the workbench, again when the window is resized; an
        // anchored group keeps its distance to its edges (not while it
        // is dragged or resized: the drop sets its anchor again).
        let pos = if dragging || resizing {
            clamp(content.min + Vec2::from(g.pos), size, content)
        } else {
            anchored_pos(g.pos, size, content, &g.anchor)
        };
        if !g.anchor.is_none() && !dragging && !resizing && drawn.is_some() {
            // Its place follows the edges (not a move: no step).
            let at = [pos.x - content.min.x, pos.y - content.min.y];
            if at != g.pos
                && let Some(fl) = app.workbench_mut().layout.floating_mut(&lead)
            {
                fl.pos = at;
            }
        }
        let hovered = before
            .get(&lead)
            .zip(pointer)
            .is_some_and(|(r, p)| r.contains(p));
        let target = if g.pinned || dragging {
            1.0
        } else if stroke {
            STROKE_OPACITY
        } else if hovered {
            1.0
        } else {
            idle
        };
        let opacity = ctx.animate_value_with_time(id.with("opacity"), target, t.motion_duration);
        let titles: Vec<String> = g.panels().map(|p| app.panel_title(ctx, p)).collect();
        let min = specs.iter().map(|s| s.min_size).fold(0.0, f32::max);
        let max = specs
            .iter()
            .map(|s| s.default_size.unwrap_or(320.0).max(s.min_size))
            .fold(0.0, f32::max);
        // Its size once resized, between its limits.
        let margin = Kit::of(ctx)
            .palette_window
            .frame(State::REST, ctx.pixels_per_point())
            .total_margin()
            .sum();
        let (least, most) = size_limits(ctx, min + margin.x, pos, content);
        let fixed = g.size.map(|s| Vec2::from(s).clamp(least, most));
        let inner = fixed.map(|s| (s - margin).max(Vec2::ZERO));
        // Rolled up, it keeps its width and only its bands' height.
        let unrolled = g
            .members
            .iter()
            .any(|m| !m.rolled && (g.arrangement == Arrangement::Stack || m.panel == *g.shown()));
        let mut asked = Asked::default();
        let mut moved: Option<band::Band> = None;
        let mut bands: Vec<MemberBand> = Vec::new();
        let mut edges: Vec<(egui::Response, [bool; 2])> = Vec::new();
        let shown = egui::Area::new(id)
            .order(egui::Order::Middle)
            .fixed_pos(pos)
            .constrain(false)
            .show(ctx, |ui| {
                ui.multiply_opacity(opacity);
                let kit = Kit::of(ui.ctx());
                let window = &kit.palette_window;
                let style = BandStyle {
                    padding: window.data.padding_x,
                    gap: kit.header_band.data.padding_y,
                };
                let shadow = ui.visuals().window_shadow;
                let framed = window.show(ui, State::REST, shadow, |ui| {
                    match inner {
                        Some(i) => {
                            ui.set_min_width(i.x);
                            ui.set_max_width(i.x);
                        }
                        None => {
                            ui.set_min_width(min);
                            ui.set_max_width(max);
                        }
                    }
                    let top = ui.cursor().top();
                    // The room left under the bands for a resized one.
                    let body = |ui: &Ui| {
                        inner
                            .filter(|_| unrolled)
                            .map(|i| (top + i.y - ui.cursor().top()).max(0.0))
                    };
                    if g.is_lone() {
                        let m = &g.members[0];
                        let (_, b) = band::begin(ui, &style, |ui, state| {
                            band_row(ui, &t, &titles[0], state, m, Some(g.pinned), &mut asked);
                        });
                        if !m.rolled {
                            match body(ui) {
                                Some(h) => scrolled(ui, id, h, |ui| app.show_panel(ui, &m.panel)),
                                None => app.show_panel(ui, &m.panel),
                            }
                        }
                        band::paint(ui, &b, &style);
                        moved = Some(b);
                        return;
                    }
                    // The group's band (its tabs, for tabs), then each
                    // member shown under its own.
                    let joined = titles.join(" · ");
                    let tabs = g.arrangement == Arrangement::Tabs;
                    let (_, b) = band::begin(ui, &style, |ui, state| {
                        let shown = tabs.then_some(g.active);
                        group_row(ui, &t, &joined, &titles, shown, state, g.pinned, &mut asked);
                    });
                    let mut column = |ui: &mut Ui| {
                        for (i, m) in g.members.iter().enumerate() {
                            if tabs && m.panel != *g.shown() {
                                continue;
                            }
                            if !bands.is_empty() {
                                ui.add_space(gap);
                            }
                            // Room for the member's band to reach up into.
                            ui.add_space(style.padding);
                            let top = ui.cursor().top() - style.padding;
                            let (mb, bottom) = ui
                                .push_id(("egui_pop.palette.member", m.panel.as_str()), |ui| {
                                    let (_, mb) = band::begin(ui, &style, |ui, state| {
                                        band_row(ui, &t, &titles[i], state, m, None, &mut asked);
                                    });
                                    if !m.rolled {
                                        app.show_panel(ui, &m.panel);
                                    }
                                    (mb, ui.min_rect().bottom())
                                })
                                .inner;
                            bands.push(MemberBand {
                                panel: m.panel.clone(),
                                band: mb,
                                top,
                                bottom,
                            });
                        }
                        // Painted once the column's width is known.
                        for mb in &bands {
                            band::paint(ui, &mb.band, &style);
                        }
                    };
                    match body(ui) {
                        Some(h) => scrolled(ui, id, h, |ui| {
                            ui.set_min_width(ui.available_width());
                            column(ui);
                        }),
                        None => column(ui),
                    }
                    band::paint(ui, &b, &style);
                    moved = Some(b);
                });
                let grip = palette_dimension(ui.ctx(), "palette.grip", 6.0);
                edges = resize_edges(ui, id, framed.response.rect, grip, unrolled);
            });
        let rect = shown.response.rect;
        for p in g.panels() {
            rects.insert(p.clone(), rect);
        }
        if g.is_lone() {
            members.insert(lead.clone(), rect);
        }
        // Each member from its band's top to the next one's (the last to
        // the frame's bottom).
        let own: Vec<Rect> = bands
            .iter()
            .enumerate()
            .map(|(i, mb)| {
                let bottom = bands.get(i + 1).map_or(rect.bottom(), |n| n.top);
                Rect::from_x_y_ranges(rect.x_range(), mb.top..=bottom.max(mb.bottom))
            })
            .collect();
        for (mb, r) in bands.iter().zip(&own) {
            members.insert(mb.panel.clone(), *r);
        }
        // Escape in a palette gives the keyboard back.
        let layer = shown.response.layer_id;
        if let Some(focused) = ctx.memory(|m| m.focused())
            && ctx
                .read_response(focused)
                .is_some_and(|r| r.layer_id == layer)
            && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape))
        {
            ctx.memory_mut(|m| m.surrender_focus(focused));
        }
        if let Some(p) = asked.close {
            app.set_panel_visible(&p, false);
            continue;
        }
        if let Some(p) = asked.roll {
            app.change_workbench(Change::Roll(p.clone(), !g.rolled(&p)));
        }
        if asked.pin {
            app.change_workbench(Change::Pin(lead.clone(), !g.pinned));
        }
        if let Some(i) = asked.show {
            // Which tab shows: the group's state, not a step.
            if let Some(fl) = app.workbench_mut().layout.floating_mut(&lead) {
                fl.active = i;
            }
            ctx.request_repaint();
        }
        let drag = Drag {
            content,
            before: &before,
            members: &members_before,
        };
        if let Some(b) = moved {
            follow(app, ctx, &g, rect, &b, &drag);
        }
        follow_resize(app, ctx, &g, rect, &edges, (least, most));
        for (mb, own) in bands.iter().zip(own) {
            follow_member(app, ctx, &g, &mb.panel, own, rect, &mb.band, &drag);
        }
    }
    {
        let mut wb = app.workbench_mut();
        wb.palette_rects = rects;
        wb.member_rects = members;
    }
    paint_hints(app, ctx, content, &members_before);
}

/// The band's row of a palette alone, or of a member of a group: the
/// title (in the band's `state`), then Close, Roll and, for a palette
/// alone (`pinned` is its pin), Pin at its end.
fn band_row(
    ui: &mut Ui,
    t: &Tokens,
    title: &str,
    state: State,
    m: &Member,
    pinned: Option<bool>,
    asked: &mut Asked,
) {
    let side = t.size.control_height.max(24.0);
    let kit = Kit::of(ui.ctx());
    ui.horizontal(|ui| {
        ui.add(
            egui::Label::new(
                egui::RichText::new(title)
                    .strong()
                    .color(kit.header_band.style(state).text),
            )
            .truncate()
            .selectable(false),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Named with the palette's title: one name per button.
            let named = |ui: &Ui, key: &strings::StrKey| {
                strings::tr_args(ui.ctx(), key, &[("palette", title)])
            };
            let name = named(ui, &txt::PALETTE_CLOSE);
            if band::button(ui, side, kit.glyph(Glyph::Close), &name, false).clicked() {
                asked.close = Some(m.panel.clone());
            }
            let (icon, name) = if m.rolled {
                (Icon::ChevronDown, &txt::PALETTE_ROLL_DOWN)
            } else {
                (Icon::ChevronUp, &txt::PALETTE_ROLL_UP)
            };
            let name = named(ui, name);
            if band::button(ui, side, icon, &name, false).clicked() {
                asked.roll = Some(m.panel.clone());
            }
            if let Some(pinned) = pinned {
                pin_button(ui, side, title, pinned, asked);
            }
        });
    });
}

/// The band's row of a group: its members' titles (as tabs when
/// `shown` says which tab shows), then Pin.
#[allow(clippy::too_many_arguments)]
fn group_row(
    ui: &mut Ui,
    t: &Tokens,
    title: &str,
    titles: &[String],
    shown: Option<usize>,
    state: State,
    pinned: bool,
    asked: &mut Asked,
) {
    let side = t.size.control_height.max(24.0);
    let kit = Kit::of(ui.ctx());
    ui.horizontal(|ui| {
        match shown {
            Some(active) => {
                for (i, name) in titles.iter().enumerate() {
                    if widgets::selectable_label(ui, i == active, name.as_str()).clicked() {
                        asked.show = Some(i);
                    }
                }
            }
            None => {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(title)
                            .weak()
                            .color(kit.header_band.style(state).text),
                    )
                    .truncate()
                    .selectable(false),
                );
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            pin_button(ui, side, title, pinned, asked);
        });
    });
}

/// Pin, or Unpin, named with the palette's (or the group's) title.
fn pin_button(ui: &mut Ui, side: f32, title: &str, pinned: bool, asked: &mut Asked) {
    let (icon, key) = if pinned {
        (Icon::Lock, &txt::PALETTE_UNPIN)
    } else {
        (Icon::Unlock, &txt::PALETTE_PIN)
    };
    let name = strings::tr_args(ui.ctx(), key, &[("palette", title)]);
    if band::button(ui, side, icon, &name, pinned).clicked() {
        asked.pin = true;
    }
}

/// What a drag reads of the last frame: the workbench's rectangle, each
/// panel's group as drawn, each member as drawn.
struct Drag<'a> {
    content: Rect,
    before: &'a BTreeMap<PanelId, Rect>,
    members: &'a BTreeMap<PanelId, Rect>,
}

impl Drag<'_> {
    /// Where a box of `size` dragged from `start` lands with the pointer
    /// moved from `origin` to `now`: snapped to the workbench's edges and
    /// to the palettes `skip` does not name, kept inside.
    fn land(
        &self,
        ctx: &egui::Context,
        start: [f32; 2],
        size: Vec2,
        origin: Pos2,
        now: Pos2,
        skip: impl Fn(&PanelId) -> bool,
    ) -> [f32; 2] {
        let raw = Rect::from_min_size(self.content.min + Vec2::from(start) + (now - origin), size);
        let others: Vec<Rect> = self
            .before
            .iter()
            .filter(|(p, _)| !skip(p))
            .map(|(_, r)| *r)
            .collect();
        let at = clamp(
            snap(raw, self.content, &others, magnet(ctx)).min,
            size,
            self.content,
        );
        [at.x - self.content.min.x, at.y - self.content.min.y]
    }
}

/// Follows a group's band (a palette alone's band): a drag moves the
/// group live (snapped, clamped), the drop joins another palette's group
/// over it (`palette.group`), docks every member over an area
/// (`palette.dock`), else moves it (`palette.move`); a double click rolls
/// a palette alone up or down.
fn follow(
    app: &AppHandle,
    ctx: &egui::Context,
    g: &FloatingGroup,
    rect: Rect,
    b: &band::Band,
    d: &Drag<'_>,
) {
    let lead = g.lead().clone();
    if b.double_clicked {
        if g.is_lone() {
            app.change_workbench(Change::Roll(lead.clone(), !g.rolled(&lead)));
        }
        return;
    }
    let drag = app.workbench_mut().palette_drag.clone();
    let mine = drag.filter(|x| x.whole && x.panel == lead);
    if b.dragging && !g.pinned {
        let (origin, now) = ctx.input(|i| (i.pointer.press_origin(), i.pointer.latest_pos()));
        let (Some(origin), Some(now)) = (origin, now) else {
            return;
        };
        let start = mine.as_ref().map_or(g.pos, |x| x.start);
        let pos = d.land(ctx, start, rect.size(), origin, now, |p| g.holds(p));
        let hint = join_target(d.members, now, |p| g.holds(p)).map(|(into, at, r)| GroupHint {
            panels: g.panels().cloned().collect(),
            into,
            at,
            rect: r,
        });
        let mut wb = app.workbench_mut();
        let mut moved = false;
        if let Some(fl) = wb.layout.floating_mut(&lead) {
            moved = fl.pos != pos;
            fl.pos = pos;
        }
        wb.palette_drag = Some(PaletteDrag {
            panel: lead,
            whole: true,
            start,
            pos,
            at: now,
        });
        wb.group_hint = hint;
        // Only a move needs another frame: a held, still pointer does not.
        if moved {
            ctx.request_repaint();
        }
        return;
    }
    let Some(x) = mine else {
        return;
    };
    // Dropped: back where it started, then the step decides.
    let (to, area, hint) = {
        let mut wb = app.workbench_mut();
        wb.palette_drag = None;
        let hint = wb.group_hint.take();
        let area = wb.dock_at(x.at, magnet(ctx));
        let fl = wb.layout.floating_mut(&lead);
        let to = fl.map(|fl| std::mem::replace(&mut fl.pos, x.start));
        (to, area, hint)
    };
    let change = match (hint, area, to) {
        (Some(h), _, _) => Some(Change::Group(h.panels, h.into, h.at)),
        (None, Some(area), _) if g.is_lone() => Some(Change::Dock(lead.clone(), area)),
        (None, Some(area), _) => Some(Change::DockGroup(lead.clone(), area)),
        (None, None, Some(to)) if to != x.start => Some(Change::MovePalette(lead.clone(), to)),
        _ => None,
    };
    run(app, &lead, change);
    ctx.request_repaint();
}

/// Follows the band of a member of a group: a drag shows where it would
/// go, the drop joins another palette's group over it (or moves it
/// within its own), docks it alone over an area, takes it out of its
/// group anywhere else (`palette.ungroup`), or leaves it in its group
/// when dropped back over it; a double click rolls it up or down.
#[allow(clippy::too_many_arguments)]
fn follow_member(
    app: &AppHandle,
    ctx: &egui::Context,
    g: &FloatingGroup,
    panel: &PanelId,
    own: Rect,
    group: Rect,
    b: &band::Band,
    d: &Drag<'_>,
) {
    if b.double_clicked {
        app.change_workbench(Change::Roll(panel.clone(), !g.rolled(panel)));
        return;
    }
    let drag = app.workbench_mut().palette_drag.clone();
    let mine = drag.filter(|x| !x.whole && x.panel == *panel);
    if b.dragging && !g.pinned {
        let (origin, now) = ctx.input(|i| (i.pointer.press_origin(), i.pointer.latest_pos()));
        let (Some(origin), Some(now)) = (origin, now) else {
            return;
        };
        let from = own.min - d.content.min;
        let start = mine.as_ref().map_or([from.x, from.y], |x| x.start);
        let pos = d.land(ctx, start, own.size(), origin, now, |p| g.holds(p));
        let hint = join_target(d.members, now, |p| p == panel).map(|(into, at, r)| GroupHint {
            panels: vec![panel.clone()],
            into,
            at,
            rect: r,
        });
        let mut wb = app.workbench_mut();
        let moved = mine.as_ref().is_none_or(|x| x.pos != pos || x.at != now);
        wb.palette_drag = Some(PaletteDrag {
            panel: panel.clone(),
            whole: false,
            start,
            pos,
            at: now,
        });
        wb.group_hint = hint;
        if moved {
            ctx.request_repaint();
        }
        return;
    }
    let Some(x) = mine else {
        return;
    };
    let (area, hint) = {
        let mut wb = app.workbench_mut();
        wb.palette_drag = None;
        let hint = wb.group_hint.take();
        (wb.dock_at(x.at, magnet(ctx)), hint)
    };
    let change = match (hint, area) {
        (Some(h), _) => Some(Change::Group(h.panels, h.into, h.at)),
        (None, Some(area)) => Some(Change::Dock(panel.clone(), area)),
        (None, None) if group.contains(x.at) => None,
        (None, None) => Some(Change::Ungroup(panel.clone(), x.pos)),
    };
    run(app, panel, change);
    ctx.request_repaint();
}

/// Runs a dropped palette's change, telling why when it is refused.
fn run(app: &AppHandle, panel: &PanelId, change: Option<Change>) {
    let out = change.and_then(|c| app.change_workbench(c));
    if let Some(Outcome::Done(Err(e))) = &out {
        app.workbench_refused(&format!("palette {panel}"), e);
    }
}

/// While palettes are dragged: over another palette, where they would
/// join its group (the kit's `dock-preview`, as for tabs); a member
/// dragged out of its group, where it would float alone.
fn paint_hints(
    app: &AppHandle,
    ctx: &egui::Context,
    content: Rect,
    members: &BTreeMap<PanelId, Rect>,
) {
    let (drag, hint, dock) = {
        let wb = app.workbench_mut();
        let dock = wb
            .palette_drag
            .as_ref()
            .and_then(|d| wb.dock_at(d.at, magnet(ctx)));
        (wb.palette_drag.clone(), wb.group_hint.clone(), dock)
    };
    let rect = match (&hint, &drag) {
        (Some(h), _) => h.rect,
        (None, Some(d)) if !d.whole && dock.is_none() => {
            let Some(own) = members.get(&d.panel) else {
                return;
            };
            Rect::from_min_size(content.min + Vec2::from(d.pos), own.size())
        }
        _ => return,
    };
    let kit = Kit::of(ctx);
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        Id::new("egui_pop.palette.hint"),
    ));
    let cx = KitCx {
        painter: &painter,
        ppp: ctx.pixels_per_point(),
    };
    kit.dock_preview.paint_part(
        &cx,
        &Part {
            rect,
            room: rect,
            state: State::REST,
            label: None,
        },
    );
}

/// The room between the members of a stacked group (`palette.group.gap`).
fn group_gap(ctx: &egui::Context) -> f32 {
    Tokens::of(ctx)
        .resolved()
        .dimension("palette.group.gap")
        .unwrap_or(0.0)
}

/// How close a dragged palette snaps to an edge or another palette, and
/// docks in an area (`palette.magnet`).
pub(crate) fn magnet(ctx: &egui::Context) -> f32 {
    Tokens::of(ctx)
        .resolved()
        .dimension("palette.magnet")
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{pos2, vec2};

    fn bounds(w: f32, h: f32) -> Rect {
        Rect::from_min_size(pos2(10.0, 20.0), vec2(w, h))
    }

    const SIZE: Vec2 = Vec2::new(100.0, 50.0);

    /// A palette of `SIZE` at `pos` from the bounds' top-left.
    fn at(b: Rect, pos: [f32; 2]) -> Rect {
        Rect::from_min_size(b.min + Vec2::from(pos), SIZE)
    }

    #[test]
    fn each_corner_anchors_both_axes_and_follows_a_resize() {
        let small = bounds(800.0, 600.0);
        let large = bounds(1000.0, 700.0);
        for (pos, x, y, moved) in [
            ([0.0, 0.0], AnchorEdge::Start, AnchorEdge::Start, [0.0, 0.0]),
            (
                [700.0, 0.0],
                AnchorEdge::End,
                AnchorEdge::Start,
                [900.0, 0.0],
            ),
            (
                [0.0, 550.0],
                AnchorEdge::Start,
                AnchorEdge::End,
                [0.0, 650.0],
            ),
            (
                [700.0, 550.0],
                AnchorEdge::End,
                AnchorEdge::End,
                [900.0, 650.0],
            ),
        ] {
            let a = anchor_of(at(small, pos), small);
            assert_eq!((a.x, a.y), (x, y), "{pos:?}");
            assert_eq!(a.offset, [0.0, 0.0]);
            let p = anchored_pos(pos, SIZE, large, &a);
            assert_eq!(p, large.min + Vec2::from(moved), "{pos:?}");
        }
    }

    #[test]
    fn one_axis_only_keeps_the_other_place() {
        let small = bounds(800.0, 600.0);
        let a = anchor_of(at(small, [700.0, 200.0]), small);
        assert_eq!((a.x, a.y), (AnchorEdge::End, AnchorEdge::None));
        let large = bounds(1000.0, 700.0);
        let p = anchored_pos([700.0, 200.0], SIZE, large, &a);
        assert_eq!(p, large.min + vec2(900.0, 200.0));
        let a = anchor_of(at(small, [300.0, 550.0]), small);
        assert_eq!((a.x, a.y), (AnchorEdge::None, AnchorEdge::End));
        let p = anchored_pos([300.0, 550.0], SIZE, large, &a);
        assert_eq!(p, large.min + vec2(300.0, 650.0));
    }

    #[test]
    fn the_offset_is_kept_through_a_resize() {
        let a = Anchor {
            x: AnchorEdge::End,
            y: AnchorEdge::Start,
            offset: [12.0, 8.0],
        };
        for w in [600.0, 800.0, 1200.0] {
            let b = bounds(w, 500.0);
            let p = anchored_pos([0.0, 0.0], SIZE, b, &a);
            assert_eq!(b.right() - (p.x + SIZE.x), 12.0, "{w}");
            assert_eq!(p.y - b.top(), 8.0, "{w}");
        }
        // Too small for the offset: still inside.
        let b = bounds(105.0, 500.0);
        let p = anchored_pos([0.0, 0.0], SIZE, b, &a);
        assert_eq!(p.x, b.left());
    }

    #[test]
    fn dragged_away_from_the_edge_or_onto_a_palette_it_is_not_anchored() {
        let b = bounds(800.0, 600.0);
        // Dropped away from the edges, beyond the magnet: snap leaves it,
        // so no anchor.
        let away = snap(at(b, [650.0, 200.0]), b, &[], 12.0);
        assert!(anchor_of(away, b).is_none());
        // Within the magnet: snapped onto the edge, anchored.
        let near = snap(at(b, [692.0, 200.0]), b, &[], 12.0);
        assert_eq!(anchor_of(near, b).x, AnchorEdge::End);
        // Snapped beside another palette (not the workbench): no anchor.
        let other = Rect::from_min_size(b.min + vec2(400.0, 180.0), SIZE);
        let beside = snap(at(b, [505.0, 200.0]), b, &[other], 12.0);
        assert_eq!(beside.left(), other.right());
        assert!(anchor_of(beside, b).is_none());
        // Unanchored: today's behaviour, clamped inside.
        let p = anchored_pos([780.0, 200.0], SIZE, b, &Anchor::default());
        assert_eq!(p, b.min + vec2(700.0, 200.0));
    }
}
