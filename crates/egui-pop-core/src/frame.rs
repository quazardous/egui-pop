//! The window's own frame, for windows drawn without the system's
//! decorations: a title bar (a caption to drag, double click to
//! maximize, the minimize, maximize and close buttons, room for a logo
//! and for the application's items) and resize edges along the window's
//! sides, never thinner than [`MIN_GRIP_PX`] physical pixels.
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
    Color32, Context, CursorIcon, Id, Pos2, Rect, ResizeDirection, Sense, Stroke, Ui, Vec2,
    ViewportCommand, ViewportId, WidgetType, pos2, vec2,
};
use serde::Serialize;

use crate::a11y;
use crate::state::lock;
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
        let id = Id::new("egui_pop_core::frame::table");
        if let Some(t) = ctx.data(|d| d.get_temp::<HitTable>(id)) {
            return t;
        }
        let t = HitTable::default();
        ctx.data_mut(|d| d.insert_temp(id, t.clone()));
        t
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
/// published by [`publish`].
fn pending_id(ctx: &Context) -> Id {
    Id::new(("egui_pop_core::frame::pending", ctx.viewport_id()))
}

fn update_pending(ctx: &Context, f: impl FnOnce(&mut FrameLayout)) {
    let id = pending_id(ctx);
    let mut layout = ctx
        .data(|d| d.get_temp::<FrameLayout>(id))
        .unwrap_or_default();
    f(&mut layout);
    ctx.data_mut(|d| d.insert_temp(id, layout));
}

/// Publishes the current window's regions from what the frame drew this
/// pass (the title bar and the edges); `TitleBar::show` calls it.
pub fn publish(ctx: &Context) -> HitRegions {
    let id = pending_id(ctx);
    let mut layout = ctx
        .data(|d| d.get_temp::<FrameLayout>(id))
        .unwrap_or_default();
    layout.window = ctx.content_rect();
    if layout.grip == 0.0 {
        layout.grip = Tokens::of(ctx).titlebar.grip;
    }
    layout.resizable = resizable(ctx);
    let regions = HitRegions::measure(&layout, ctx.pixels_per_point());
    HitTable::of(ctx).publish(ctx.viewport_id(), regions.clone());
    regions
}

/// The current window's published regions.
pub fn regions(ctx: &Context) -> Option<Arc<HitRegions>> {
    HitTable::of(ctx).get(ctx.viewport_id())
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
    Tokens::of(ctx)
        .titlebar
        .grip
        .max(MIN_GRIP_PX / ctx.pixels_per_point())
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
    }
    Some(direction)
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
}

/// What [`TitleBar::show`] gives back.
#[derive(Clone, Debug, PartialEq)]
pub struct TitleBarOutput {
    pub request: Option<FrameRequest>,
    /// The bar.
    pub rect: Rect,
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
}

