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
//! open first, rolled down.
//!
//! **Steps** (ADR 0011): `palette.open` (`{panel}`), `palette.float`
//! (`{panel, pos}`), `palette.dock` (`{panel, area}`), `palette.move`
//! (`{panel, pos}`, once a drag is dropped), `palette.roll`
//! (`{panel, rolled}`) and `palette.pin` (`{panel, pinned}`); `pos` is
//! the top-left corner in points from the workbench's top-left. Each
//! done change is published as `palette.opened`, `palette.floated`,
//! `palette.docked`, `palette.moved`, `palette.rolled` or
//! `palette.pinned`.
//!
//! Not built yet: a palette outside the main window (a native utility
//! window through egui's viewports).
//!
//! [`PanelSpec::palette`]: egui_pop_model::panels::PanelSpec::palette

use std::collections::BTreeMap;

use egui::{Id, Key, Modifiers, Pos2, Rect, Ui, Vec2};
use egui_pop_core::band::{self, BandStyle};
use egui_pop_core::icon::Icon;
use egui_pop_core::kit::{Glyph, Kit, State};
use egui_pop_core::lock;
use egui_pop_core::strings;
use egui_pop_core::theme::Tokens;
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::ids::{CommandId, PanelId, SettingId, StepId};
use egui_pop_model::keymap::BindOptions;
use egui_pop_model::layout::{Anchor, AnchorEdge, FloatingGroup};
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
/// A floating panel rolls up or down (`{panel, rolled}`).
pub const STEP_ROLL: StepId = StepId::from_static("palette.roll");
/// A floating panel is pinned or unpinned (`{panel, pinned}`).
pub const STEP_PIN: StepId = StepId::from_static("palette.pin");

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
            "A floating panel docks in an area (`panel`, `area`).",
            json!({ "panel": "app.tools", "area": "start_side" }),
        ),
        (
            STEP_MOVE,
            "A floating panel is moved, once dropped (`panel`, `pos`).",
            json!({ "panel": "app.tools", "pos": [120.0, 40.0] }),
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
            .description("Puts the floating palettes back where they first open, rolled down."),
        |call| {
            reset(call.app);
            Ok(Json::Null)
        },
    )?;
    Ok(())
}

/// Puts the floating palettes back where they first open, rolled down,
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

/// One floating palette as drawn: its panel, and its group's place.
struct Palette {
    panel: PanelId,
    rolled: bool,
    pinned: bool,
    pos: [f32; 2],
    anchor: Anchor,
}

/// What a palette's band buttons asked for.
#[derive(Default)]
struct Asked {
    close: bool,
    roll: bool,
    pin: bool,
}

/// Draws the floating palettes over the workbench, and follows their
/// drags: the drop docks a palette over an area, else moves it through
/// `palette.move`.
pub(crate) fn show(app: &AppHandle, ctx: &egui::Context) {
    let (floating, content, before) = {
        let wb = app.workbench_mut();
        (
            wb.layout.floating.clone(),
            wb.content,
            wb.palette_rects.clone(),
        )
    };
    let any = if floating.is_empty() { "false" } else { "true" };
    if app.context().get(CONTEXT_FLOATING) != any {
        app.set_context(CONTEXT_FLOATING, any);
    }
    if floating.is_empty() || !content.is_positive() {
        app.workbench_mut().palette_rects.clear();
        return;
    }
    let t = Tokens::of(ctx);
    let hide = hidden(app);
    let stroke = app.context().get(CONTEXT_STROKE) == "true" && setting_bool(app, SETTING_FADE);
    let idle = setting_f32(app, SETTING_IDLE_OPACITY)
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    let pointer = ctx.input(|i| i.pointer.latest_pos());
    let mut rects = BTreeMap::new();
    for (g, m) in floating
        .iter()
        .flat_map(|g| g.members.iter().map(move |m| (g, m)))
    {
        let f = Palette {
            panel: m.panel.clone(),
            rolled: m.rolled,
            pinned: g.pinned,
            pos: g.pos,
            anchor: g.anchor,
        };
        if hide && !f.pinned {
            continue;
        }
        let Some(spec) = app.workbench_mut().spec(&f.panel).cloned() else {
            continue;
        };
        let id = area_id(&f.panel);
        let drawn = before.get(&f.panel).map(|r| r.size());
        let size = drawn.unwrap_or(Vec2::ZERO);
        let dragging = app
            .workbench_mut()
            .palette_drag
            .as_ref()
            .is_some_and(|d| d.0 == f.panel);
        // Inside the workbench, again when the window is resized; an
        // anchored palette keeps its distance to its edges (not while it
        // is dragged: the drop sets its anchor again).
        let pos = if dragging {
            clamp(content.min + Vec2::from(f.pos), size, content)
        } else {
            anchored_pos(f.pos, size, content, &f.anchor)
        };
        if !f.anchor.is_none() && !dragging && drawn.is_some() {
            // Its place follows the edges (not a move: no step).
            let at = [pos.x - content.min.x, pos.y - content.min.y];
            if at != f.pos
                && let Some(fl) = app.workbench_mut().layout.floating_mut(&f.panel)
            {
                fl.pos = at;
            }
        }
        let hovered = before
            .get(&f.panel)
            .zip(pointer)
            .is_some_and(|(r, p)| r.contains(p));
        let target = if f.pinned || dragging {
            1.0
        } else if stroke {
            STROKE_OPACITY
        } else if hovered {
            1.0
        } else {
            idle
        };
        let opacity = ctx.animate_value_with_time(id.with("opacity"), target, t.motion_duration);
        let title = app.panel_title(ctx, &f.panel);
        let mut asked = Asked::default();
        let mut moved: Option<band::Band> = None;
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
                window.show(ui, State::REST, shadow, |ui| {
                    ui.set_min_width(spec.min_size);
                    ui.set_max_width(spec.default_size.unwrap_or(320.0).max(spec.min_size));
                    let (_, b) = band::begin(ui, &style, |ui, state| {
                        band_row(ui, &t, &title, state, &f, &mut asked);
                    });
                    if !f.rolled {
                        app.show_panel(ui, &f.panel);
                    }
                    band::paint(ui, &b, &style);
                    moved = Some(b);
                });
            });
        let rect = shown.response.rect;
        rects.insert(f.panel.clone(), rect);
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
        if asked.close {
            app.set_panel_visible(&f.panel, false);
            continue;
        }
        if asked.roll {
            app.change_workbench(Change::Roll(f.panel.clone(), !f.rolled));
        }
        if asked.pin {
            app.change_workbench(Change::Pin(f.panel.clone(), !f.pinned));
        }
        if let Some(b) = moved {
            follow(app, ctx, &f, rect, &before, &b, content);
        }
    }
    app.workbench_mut().palette_rects = rects;
}

