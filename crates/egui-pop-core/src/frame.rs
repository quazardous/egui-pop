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
    Color32, Context, CursorIcon, Pos2, Rect, ResizeDirection, Sense, Ui, Vec2, ViewportCommand,
    ViewportId, WidgetType, pos2, vec2,
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
/// (`window.rest.border-width`), or none: maximized and full screen
/// windows have no edges to draw.
pub fn outline(ctx: &Context) -> egui::Stroke {
    if resizable(ctx) {
        crate::kit::Kit::of(ctx).window.outline
    } else {
        egui::Stroke::NONE
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
        // screen the bar neither moves the window nor maximizes it.
        let drag = ui.interact(
            bar,
            ui.id().with("egui_pop.caption"),
            Sense::click_and_drag(),
        );
        let mut request = None;
        if drag.double_clicked() {
            if !fullscreen {
                request = Some(FrameRequest::ToggleMaximize);
            }
        } else if drag.drag_started() {
            if !fullscreen {
                ctx.send_viewport_cmd(ViewportCommand::StartDrag);
            }
        } else if drag.secondary_clicked() {
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
            ui.interact(at, ui.id().with("egui_pop.title"), Sense::hover())
                .widget_info(|| egui::WidgetInfo::labeled(WidgetType::Label, true, title));
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
    let id = ui.id().with(("egui_pop.window_button", which as u8));
    let response = ui.interact(rect, id, Sense::click());
    a11y::name(&response, WidgetType::Button, name);
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
}