impl<'a> TitleBar<'a> {
    /// A bar showing `title`, with the three buttons.
    pub fn new(title: &'a str) -> Self {
        Self {
            title,
            buttons: [true; 3],
            publish: true,
        }
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
        let height = t.titlebar.height;
        let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        let full = ui.max_rect();
        let bar = Rect::from_min_size(full.min, vec2(full.width(), height));
        ui.painter().rect_filled(bar, 0.0, t.titlebar.background);
        // The caption: drag, double click, right click. Widgets on the
        // bar are drawn after it and take their clicks first.
        let drag = ui.interact(
            bar,
            ui.id().with("egui_pop.caption"),
            Sense::click_and_drag(),
        );
        let mut request = None;
        if drag.double_clicked() {
            request = Some(FrameRequest::ToggleMaximize);
        } else if drag.drag_started() {
            ctx.send_viewport_cmd(ViewportCommand::StartDrag);
        } else if drag.secondary_clicked() {
            let at = drag.interact_pointer_pos().unwrap_or(bar.left_bottom());
            request = Some(FrameRequest::SystemMenu(at));
        }
        let mut layout = FrameLayout {
            caption: Some(bar),
            ..FrameLayout::default()
        };
        // The buttons, from the end edge.
        let mut x = bar.right();
        let w = t.titlebar.button_width;
        for (which, shown) in [Button::Close, Button::Maximize, Button::Minimize]
            .into_iter()
            .zip([self.buttons[2], self.buttons[1], self.buttons[0]])
        {
            if !shown {
                continue;
            }
            let r = Rect::from_min_max(pos2(x - w, bar.top()), pos2(x, bar.bottom()));
            x -= w;
            if window_button(ui, r, which, maximized) {
                request = Some(match which {
                    Button::Minimize => FrameRequest::Minimize,
                    Button::Maximize => FrameRequest::ToggleMaximize,
                    Button::Close => FrameRequest::Close,
                });
            }
            match which {
                Button::Minimize => layout.minimize = Some(r),
                Button::Maximize => layout.maximize = Some(r),
                Button::Close => layout.close = Some(r),
            }
        }
        let gap = t.space.m;
        // The application's items, before the buttons.
        let end = Rect::from_min_max(pos2(bar.left(), bar.top()), pos2(x - gap, bar.bottom()));
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
        // Leading items, from the start edge.
        let start = Rect::from_min_max(
            pos2(bar.left() + gap, bar.top()),
            pos2(trailing_rect.left().min(x) - gap, bar.bottom()),
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
        // The title, centred on the bar when there is room, between the
        // items otherwise. A label (assistive technology and tests read
        // it) that takes no click, so the bar still drags under it.
        let room = Rect::from_min_max(
            pos2(leading_rect.right().max(bar.left()) + gap, bar.top()),
            pos2(trailing_rect.left().min(x) - gap, bar.bottom()),
        );
        if room.width() > 0.0 {
            let galley =
                egui::WidgetText::from(egui::RichText::new(self.title).color(t.titlebar.text))
                    .into_galley(
                        ui,
                        Some(egui::TextWrapMode::Truncate),
                        room.width(),
                        egui::TextStyle::Body,
                    );
            let size = galley.size();
            let mut at = bar.center().x - size.x / 2.0;
            at = at.clamp(room.left(), (room.right() - size.x).max(room.left()));
            let r = Rect::from_min_size(pos2(at, bar.center().y - size.y / 2.0), size);
            ui.put(
                r,
                egui::Label::new(egui::RichText::new(self.title).color(t.titlebar.text))
                    .truncate()
                    .selectable(false)
                    .sense(Sense::hover()),
            );
        }
        if !self.publish {
            ui.advance_cursor_after_rect(bar);
            return TitleBarOutput {
                request,
                rect: bar,
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
            layout,
        }
    }
}

/// Draws a window button; says whether it was clicked.
fn window_button(ui: &mut Ui, rect: Rect, which: Button, maximized: bool) -> bool {
    let name = match which {
        Button::Minimize => "Minimize",
        Button::Maximize if maximized => "Restore",
        Button::Maximize => "Maximize",
        Button::Close => "Close",
    };
    let id = ui.id().with(("egui_pop.window_button", which as u8));
    let response = ui.interact(rect, id, Sense::click());
    a11y::name(&response, WidgetType::Button, name);
    let t = Tokens::of(ui.ctx());
    let hot = response.hovered() || response.is_pointer_button_down_on();
    let (fill, ink) = match (which, hot) {
        (Button::Close, true) => (t.titlebar.close_hover, t.titlebar.close_text),
        (_, true) => (t.titlebar.button_hover, t.titlebar.text),
        (_, false) => (Color32::TRANSPARENT, t.titlebar.text_muted),
    };
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, fill);
    // Glyphs a multiple of 4 points wide, on the pixel grid.
    let c = rect.center();
    let c = pos2(
        painter.round_to_pixel_center(c.x),
        painter.round_to_pixel_center(c.y),
    );
    let s = 5.0;
    let stroke = Stroke::new(crate::scale::stroke(ui.ctx(), 1.0), ink);
    match which {
        Button::Minimize => {
            painter.line_segment([pos2(c.x - s, c.y), pos2(c.x + s, c.y)], stroke);
        }
        Button::Maximize if maximized => {
            let back =
                Rect::from_min_size(pos2(c.x - s + 2.0, c.y - s), Vec2::splat(2.0 * s - 2.0));
            let front =
                Rect::from_min_size(pos2(c.x - s, c.y - s + 2.0), Vec2::splat(2.0 * s - 2.0));
            painter.rect_stroke(back, 0.0, stroke, egui::StrokeKind::Inside);
            painter.rect_filled(front, 0.0, if hot { fill } else { t.titlebar.background });
            painter.rect_stroke(front, 0.0, stroke, egui::StrokeKind::Inside);
        }
        Button::Maximize => {
            let r = Rect::from_center_size(c, Vec2::splat(2.0 * s));
            painter.rect_stroke(r, 0.0, stroke, egui::StrokeKind::Inside);
        }
        Button::Close => {
            painter.line_segment([pos2(c.x - s, c.y - s), pos2(c.x + s, c.y + s)], stroke);
            painter.line_segment([pos2(c.x + s, c.y - s), pos2(c.x - s, c.y + s)], stroke);
        }
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
}
