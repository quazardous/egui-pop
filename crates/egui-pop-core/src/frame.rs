//! The window's own frame, for windows drawn without the system's
//! decorations: a title bar (a caption to drag, double click to
//! maximize, the minimize, maximize and close buttons, room for a logo
//! and for the application's items) and resize edges along the window's
//! sides, never thinner than [`MIN_GRIP_PX`] physical pixels.
//!
//! **In full screen** the bar stays (it holds the application's menus and
//! items) without what only a window needs: no minimize, a button leaving
//! full screen where maximize was ([`FrameRequest::ExitFullscreen`]), no
//! drag, a double click asking nothing, and no resize edges.
//!
//! The frame **publishes its hit regions** every frame, per window, in
//! physical client pixels ([`HitRegions`], read through [`HitTable`]):
//! what a platform layer needs to answer the system's hit tests itself
//! (on Windows, `WM_NCHITTEST`, so that Snap Layouts show over the
//! maximize button). [`HitRegions::hit`] maps a point to its region.
//!
//! The widget only reports what was asked ([`FrameRequest`]); dragging
//! and resizing are handed to the system at once. What a request does is
//! the application's (egui-pop's `frame` plugin turns them into steps
//! plugins may veto or replace).
//!
//! ```
//! use egui_pop_core::frame::{self, FrameRequest};
//!
//! fn ui(ui: &mut egui::Ui) {
//!     frame::resize_edges(ui.ctx());
//!     let out = frame::TitleBar::new("My application").show(ui, |_| {}, |_| {});
//!     match out.request {
//!         Some(FrameRequest::Close) => ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close),
//!         Some(FrameRequest::Minimize) => {
//!             ui.ctx().send_viewport_cmd(egui::ViewportCommand::Minimized(true))
//!         }
//!         _ => {}
//!     }
//! }
//! ```

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use egui::{
    Color32, Context, CursorIcon, Pos2, Rect, ResizeDirection, Role, Sense, Ui, Vec2,
    ViewportCommand, ViewportId, pos2, vec2,
};
use serde::Serialize;

use crate::a11y;
use crate::kit::{ButtonsHover, ButtonsSide, Glyph, Kit, KitCx, Part, State, Tone};
use crate::state::{Slot, lock};
use crate::theme::Tokens;

/// The thinnest resize edge, in physical pixels, whatever the scale.
pub const MIN_GRIP_PX: f32 = 6.0;

/// What a point of a window is, for the system's hit test.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Hit {
    /// The application's: widgets, content.
    Client,
    /// The title bar: drags the window, double click maximizes, a right
    /// click opens the system menu.
    Caption,
    Minimize,
    Maximize,
    Close,
    Left,
    Right,
    Top,
    Bottom,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// A rectangle in physical client pixels (from the window's client
/// area's top left corner).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize)]
pub struct RectPx {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl RectPx {
    /// A rectangle in points, at `ppp` physical pixels per point,
    /// rounded to the pixels it covers.
    pub fn from_points(r: Rect, ppp: f32) -> RectPx {
        let x0 = (r.min.x * ppp).round() as i32;
        let y0 = (r.min.y * ppp).round() as i32;
        let x1 = (r.max.x * ppp).round() as i32;
        let y1 = (r.max.y * ppp).round() as i32;
        RectPx {
            x: x0,
            y: y0,
            w: (x1 - x0).max(0),
            h: (y1 - y0).max(0),
        }
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }
}

/// A window's frame regions, in physical client pixels.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct HitRegions {
    /// Physical pixels per point when they were measured.
    pub pixels_per_point: f32,
    /// The client area's size.
    pub size: [i32; 2],
    /// The title bar.
    pub caption: Option<RectPx>,
    /// Widgets on the title bar (a logo, menus, the application's
    /// buttons): they take their own clicks.
    pub holes: Vec<RectPx>,
    pub minimize: Option<RectPx>,
    pub maximize: Option<RectPx>,
    pub close: Option<RectPx>,
    /// The resize edges' width; zero when the window cannot be resized
    /// from its edges (maximized, full screen).
    pub grip: i32,
    /// The window is in full screen: its caption does not move it (a
    /// platform layer answers it as client area).
    pub fullscreen: bool,
}

impl HitRegions {
    /// The regions of a frame laid out in points.
    pub fn measure(layout: &FrameLayout, ppp: f32) -> HitRegions {
        let px = |r: Rect| RectPx::from_points(r, ppp);
        let window = px(layout.window);
        HitRegions {
            pixels_per_point: ppp,
            size: [window.w, window.h],
            caption: layout.caption.map(px),
            holes: layout.holes.iter().copied().map(px).collect(),
            minimize: layout.minimize.map(px),
            maximize: layout.maximize.map(px),
            close: layout.close.map(px),
            grip: if layout.resizable {
                (layout.grip * ppp).round().max(MIN_GRIP_PX) as i32
            } else {
                0
            },
            fullscreen: layout.fullscreen,
        }
    }

    /// What the point `(x, y)` (physical client pixels) is: a resize
    /// edge first, then a button, a widget on the bar, the caption,
    /// else the client area.
    pub fn hit(&self, x: i32, y: i32) -> Hit {
        let [w, h] = self.size;
        if self.grip > 0 && x >= 0 && y >= 0 && x < w && y < h {
            let g = self.grip;
            let (left, right) = (x < g, x >= w - g);
            let (top, bottom) = (y < g, y >= h - g);
            let edge = match (left, right, top, bottom) {
                (true, _, true, _) => Some(Hit::TopLeft),
                (_, true, true, _) => Some(Hit::TopRight),
                (true, _, _, true) => Some(Hit::BottomLeft),
                (_, true, _, true) => Some(Hit::BottomRight),
                (true, ..) => Some(Hit::Left),
                (_, true, ..) => Some(Hit::Right),
                (_, _, true, _) => Some(Hit::Top),
                (_, _, _, true) => Some(Hit::Bottom),
                _ => None,
            };
            if let Some(e) = edge {
                return e;
            }
        }
        let inside = |r: &Option<RectPx>| r.is_some_and(|r| r.contains(x, y));
        if inside(&self.close) {
            Hit::Close
        } else if inside(&self.maximize) {
            Hit::Maximize
        } else if inside(&self.minimize) {
            Hit::Minimize
        } else if self.holes.iter().any(|r| r.contains(x, y)) {
            Hit::Client
        } else if inside(&self.caption) {
            Hit::Caption
        } else {
            Hit::Client
        }
    }
}

/// A frame's parts in points, as drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct FrameLayout {
    /// The window's client area.
    pub window: Rect,
    pub caption: Option<Rect>,
    pub holes: Vec<Rect>,
    pub minimize: Option<Rect>,
    pub maximize: Option<Rect>,
    pub close: Option<Rect>,
    /// The resize edges' width in points (before the pixel minimum).
    pub grip: f32,
    pub resizable: bool,
    /// The window is in full screen.
    pub fullscreen: bool,
}

impl Default for FrameLayout {
    fn default() -> Self {
        Self {
            window: Rect::NOTHING,
            caption: None,
            holes: Vec::new(),
            minimize: None,
            maximize: None,
            close: None,
            grip: 0.0,
            resizable: false,
            fullscreen: false,
        }
    }
}