/// The band's row: the title (in the band's `state`), then Close, Roll
/// and Pin at its end.
fn band_row(ui: &mut Ui, t: &Tokens, title: &str, state: State, f: &Palette, asked: &mut Asked) {
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
                asked.close = true;
            }
            let (icon, name) = if f.rolled {
                (Icon::ChevronDown, &txt::PALETTE_ROLL_DOWN)
            } else {
                (Icon::ChevronUp, &txt::PALETTE_ROLL_UP)
            };
            let name = named(ui, name);
            if band::button(ui, side, icon, &name, false).clicked() {
                asked.roll = true;
            }
            let (icon, name) = if f.pinned {
                (Icon::Lock, &txt::PALETTE_UNPIN)
            } else {
                (Icon::Unlock, &txt::PALETTE_PIN)
            };
            let name = named(ui, name);
            if band::button(ui, side, icon, &name, f.pinned).clicked() {
                asked.pin = true;
            }
        });
    });
}

/// Follows a palette's band: a drag moves it live (snapped, clamped),
/// the drop docks it over an area or moves it through `palette.move`; a
/// double click rolls it up or down.
fn follow(
    app: &AppHandle,
    ctx: &egui::Context,
    f: &Palette,
    rect: Rect,
    before: &BTreeMap<PanelId, Rect>,
    b: &band::Band,
    content: Rect,
) {
    if b.double_clicked {
        app.change_workbench(Change::Roll(f.panel.clone(), !f.rolled));
        return;
    }
    let drag = app.workbench_mut().palette_drag.clone();
    let mine = drag.as_ref().filter(|d| d.0 == f.panel).cloned();
    if b.dragging && !f.pinned {
        let (origin, now) = ctx.input(|i| (i.pointer.press_origin(), i.pointer.latest_pos()));
        let (Some(origin), Some(now)) = (origin, now) else {
            return;
        };
        let start = mine.as_ref().map_or(f.pos, |d| d.1);
        let raw = Rect::from_min_size(
            content.min + Vec2::from(start) + (now - origin),
            rect.size(),
        );
        let others: Vec<Rect> = before
            .iter()
            .filter(|(p, _)| **p != f.panel)
            .map(|(_, r)| *r)
            .collect();
        let magnet = magnet(ctx);
        let at = clamp(
            snap(raw, content, &others, magnet).min,
            rect.size(),
            content,
        );
        let mut wb = app.workbench_mut();
        let pos = [at.x - content.min.x, at.y - content.min.y];
        let mut moved = false;
        if let Some(fl) = wb.layout.floating_mut(&f.panel) {
            moved = fl.pos != pos;
            fl.pos = pos;
        }
        wb.palette_drag = Some((f.panel.clone(), start, now));
        // Only a move needs another frame: a held, still pointer does not.
        if moved {
            ctx.request_repaint();
        }
        return;
    }
    let Some((panel, start, at)) = mine else {
        return;
    };
    // Dropped: back where it started, then the step decides.
    let (to, area) = {
        let mut wb = app.workbench_mut();
        wb.palette_drag = None;
        let area = wb.dock_at(at, magnet(ctx));
        let fl = wb.layout.floating_mut(&panel);
        let to = fl.map(|fl| std::mem::replace(&mut fl.pos, start));
        (to, area)
    };
    let out = match (area, to) {
        (Some(area), _) => app.change_workbench(Change::Dock(panel.clone(), area)),
        (None, Some(to)) if to != start => {
            app.change_workbench(Change::MovePalette(panel.clone(), to))
        }
        _ => None,
    };
    if let Some(Outcome::Done(Err(e))) = &out {
        app.workbench_refused(&format!("palette {panel}"), e);
    }
    ctx.request_repaint();
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