/// Every window's published regions, shared: a platform layer keeps a
/// clone and reads a window's regions without calling into egui. The
/// lock is held only to swap or clone an `Arc`.
#[derive(Clone, Debug, Default)]
pub struct HitTable(Arc<Mutex<BTreeMap<ViewportId, Arc<HitRegions>>>>);

impl HitTable {
    /// The context's table (one per context, made on first use).
    pub fn of(ctx: &Context) -> HitTable {
        const TABLE: Slot<HitTable> = Slot::new("core.frame.table");
        TABLE.get_or_insert_with(ctx, HitTable::default)
    }

    /// A window's last published regions.
    pub fn get(&self, viewport: ViewportId) -> Option<Arc<HitRegions>> {
        lock(&self.0).get(&viewport).cloned()
    }

    /// Every window with regions.
    pub fn viewports(&self) -> Vec<ViewportId> {
        lock(&self.0).keys().copied().collect()
    }

    /// Publishes a window's regions.
    pub fn publish(&self, viewport: ViewportId, regions: HitRegions) {
        lock(&self.0).insert(viewport, Arc::new(regions));
    }

    /// Forgets a window (closed, or drawn by the system again).
    pub fn forget(&self, viewport: ViewportId) {
        lock(&self.0).remove(&viewport);
    }
}

/// The parts of the current window's frame drawn this pass, merged and
/// published by [`publish`], per window.
const PENDING: Slot<FrameLayout> = Slot::new("core.frame.pending");

fn pending(ctx: &Context) -> Slot<FrameLayout> {
    PENDING.keyed(ctx.viewport_id())
}

fn update_pending(ctx: &Context, f: impl FnOnce(&mut FrameLayout)) {
    let slot = pending(ctx);
    let mut layout = slot.get_or_default(ctx);
    f(&mut layout);
    slot.set(ctx, layout);
}

/// Publishes the current window's regions from what the frame drew this
/// pass (the title bar and the edges); `TitleBar::show` calls it.
pub fn publish(ctx: &Context) -> HitRegions {
    let mut layout = pending(ctx).get_or_default(ctx);
    layout.window = ctx.content_rect();
    if layout.grip == 0.0 {
        layout.grip = grip_token(ctx);
    }
    layout.resizable = resizable(ctx);
    layout.fullscreen = full_screen(ctx);
    let regions = HitRegions::measure(&layout, ctx.pixels_per_point());
    HitTable::of(ctx).publish(ctx.viewport_id(), regions.clone());
    regions
}

/// Says the current window shows no title bar this pass (full screen
/// without its bar, a zen mode): its caption and buttons are left out of
/// the next [`publish`].
pub fn hide_title_bar(ctx: &Context) {
    update_pending(ctx, |l| {
        l.caption = None;
        l.holes.clear();
        l.minimize = None;
        l.maximize = None;
        l.close = None;
    });
}

/// The current window's published regions.
pub fn regions(ctx: &Context) -> Option<Arc<HitRegions>> {
    HitTable::of(ctx).get(ctx.viewport_id())
}

/// The line the kit draws around a window the application draws
/// (`window.rest.border-width`), dimmer while another window has the
/// focus (`window.inactive.border-color`), or none in full screen. A
/// maximized window keeps it.
pub fn outline(ctx: &Context) -> egui::Stroke {
    if full_screen(ctx) {
        return egui::Stroke::NONE;
    }
    let kit = crate::kit::Kit::of(ctx);
    // Unknown (a backend that does not say): drawn as the active one.
    if ctx.input(|i| i.viewport().focused).unwrap_or(true) {
        kit.window.outline
    } else {
        kit.window_edge.inactive
    }
}

/// The sides of a window against an edge of its screen (tiled or
/// snapped there): its corners on those sides stay square.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stuck {
    pub left: bool,
    pub top: bool,
    pub right: bool,
    pub bottom: bool,
}

impl Stuck {
    /// The sides of `outer` (the window with its frame, in points)
    /// within a point of an edge of the screen it overlaps most, among
    /// `screens`; none without screens.
    pub fn against(outer: Rect, screens: &[Rect]) -> Stuck {
        let area = |s: &Rect| {
            let i = s.intersect(outer);
            if i.is_positive() { i.area() } else { 0.0 }
        };
        let Some(screen) = screens
            .iter()
            .filter(|s| area(s) > 0.0)
            .max_by(|a, b| area(a).total_cmp(&area(b)))
        else {
            return Stuck::default();
        };
        const NEAR: f32 = 1.0;
        Stuck {
            left: (outer.left() - screen.left()).abs() <= NEAR,
            top: (outer.top() - screen.top()).abs() <= NEAR,
            right: (screen.right() - outer.right()).abs() <= NEAR,
            bottom: (screen.bottom() - outer.bottom()).abs() <= NEAR,
        }
    }
}

/// Where a window stands, for its corners: maximized, in full screen,
/// against a screen's edges. `None` is a state the backend does not
/// tell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Placement {
    pub maximized: Option<bool>,
    pub fullscreen: Option<bool>,
    pub stuck: Stuck,
}

impl Placement {
    /// The current window's, its sides against `screens` (in points) as
    /// far as the system tells where it is: X11 and Windows do, Wayland
    /// does not (no window position, and winit 0.30 keeps the
    /// compositor's tiled states to itself), so a window tiled there
    /// counts as free.
    pub fn of(ctx: &Context, screens: &[Rect]) -> Placement {
        ctx.input(|i| {
            let v = i.viewport();
            Placement {
                maximized: v.maximized,
                fullscreen: v.fullscreen,
                stuck: v
                    .outer_rect
                    .map(|r| Stuck::against(r, screens))
                    .unwrap_or_default(),
            }
        })
    }

    /// Whether the window floats free: known not maximized and not in
    /// full screen.
    pub fn floating(&self) -> bool {
        self.maximized == Some(false) && self.fullscreen == Some(false)
    }
}

/// A window's corners from its theme's `radius` (in points) and where it
/// stands: rounded only while it floats free, a corner square on a side
/// against a screen's edge; all square when maximized, in full screen,
/// or when the backend does not tell those states.
pub fn corner_radius(radius: f32, place: &Placement) -> egui::CornerRadius {
    if radius <= 0.0 || !place.floating() {
        return egui::CornerRadius::ZERO;
    }
    let r = radius.round().clamp(0.0, f32::from(u8::MAX)) as u8;
    let s = place.stuck;
    let at = |a: bool, b: bool| if a || b { 0 } else { r };
    egui::CornerRadius {
        nw: at(s.left, s.top),
        ne: at(s.right, s.top),
        sw: at(s.left, s.bottom),
        se: at(s.right, s.bottom),
    }
}

/// Whether the windows this context decorates may have rounded corners:
/// they are built transparent (what shows outside a rounded corner is
/// the desktop). Off unless said ([`set_rounding`]).
const ROUNDING: Slot<bool> = Slot::new("core.frame.rounding");

/// Says whether the windows the application decorates are built
/// transparent, so that [`window_edge`] may round their corners.
pub fn set_rounding(ctx: &Context, on: bool) {
    ROUNDING.set(ctx, on);
}

/// Whether [`set_rounding`] turned rounded corners on.
pub fn rounding(ctx: &Context) -> bool {
    ROUNDING.get(ctx).unwrap_or(false)
}

/// The line and corners of the current window's own edge
/// ([`window_edge`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WindowEdge {
    /// The line around the window ([`outline`]).
    pub stroke: egui::Stroke,
    /// Its corners ([`corner_radius`]); square without [`rounding`].
    pub corners: egui::CornerRadius,
}

/// The current window's edge as the kit and its placement say, its sides
/// against `screens` (in points) square.
pub fn window_edge(ctx: &Context, screens: &[Rect]) -> WindowEdge {
    let corners = if rounding(ctx) {
        corner_radius(
            crate::kit::Kit::of(ctx).window_edge.radius,
            &Placement::of(ctx, screens),
        )
    } else {
        egui::CornerRadius::ZERO
    };
    WindowEdge {
        stroke: outline(ctx),
        corners,
    }
}

/// The corners to round in the current window at the end of this pass.
const ROUND: Slot<egui::CornerRadius> = Slot::new("core.frame.round");

/// Draws the current window's edge: its line over everything, inside
/// the window, and, with rounded corners, rounds at the end of the pass
/// what egui painted at the window's corners (the panels' and bars'
/// backgrounds), so the transparent window shows through outside them.
/// A mesh there (a picture) is left as painted.
pub fn paint_window_edge(ctx: &Context, edge: WindowEdge) {
    let rect = ctx.content_rect();
    if edge.stroke.width > 0.0 {
        ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::unique("egui_pop.window_edge"),
        ))
        .rect_stroke(rect, edge.corners, edge.stroke, egui::StrokeKind::Inside);
    }
    if edge.corners != egui::CornerRadius::ZERO {
        let st = crate::state::shared::<RoundState>(ctx, "core.frame.round_state");
        let mut st = lock(&st);
        if !st.installed {
            st.installed = true;
            ctx.add_plugin(RoundCorners);
        }
        drop(st);
        ROUND.keyed(ctx.viewport_id()).set(ctx, edge.corners);
    }
}

#[derive(Debug, Default)]
struct RoundState {
    installed: bool,
}

/// Rounds, at the end of a window's pass, the rectangles painted over
/// its corners ([`paint_window_edge`]).
#[derive(Debug)]
struct RoundCorners;

impl egui::plugin::Plugin for RoundCorners {
    fn debug_name(&self) -> &'static str {
        "egui_pop_core::frame::round_corners"
    }

    fn on_end_pass(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let slot = ROUND.keyed(ctx.viewport_id());
        // Said again each pass it is wanted.
        let Some(corners) = slot.remove(&ctx) else {
            return;
        };
        let window = ctx.content_rect();
        let mut layers: Vec<egui::LayerId> = ctx.memory(|m| m.layer_ids().collect());
        layers.push(egui::LayerId::background());
        ctx.graphics_mut(|g| {
            for layer in layers {
                if let Some(list) = g.get_mut(layer) {
                    for i in 0..list.next_idx().0 {
                        list.mutate_shape(egui::layers::ShapeIdx(i), |c| {
                            round_shape(&mut c.shape, window, corners);
                        });
                    }
                }
            }
        });
    }
}

/// Rounds `shape`'s corners that cover `window`'s, by `corners`: a
/// rectangle (and those in a list of shapes) over a corner of the window
/// or within a line's width of it (the shell inside the window's line),
/// its radius less what it stands in from the corner. A plain fill
/// reaching past the window is cut back to it first; a line, a picture
/// or a shadow keeps its place.
pub fn round_shape(shape: &mut egui::Shape, window: Rect, corners: egui::CornerRadius) {
    /// How far in from a corner a rectangle still counts as on it.
    const NEAR: f32 = 2.0;
    match shape {
        egui::Shape::Vec(shapes) => {
            for s in shapes {
                round_shape(s, window, corners);
            }
        }
        egui::Shape::Rect(r) => {
            let fixed = r.stroke.width > 0.0 || r.brush.is_some() || r.blur_width > 0.0;
            // Per corner: how far in the rectangle stands (negative: past
            // the window's edge), on each axis.
            let insets = |rect: Rect| {
                let (l, t) = (rect.left() - window.left(), rect.top() - window.top());
                let (rt, b) = (
                    window.right() - rect.right(),
                    window.bottom() - rect.bottom(),
                );
                [(l, t), (rt, t), (l, b), (rt, b)]
            };
            let want = [corners.nw, corners.ne, corners.sw, corners.se];
            let on = |(x, y): (f32, f32), w: u8| {
                w > 0 && x <= NEAR && y <= NEAR && (!fixed || (x >= -NEAR && y >= -NEAR))
            };
            if !insets(r.rect).iter().zip(want).any(|(i, w)| on(*i, w)) {
                return;
            }
            if !fixed {
                r.rect = r.rect.intersect(window);
            }
            let c = &mut r.corner_radius;
            let mine = [&mut c.nw, &mut c.ne, &mut c.sw, &mut c.se];
            for ((inset, w), mine) in insets(r.rect).into_iter().zip(want).zip(mine) {
                if on(inset, w) {
                    let less = inset.0.max(inset.1).max(0.0);
                    let radius = (f32::from(w) - less).round().max(0.0) as u8;
                    *mine = (*mine).max(radius);
                }
            }
        }
        _ => {}
    }
}

/// Whether the current window is in full screen.
pub fn full_screen(ctx: &Context) -> bool {
    ctx.input(|i| i.viewport().fullscreen.unwrap_or(false))
}

fn resizable(ctx: &Context) -> bool {
    ctx.input(|i| {
        let v = i.viewport();
        !v.maximized.unwrap_or(false) && !v.fullscreen.unwrap_or(false)
    })
}

/// The resize edges' width in points here: the token, never under
/// [`MIN_GRIP_PX`] physical pixels.
pub fn grip(ctx: &Context) -> f32 {
    grip_token(ctx).max(MIN_GRIP_PX / ctx.pixels_per_point())
}

/// The resize edges' width the theme gives (`window.grip`).
fn grip_token(ctx: &Context) -> f32 {
    Tokens::of(ctx)
        .resolved()
        .dimension("window.grip")
        .unwrap_or(0.0)
}

/// Lets the window be resized from its edges and corners, as a system
/// frame would; call it before drawing the window's content. Returns the
/// edge under the pointer.
pub fn resize_edges(ctx: &Context) -> Option<ResizeDirection> {
    let g = grip(ctx);
    update_pending(ctx, |l| l.grip = g);
    if !resizable(ctx) {
        return None;
    }
    let pos = ctx.input(|i| i.pointer.hover_pos())?;
    let r = ctx.content_rect();
    let (left, right) = (pos.x - r.left() < g, r.right() - pos.x < g);
    let (top, bottom) = (pos.y - r.top() < g, r.bottom() - pos.y < g);
    let direction = match (left, right, top, bottom) {
        (true, _, true, _) => ResizeDirection::NorthWest,
        (_, true, true, _) => ResizeDirection::NorthEast,
        (true, _, _, true) => ResizeDirection::SouthWest,
        (_, true, _, true) => ResizeDirection::SouthEast,
        (true, ..) => ResizeDirection::West,
        (_, true, ..) => ResizeDirection::East,
        (_, _, true, _) => ResizeDirection::North,
        (_, _, _, true) => ResizeDirection::South,
        _ => return None,
    };
    ctx.set_cursor_icon(match direction {
        ResizeDirection::North | ResizeDirection::South => CursorIcon::ResizeVertical,
        ResizeDirection::East | ResizeDirection::West => CursorIcon::ResizeHorizontal,
        ResizeDirection::NorthWest | ResizeDirection::SouthEast => CursorIcon::ResizeNwSe,
        ResizeDirection::NorthEast | ResizeDirection::SouthWest => CursorIcon::ResizeNeSw,
    });
    if ctx.input(|i| i.pointer.primary_pressed()) {
        ctx.send_viewport_cmd(ViewportCommand::BeginResize(direction));
        compositor_grab(ctx);
    }
    Some(direction)
}

/// Says that the compositor takes the pointer from this pass on, for an
/// interactive move or resize of the window (`ViewportCommand::StartDrag`,
/// `BeginResize`): the next pass of this window starts with the primary
/// button released, if egui still holds it down.
///
/// upstream: Wayland compositors (labwc, GNOME's mutter) keep the
/// button's release for their move, and winit never hands it to the
/// window: egui then believes the button held until the pointer leaves
/// the window, every click after it a drag or nothing (docs/upstream.md,
/// "winit: no release after a compositor move on Wayland"). A release that
/// does come later finds the button up and changes nothing.
pub fn compositor_grab(ctx: &Context) {
    let viewport = ctx.viewport_id();
    let st = crate::state::shared::<Grabs>(ctx, "core.frame.grabs");
    let mut grabs = lock(&st);
    if !grabs.installed {
        grabs.installed = true;
        ctx.add_plugin(ReleaseAfterGrab);
    }
    if !grabs.pending.contains(&viewport) {
        grabs.pending.push(viewport);
    }
}

/// The windows whose compositor took the pointer ([`compositor_grab`]).
#[derive(Debug, Default)]
struct Grabs {
    installed: bool,
    pending: Vec<ViewportId>,
}

/// Releases the primary button in a window's next pass after a
/// compositor grab ([`compositor_grab`]).
#[derive(Debug)]
struct ReleaseAfterGrab;

impl egui::plugin::Plugin for ReleaseAfterGrab {
    fn debug_name(&self) -> &'static str {
        "egui_pop_core::frame::release_after_grab"
    }

    fn input_hook(&mut self, ctx: &Context, input: &mut egui::RawInput) {
        let viewport = input.viewport_id;
        {
            let st = crate::state::shared::<Grabs>(ctx, "core.frame.grabs");
            let mut grabs = lock(&st);
            let Some(at) = grabs.pending.iter().position(|v| *v == viewport) else {
                return;
            };
            grabs.pending.swap_remove(at);
        }
        let (down, pos) = ctx.input_for(viewport, |i| {
            (i.pointer.primary_down(), i.pointer.latest_pos())
        });
        let released = input.events.iter().any(|e| {
            matches!(
                e,
                egui::Event::PointerButton {
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    ..
                }
            )
        });
        if down && !released {
            // Before anything else of the pass: a press that follows is a
            // new one.
            input.events.insert(
                0,
                egui::Event::PointerButton {
                    pos: pos.unwrap_or_default(),
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: ctx.input_for(viewport, |i| i.modifiers),
                },
            );
        }
    }
}

/// What the user asked of the window through its frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FrameRequest {
    Minimize,
    /// Maximize, or restore when maximized.
    ToggleMaximize,
    Close,
    /// The system menu, opened at this point (a right click on the
    /// caption, or the logo).
    SystemMenu(Pos2),
    /// Leave full screen (the button in place of maximize).
    ExitFullscreen,
}

/// What [`TitleBar::show`] gives back.
#[derive(Clone, Debug, PartialEq)]
pub struct TitleBarOutput {
    pub request: Option<FrameRequest>,
    /// The bar.
    pub rect: Rect,
    /// Where the title was drawn; `None` when there was no room for it.
    pub title: Option<Rect>,
    /// Its parts, in points (the window's rectangle is left empty).
    pub layout: FrameLayout,
}

/// A window button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Minimize,
    Maximize,
    Close,
}

/// The title bar.
#[derive(Clone, Debug)]
pub struct TitleBar<'a> {
    title: &'a str,
    buttons: [bool; 3],
    publish: bool,
    full_screen: Option<bool>,
}

impl<'a> TitleBar<'a> {
    /// A bar showing `title`, with the three buttons.
    pub fn new(title: &'a str) -> Self {
        Self {
            title,
            buttons: [true; 3],
            publish: true,
            full_screen: None,
        }
    }

    /// Draws the bar as in full screen (`true`) or not, whatever the
    /// window is (a preview); by default, as the window is.
    pub fn full_screen(mut self, on: bool) -> Self {
        self.full_screen = Some(on);
        self
    }

    /// Whether the bar publishes the window's regions (off for a preview
    /// drawn inside a window).
    pub fn publish(mut self, on: bool) -> Self {
        self.publish = on;
        self
    }

    /// Shows or hides a button (an auxiliary window without minimize).
    pub fn button(mut self, which: Button, shown: bool) -> Self {
        self.buttons[which as usize] = shown;
        self
    }

    /// Draws the bar across `ui`'s width (a top panel's `ui`), with
    /// `leading` items after the start edge (a logo, a menu) and
    /// `trailing` items before the buttons (the application's buttons);
    /// both take their own clicks and are published as holes in the
    /// caption. Publishes the window's regions.
    pub fn show(
        self,
        ui: &mut Ui,
        leading: impl FnOnce(&mut Ui),
        trailing: impl FnOnce(&mut Ui),
    ) -> TitleBarOutput {
        let ctx = ui.ctx().clone();
        let t = Tokens::of(&ctx);
        let kit = Kit::of(&ctx);
        let height = kit.title_bar.data.height;
        let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        let fullscreen = self.full_screen.unwrap_or_else(|| full_screen(&ctx));
        let full = ui.max_rect();
        let bar = Rect::from_min_size(full.min, vec2(full.width(), height));
        let ppp = ctx.pixels_per_point();
        // The bar's box first; its label (the title) after the items.
        kit.title_bar.paint(
            &KitCx {
                painter: ui.painter(),
                ppp,
            },
            &Part {
                rect: bar,
                room: bar,
                state: State::REST,
                label: None,
            },
        );
        // The caption: drag, double click, right click. Widgets on the
        // bar are drawn after it and take their clicks first. In full
        // screen the bar neither moves the window nor maximizes it. No Tab
        // stop: focused, it would show nothing and answer no key.
        let drag = ui.interact(
            bar,
            ui.scope_id().with("egui_pop.caption"),
            Sense::CLICK | Sense::DRAG,
        );
        let mut request = None;
        if drag.double_clicked() {
            if !fullscreen {
                request = Some(FrameRequest::ToggleMaximize);
            }
        } else if drag.drag_started() {
            if !fullscreen {
                ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                compositor_grab(&ctx);
            }
        } else if crate::pointer::claim::opens_context_menu(&drag) {
            let at = drag.interact_pointer_pos().unwrap_or(bar.left_bottom());
            request = Some(FrameRequest::SystemMenu(at));
        }
        let mut layout = FrameLayout {
            caption: Some(bar),
            fullscreen,
            ..FrameLayout::default()
        };
        // The buttons, from the edge `title-bar.buttons-side` names, close
        // outermost: minimize, maximize, close at the end edge; close,
        // minimize, maximize from the start edge. In full screen: no
        // minimize, and the way out of full screen where maximize was.
        let mut buttons = self.buttons;
        if fullscreen {
            buttons[Button::Minimize as usize] = false;
            buttons[Button::Maximize as usize] = true;
        }
        let side = kit.window.buttons_side;
        let order = match side {
            ButtonsSide::End => [Button::Close, Button::Maximize, Button::Minimize],
            ButtonsSide::Start => [Button::Close, Button::Minimize, Button::Maximize],
        };
        // The buttons and the items are centred above the bar's line.
        let line = kit.title_bar.style(State::REST).border_width;
        let inner = bar.with_max_y(bar.bottom() - line);
        let mut slots: [Option<Rect>; 3] = [None; 3];
        let mut x = match side {
            ButtonsSide::End => bar.right(),
            ButtonsSide::Start => bar.left(),
        };
        for which in order {
            if !buttons[which as usize] {
                continue;
            }
            let w = match which {
                Button::Close => kit.window_close.data.width,
                _ => kit.window_button.data.width,
            };
            let (x0, x1) = match side {
                ButtonsSide::End => (x - w, x),
                ButtonsSide::Start => (x, x + w),
            };
            x = match side {
                ButtonsSide::End => x0,
                ButtonsSide::Start => x1,
            };
            slots[which as usize] = Some(Rect::from_min_max(
                pos2(x0, inner.top()),
                pos2(x1, inner.bottom()),
            ));
        }
        // `buttons-hover: group`: all three are hovered while the pointer
        // is over any of them (egui's pointer, or the system's on Windows).
        let group = kit.window.buttons_hover == ButtonsHover::Group && {
            let all = slots
                .iter()
                .flatten()
                .fold(Rect::NOTHING, |a, r| a.union(*r));
            native_hover(&ctx).is_some() || (all.is_positive() && ui.rect_contains_pointer(all))
        };
        for which in order {
            let Some(r) = slots[which as usize] else {
                continue;
            };
            let look = match which {
                Button::Maximize if fullscreen => Look::ExitFullscreen,
                Button::Maximize if maximized => Look::Restore,
                _ => Look::Plain,
            };
            if window_button(ui, r, which, look, group) {
                request = Some(match which {
                    Button::Minimize => FrameRequest::Minimize,
                    Button::Maximize if fullscreen => FrameRequest::ExitFullscreen,
                    Button::Maximize => FrameRequest::ToggleMaximize,
                    Button::Close => FrameRequest::Close,
                });
            }
        }
        layout.minimize = slots[Button::Minimize as usize];
        layout.maximize = slots[Button::Maximize as usize];
        layout.close = slots[Button::Close as usize];
        if fullscreen {
            // The way out of full screen takes its own clicks: a hole in
            // the caption, never the system's maximize button (no Snap
            // Layouts over it).
            layout.holes.extend(layout.maximize.take());
        }
        // The room the items and the title share: between the buttons and
        // the bar's other edge.
        let (lo, hi) = match side {
            ButtonsSide::End => (bar.left(), x),
            ButtonsSide::Start => (x, bar.right()),
        };
        let gap = t.space.m;
        // The application's items, at the end of that room.
        let end = Rect::from_min_max(pos2(lo, inner.top()), pos2(hi - gap, inner.bottom()));
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(end)
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        trailing(&mut child);
        let trailing_rect = child.min_rect();
        if trailing_rect.width() > 0.0 {
            layout.holes.push(trailing_rect);
        }
        // Leading items, at its start.
        let start = Rect::from_min_max(
            pos2(lo + gap, inner.top()),
            pos2(trailing_rect.left().min(hi) - gap, inner.bottom()),
        );
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(start)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        leading(&mut child);
        let leading_rect = child.min_rect();
        if leading_rect.width() > 0.0 {
            layout.holes.push(leading_rect);
        }
        // The title, where the kit's `title-bar.align` puts it, between
        // the items. Named for assistive technology and tests; takes no
        // click, so the bar still drags under it.
        let room = Rect::from_min_max(
            pos2(leading_rect.right().max(lo), inner.top()),
            pos2(trailing_rect.left().min(hi), inner.bottom()),
        );
        let width = room.width() - 2.0 * kit.title_bar.data.padding_x;
        let mut title_rect = None;
        if width > 0.0 && !self.title.is_empty() {
            let font = egui::TextStyle::Body.resolve(ui.style());
            let mut job = egui::text::LayoutJob::simple_singleline(
                self.title.into(),
                font,
                Color32::PLACEHOLDER,
            );
            job.wrap = egui::text::TextWrapping::truncate_at_width(width);
            let galley = ui.fonts_mut(|f| f.layout_job(job));
            let part = Part {
                rect: bar,
                room,
                state: State::REST,
                label: Some(&galley),
            };
            let at = kit.title_bar.label_rect(&part, galley.size());
            title_rect = Some(at);
            let title = self.title;
            ui.interact(at, ui.scope_id().with("egui_pop.title"), Sense::hover())
                .widget_info(|| egui::WidgetInfo::labeled(Role::Label, true, title));
            kit.title_bar.paint_label(
                &KitCx {
                    painter: ui.painter(),
                    ppp,
                },
                &part,
            );
        }
        if !self.publish {
            ui.advance_cursor_after_rect(bar);
            return TitleBarOutput {
                request,
                rect: bar,
                title: title_rect,
                layout,
            };
        }
        let grip = grip(&ctx);
        update_pending(&ctx, |l| {
            l.caption = layout.caption;
            l.holes = layout.holes.clone();
            l.minimize = layout.minimize;
            l.maximize = layout.maximize;
            l.close = layout.close;
            l.grip = grip;
        });
        publish(&ctx);
        ui.advance_cursor_after_rect(bar);
        TitleBarOutput {
            request,
            rect: bar,
            title: title_rect,
            layout,
        }
    }
}

/// The window button the system says the pointer is over, per window.
const NATIVE_HOVER: Slot<Option<Button>> = Slot::new("core.frame.native_hover");

/// Says which window button the pointer is over when the system, not
/// egui, sees the pointer there (on Windows the buttons answer the
/// system's hit test, so their hover never reaches egui): the button is
/// drawn hot. `None` when the pointer left them.
pub fn set_native_hover(ctx: &Context, viewport: ViewportId, button: Option<Button>) {
    NATIVE_HOVER.keyed(viewport).set(ctx, button);
    ctx.request_repaint_of(viewport);
}

/// The window button the system says the pointer is over.
pub fn native_hover(ctx: &Context) -> Option<Button> {
    // Read before the memory's lock: the viewport id takes the context's.
    let viewport = ctx.viewport_id();
    NATIVE_HOVER.keyed(viewport).get(ctx).flatten()
}

/// How the maximize slot's button shows.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Look {
    Plain,
    /// A maximized window's restore.
    Restore,
    /// A full screen window's way out.
    ExitFullscreen,
}

/// Draws a window button; says whether it was clicked. `group`: the
/// pointer is over one of the buttons and the kit hovers them together.
///
/// Each button carries a tone, which a kit may give to tell them apart
/// (the macOS flavour's red, yellow and green): minimize `warning`,
/// maximize `success`, close `danger`. A kit giving no tones (the
/// default) draws each with its component's own box.
fn window_button(ui: &mut Ui, rect: Rect, which: Button, look: Look, group: bool) -> bool {
    let name = crate::strings::tr(
        ui.ctx(),
        match which {
            Button::Minimize => &crate::strings::MINIMIZE,
            Button::Maximize if look == Look::ExitFullscreen => &crate::strings::EXIT_FULLSCREEN,
            Button::Maximize if look == Look::Restore => &crate::strings::RESTORE,
            Button::Maximize => &crate::strings::MAXIMIZE,
            Button::Close => &crate::strings::CLOSE,
        },
    );
    let name = name.as_str();
    let id = ui.scope_id().with(("egui_pop.window_button", which as u8));
    let response = ui.interact(rect, id, Sense::click());
    a11y::name(&response, Role::Button, name);
    let kit = Kit::of(ui.ctx());
    // On Windows the system, not egui, may see the pointer over the
    // button (its hit test answers for it): drawn as hovered then.
    let state = State::from_response(&response)
        .with(State::HOVER, group || native_hover(ui.ctx()) == Some(which))
        .toned(match which {
            Button::Minimize => Tone::Warning,
            Button::Maximize => Tone::Success,
            Button::Close => Tone::Danger,
        });
    let component = match which {
        Button::Close => &kit.window_close,
        _ => &kit.window_button,
    };
    let painter = ui.painter_at(rect);
    component.paint(
        &KitCx {
            painter: &painter,
            ppp: ui.ctx().pixels_per_point(),
        },
        &Part {
            rect,
            room: rect,
            state,
            label: None,
        },
    );
    let b = component.style(state);
    let glyph = kit.glyph(match which {
        Button::Minimize => Glyph::WindowMinimize,
        Button::Maximize if look == Look::ExitFullscreen => Glyph::WindowExitFullscreen,
        Button::Maximize if look == Look::Restore => Glyph::WindowRestore,
        Button::Maximize => Glyph::WindowMaximize,
        Button::Close => Glyph::WindowClose,
    });
    // The state's `indicator-size` when the kit gives one; else ten
    // points across, or half the button's own size when it has one
    // (`height`: a traffic light's circle).
    let h = component.data.height;
    let size = if b.indicator_size > 0.0 {
        b.indicator_size
    } else if h > 0.0 {
        (h / 2.0).max(4.0)
    } else {
        10.0
    };
    if b.text.a() > 0 {
        // On the pixel grid: an icon's texture drawn between pixels blurs.
        let ppp = ui.ctx().pixels_per_point();
        let min = rect.center() - Vec2::splat(size / 2.0);
        let min = pos2((min.x * ppp).round() / ppp, (min.y * ppp).round() / ppp);
        glyph.paint(
            &painter,
            Rect::from_min_size(min, Vec2::splat(size)),
            b.text,
        );
    }
    response.on_hover_text(name).clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout() -> FrameLayout {
        // A 800 x 600 window, a 32 pt bar, 44 pt buttons, a logo hole.
        let bar = Rect::from_min_size(Pos2::ZERO, vec2(800.0, 32.0));
        let b = |i: f32| Rect::from_min_size(pos2(800.0 - 44.0 * i, 0.0), vec2(44.0, 32.0));
        FrameLayout {
            window: Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0)),
            caption: Some(bar),
            holes: vec![Rect::from_min_size(pos2(8.0, 4.0), vec2(24.0, 24.0))],
            minimize: Some(b(3.0)),
            maximize: Some(b(2.0)),
            close: Some(b(1.0)),
            grip: 4.0,
            resizable: true,
            fullscreen: false,
        }
    }

    #[test]
    fn points_map_to_their_regions_at_every_scale() {
        for ppp in [1.0, 1.25, 1.5, 2.0] {
            let r = HitRegions::measure(&layout(), ppp);
            let px = |x: f32, y: f32| ((x * ppp) as i32, (y * ppp) as i32);
            let hit = |x: f32, y: f32| {
                let (x, y) = px(x, y);
                r.hit(x, y)
            };
            assert_eq!(hit(400.0, 16.0), Hit::Caption, "{ppp}");
            assert_eq!(hit(20.0, 16.0), Hit::Client, "the logo, {ppp}");
            assert_eq!(hit(800.0 - 22.0, 16.0), Hit::Close, "{ppp}");
            assert_eq!(hit(800.0 - 66.0, 16.0), Hit::Maximize, "{ppp}");
            assert_eq!(hit(800.0 - 110.0, 16.0), Hit::Minimize, "{ppp}");
            assert_eq!(hit(400.0, 300.0), Hit::Client, "{ppp}");
            assert_eq!(hit(0.5, 300.0), Hit::Left, "{ppp}");
            assert_eq!(hit(799.5, 300.0), Hit::Right, "{ppp}");
            assert_eq!(hit(400.0, 0.5), Hit::Top, "{ppp}");
            assert_eq!(hit(400.0, 599.5), Hit::Bottom, "{ppp}");
            assert_eq!(hit(0.5, 0.5), Hit::TopLeft, "{ppp}");
            assert_eq!(hit(799.5, 599.5), Hit::BottomRight, "{ppp}");
            assert_eq!(r.size, [(800.0 * ppp) as i32, (600.0 * ppp) as i32]);
        }
    }

    #[test]
    fn edges_never_fall_under_the_pixel_minimum_and_vanish_when_maximized() {
        let mut l = layout();
        l.grip = 2.0;
        assert_eq!(HitRegions::measure(&l, 1.0).grip, MIN_GRIP_PX as i32);
        assert_eq!(HitRegions::measure(&l, 4.0).grip, 8);
        l.resizable = false;
        let r = HitRegions::measure(&l, 1.0);
        assert_eq!(r.grip, 0);
        assert_eq!(r.hit(0, 300), Hit::Client);
        assert_eq!(r.hit(400, 0), Hit::Caption);
    }

    /// Draws a title bar 800 points wide with a leading item, in a context
    /// whose theme is pop-dark with `json` merged, the pointer at
    /// `pointer`; gives what the last pass painted.
    fn bar_with(
        json: &str,
        pointer: Option<Pos2>,
    ) -> (
        TitleBarOutput,
        Option<Rect>,
        Context,
        Vec<egui::epaint::ClippedShape>,
    ) {
        let mut themes = egui_pop_tokens::Themes::default();
        themes
            .add_text("frame-test", json)
            .expect("test theme reads");
        let tokens = Tokens::new(themes.resolve(&egui_pop_tokens::ThemeSpec::new("frame-test")));
        let ctx = Context::default();
        crate::theme::install_tokens(&ctx, tokens);
        let mut out = None;
        let mut lead = None;
        let mut shapes = Vec::new();
        // Twice: the second pass sees the pointer over what the first drew.
        for _ in 0..2 {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
                events: pointer
                    .map(|p| vec![egui::Event::PointerMoved(p)])
                    .unwrap_or_default(),
                ..Default::default()
            };
            let mut full = ctx.run_ui(input, |ui| {
                let o = TitleBar::new("Title").show(
                    ui,
                    |ui| {
                        lead = Some(ui.label("Leading").rect);
                    },
                    |_| {},
                );
                out = Some(o);
            });
            full.textures_delta.clear();
            shapes = full.shapes;
        }
        (out.expect("the bar was drawn"), lead, ctx, shapes)
    }

    const START: &str = r#"
                        $extends: "pop-dark"
                        title-bar:
                          buttons-side: {$type: "string", $value: "start"}
                        "#;

    #[test]
    fn buttons_at_the_start_come_first_and_the_title_stays_centred() {
        let (out, lead, ctx, _) = bar_with(START, None);
        let kit = Kit::of(&ctx);
        assert!(kit.problems.is_empty(), "{:?}", kit.problems);
        assert_eq!(kit.window.buttons_side, ButtonsSide::Start);
        let l = &out.layout;
        let (close, min, max) = (l.close.unwrap(), l.minimize.unwrap(), l.maximize.unwrap());
        // Close at the start edge, then minimize, then maximize.
        assert_eq!(close.left(), 0.0);
        assert_eq!(min.left(), close.right());
        assert_eq!(max.left(), min.right());
        // The leading items after the buttons, published as a hole.
        let lead = lead.unwrap();
        assert!(lead.left() > max.right(), "{lead:?} after {max:?}");
        assert!(l.holes.iter().any(|h| h.contains_rect(lead)));
        // The title on the bar's centre.
        let title = out.title.expect("the title was laid out");
        assert!((title.center().x - 400.0).abs() < 1.0, "{title:?}");
        // The regions a platform layer reads follow the buttons: Snap
        // Layouts over maximize wherever it is.
        let r = regions(&ctx).expect("published");
        let ppp = r.pixels_per_point;
        let at = |p: Rect| ((p.center().x * ppp) as i32, (p.center().y * ppp) as i32);
        assert_eq!(r.hit(at(max).0, at(max).1), Hit::Maximize);
        assert_eq!(r.hit(at(close).0, at(close).1), Hit::Close);
        assert_eq!(
            r.hit((790.0 * ppp) as i32, (16.0 * ppp) as i32),
            Hit::Caption
        );
    }

    #[test]
    fn the_default_kit_keeps_the_buttons_at_the_end() {
        let (out, lead, _, _) = bar_with(
            r#"
                                         $extends: "pop-dark"
                                         "#,
            None,
        );
        let l = &out.layout;
        assert_eq!(l.close.unwrap().right(), 800.0);
        assert_eq!(l.maximize.unwrap().right(), l.close.unwrap().left());
        assert_eq!(l.minimize.unwrap().right(), l.maximize.unwrap().left());
        assert!(lead.unwrap().right() < l.minimize.unwrap().left());
    }

    /// A compositor that keeps the release of the press that moved the
    /// window (labwc, mutter on Wayland): the caption drags the window again, and
    /// a window button still clicks.
    #[test]
    fn the_bar_moves_again_and_its_buttons_click_after_a_move_whose_release_never_came() {
        let ctx = Context::default();
        crate::theme::install(&ctx, &crate::theme::ThemeSpec::new("pop-dark"));
        // Each pass: the minimize button, the bar's request, whether the
        // window was handed to the compositor.
        let pass = |events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
                events,
                ..Default::default()
            };
            let (mut minimize, mut request) = (None, None);
            let mut full = ctx.run_ui(input, |ui| {
                let out = TitleBar::new("Title").show(ui, |_| {}, |_| {});
                minimize = out.layout.minimize;
                request = out.request;
            });
            full.textures_delta.clear();
            let started = full
                .viewport_output
                .values()
                .any(|v| v.commands.contains(&ViewportCommand::StartDrag));
            (minimize, request, started)
        };
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let caption = pos2(300.0, 10.0);
        pass(vec![egui::Event::PointerMoved(caption)]);
        pass(vec![press(caption, true)]);
        // The caption dragged: the window is moved by the compositor, which
        // never hands the release back.
        let away = caption + vec2(40.0, 0.0);
        let mut started = pass(vec![egui::Event::PointerMoved(away)]).2;
        started |= pass(vec![egui::Event::PointerMoved(away + vec2(5.0, 0.0))]).2;
        assert!(started, "the move went to the compositor");
        // A second move: a new press on the caption drags it again (egui
        // held the first press's drag until a release).
        pass(vec![egui::Event::PointerMoved(caption)]);
        pass(vec![press(caption, true)]);
        let mut again = pass(vec![egui::Event::PointerMoved(away)]).2;
        again |= pass(vec![egui::Event::PointerMoved(away + vec2(5.0, 0.0))]).2;
        assert!(again, "the window moves a second time");
        let (minimize, ..) = pass(vec![egui::Event::PointerMoved(away)]);
        let button = minimize.expect("a minimize button").center();
        let mut requests = Vec::new();
        for events in [
            vec![egui::Event::PointerMoved(button)],
            vec![press(button, true)],
            vec![press(button, false)],
            Vec::new(),
        ] {
            requests.extend(pass(events).1);
        }
        assert_eq!(requests, [FrameRequest::Minimize]);
    }

    #[test]
    fn in_full_screen_the_bar_keeps_close_and_its_way_out_but_no_edges() {
        let ctx = Context::default();
        let mut out = None;
        for _ in 0..2 {
            let mut input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
                ..Default::default()
            };
            input.viewports.insert(
                ViewportId::ROOT,
                egui::ViewportInfo {
                    fullscreen: Some(true),
                    ..Default::default()
                },
            );
            let mut full = ctx.run_ui(input, |ui| {
                resize_edges(ui.ctx());
                out = Some(TitleBar::new("Title").show(ui, |_| {}, |_| {}));
            });
            full.textures_delta.clear();
        }
        let out = out.expect("the bar was drawn");
        let l = &out.layout;
        assert!(l.fullscreen);
        assert!(l.minimize.is_none() && l.maximize.is_none(), "{l:?}");
        let close = l.close.expect("close stays");
        assert_eq!(close.right(), 800.0);
        // The way out, where maximize was: a hole, so client area.
        let exit = *l
            .holes
            .iter()
            .find(|h| h.right() == close.left())
            .expect("the way out of full screen");
        let r = regions(&ctx).expect("published");
        assert!(r.fullscreen);
        assert_eq!(r.grip, 0, "no resize edges in full screen");
        let at = |p: Pos2| (p.x as i32, p.y as i32);
        let (x, y) = at(exit.center());
        assert_eq!(r.hit(x, y), Hit::Client);
        let (x, y) = at(close.center());
        assert_eq!(r.hit(x, y), Hit::Close);
        assert_eq!(r.hit(400, 16), Hit::Caption);
        assert_eq!(r.hit(0, 300), Hit::Client);
        assert_eq!(r.hit(400, 0), Hit::Caption);
    }

    #[test]
    fn a_bad_buttons_side_is_reported() {
        let (out, _, ctx, _) = bar_with(
            r#"
            $extends: "pop-dark"
            title-bar:
              buttons-side: {$type: "string", $value: "left"}
            "#,
            None,
        );
        let kit = Kit::of(&ctx);
        assert_eq!(kit.window.buttons_side, ButtonsSide::End);
        assert_eq!(
            kit.problems,
            ["title-bar.buttons-side: `left` is not start or end"]
        );
        assert_eq!(out.layout.close.unwrap().right(), 800.0);
    }

    #[test]
    fn grouped_buttons_hover_together() {
        // A close glyph shown only on hover. With `group`, the pointer
        // over minimize draws the three hovered: the close glyph shows.
        let json = r##"
                   $extends: "pop-dark"
                   title-bar:
                     buttons-side: {$type: "string", $value: "start"}
                     buttons-hover: {$type: "string", $value: "group"}
                   window-close:
                     rest:
                       text: {$type: "color", $value: "#00000000"}
                     hover:
                       text: {$type: "color", $value: "#ff00ff"}
                   "##;
        let magenta = Color32::from_rgb(255, 0, 255);
        let close_glyph = |pointer| {
            let (out, _, ctx, shapes) = bar_with(json, pointer);
            assert_eq!(Kit::of(&ctx).window.buttons_hover, ButtonsHover::Group);
            let shown = shapes.iter().any(|s| match &s.shape {
                egui::Shape::LineSegment { stroke, .. } => stroke.color == magenta,
                _ => false,
            });
            (out, shown)
        };
        let (out, shown) = close_glyph(None);
        assert!(!shown);
        let over_minimize = out.layout.minimize.unwrap().center();
        assert!(close_glyph(Some(over_minimize)).1);
        assert!(!close_glyph(Some(pos2(400.0, 300.0))).1);
    }

    #[test]
    fn rectangles_round_to_the_pixels_they_cover() {
        let r = RectPx::from_points(Rect::from_min_max(pos2(0.2, 1.0), pos2(10.4, 2.0)), 1.25);
        assert_eq!(
            r,
            RectPx {
                x: 0,
                y: 1,
                w: 13,
                h: 2
            }
        );
        assert!(r.contains(0, 1) && !r.contains(13, 1));
    }

    fn floating() -> Placement {
        Placement {
            maximized: Some(false),
            fullscreen: Some(false),
            stuck: Stuck::default(),
        }
    }

    #[test]
    fn corners_round_only_while_the_window_floats_free() {
        let r = egui::CornerRadius::same(8);
        assert_eq!(corner_radius(8.0, &floating()), r);
        // No radius: square.
        assert_eq!(corner_radius(0.0, &floating()), egui::CornerRadius::ZERO);
        // Maximized, full screen, or a state not told: square.
        for p in [
            Placement {
                maximized: Some(true),
                ..floating()
            },
            Placement {
                fullscreen: Some(true),
                ..floating()
            },
            Placement {
                maximized: None,
                ..floating()
            },
            Placement {
                fullscreen: None,
                ..floating()
            },
        ] {
            assert_eq!(corner_radius(8.0, &p), egui::CornerRadius::ZERO, "{p:?}");
        }
        // Tiled against the left edge: the left corners square.
        let left = Placement {
            stuck: Stuck {
                left: true,
                ..Stuck::default()
            },
            ..floating()
        };
        assert_eq!(
            corner_radius(8.0, &left),
            egui::CornerRadius {
                nw: 0,
                ne: 8,
                sw: 0,
                se: 8
            }
        );
        // In a quarter, top right: only the bottom left corner rounds.
        let quarter = Placement {
            stuck: Stuck {
                top: true,
                right: true,
                ..Stuck::default()
            },
            ..floating()
        };
        assert_eq!(
            corner_radius(8.0, &quarter),
            egui::CornerRadius {
                nw: 0,
                ne: 0,
                sw: 8,
                se: 0
            }
        );
    }

    #[test]
    fn the_sides_against_a_screen_are_read_on_the_screen_it_is_on() {
        let screens = [
            Rect::from_min_size(pos2(0.0, 0.0), vec2(1920.0, 1080.0)),
            Rect::from_min_size(pos2(1920.0, 0.0), vec2(1280.0, 1024.0)),
        ];
        // The left half of the second screen.
        let half = Rect::from_min_size(pos2(1920.0, 0.0), vec2(640.0, 1024.0));
        assert_eq!(
            Stuck::against(half, &screens),
            Stuck {
                left: true,
                top: true,
                right: false,
                bottom: true
            }
        );
        // Free in the middle of the first.
        let free = Rect::from_min_size(pos2(200.0, 100.0), vec2(800.0, 600.0));
        assert_eq!(Stuck::against(free, &screens), Stuck::default());
        // No screens told (Wayland gives no position either): free.
        assert_eq!(Stuck::against(half, &[]), Stuck::default());
    }

    #[test]
    fn a_background_over_a_corner_is_rounded_and_cut_to_the_window() {
        let window = Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0));
        let corners = egui::CornerRadius::same(8);
        // A panel's fill reaching past the window's top left corner.
        let mut fill = egui::Shape::rect_filled(
            Rect::from_min_max(pos2(-1.0, -1.0), pos2(100.0, 300.0)),
            0.0,
            Color32::RED,
        );
        round_shape(&mut fill, window, corners);
        let egui::Shape::Rect(r) = &fill else {
            unreachable!()
        };
        assert_eq!(
            r.corner_radius,
            egui::CornerRadius {
                nw: 8,
                ne: 0,
                sw: 8,
                se: 0
            }
        );
        assert_eq!(r.rect.min, Pos2::ZERO);
        // The shell inside a 1 pt line: one less, so it stays inside the
        // line's curve.
        let mut shell = egui::Shape::rect_filled(window.shrink(1.0), 0.0, Color32::RED);
        round_shape(&mut shell, window, corners);
        let egui::Shape::Rect(r) = &shell else {
            unreachable!()
        };
        assert_eq!(r.corner_radius, egui::CornerRadius::same(7));
        // A button in the middle is left alone, also in a list.
        let mut inner = egui::Shape::Vec(vec![egui::Shape::rect_filled(
            Rect::from_min_size(pos2(50.0, 50.0), vec2(20.0, 20.0)),
            2.0,
            Color32::RED,
        )]);
        let before = inner.clone();
        round_shape(&mut inner, window, corners);
        assert_eq!(format!("{inner:?}"), format!("{before:?}"));
    }
}
