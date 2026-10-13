//! Cursors over the viewport: what the active tool shows under the
//! pointer, resolved each time the situation changes (the tool, the keys
//! held, a drag, a handle under the pointer) and drawn crisp at the
//! window's scale.
//!
//! A [`Cursor`] is one of:
//!
//! - a system cursor ([`egui::CursorIcon`]);
//! - one of egui-pop's tool cursors ([`ToolCursor`]: a crosshair, an
//!   eyedropper, a hand, zoom in and out...), drawn as a bitmap at the
//!   cursor size and the window's scale, in the theme's colours, and
//!   handed to the system as a real cursor;
//! - a bitmap the application registered by name ([`register_image`]),
//!   at several sizes, with a dark variant and frames if it wants;
//! - a brush outline ([`BrushOutline`]) painted over the image, sized in
//!   image pixels so it follows the zoom, the system cursor hidden.
//!
//! The viewport asks [`default_cursor`] for a [`CursorQuery`] (Alt turns
//! zoom in into zoom out, a hand closes while dragging), then its
//! [`on_cursor`](crate::Viewport::on_cursor) hook, which may give
//! another cursor (`egui-pop-imaging` runs it as the `cursor.resolve`
//! step). [`shown`] then decides what reaches the
//! screen, under the [`CursorSettings`]: painted cursors may be turned
//! off (system cursors only), and the cursor size follows the system's.
//!
//! Bitmap cursors go through `egui::Context::set_cursor_image`, which
//! eframe's native integration hands to winit as a custom cursor; where
//! it cannot (the web, immediate viewports), the system cursor given as
//! the fallback shows instead.

use std::collections::HashMap;
use std::f32::consts::PI;
use std::sync::Arc;

use egui::{Color32, Context, CursorIcon, CustomCursorImage, Id, Modifiers, Painter, Pos2, Stroke};
use serde_json::{Value as Json, json};

use crate::overlay::HandleKind;
use crate::tokens::ViewportTokens;

/// The cursor size when the system says nothing, in points.
pub const DEFAULT_SIZE: f32 = 24.0;

/// Below this radius on screen (points), a brush outline gives way to a
/// crosshair: too small to read, it would hide the point painted.
pub const MIN_OUTLINE_RADIUS: f32 = 4.0;

/// The side of the grid the built-in cursors are drawn on.
const GRID: f32 = 32.0;

/// egui-pop's tool cursors, drawn in the icon set's line style.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ToolCursor {
    /// A precise point.
    Crosshair,
    /// An eyedropper: pick a value from the image.
    Picker,
    /// An open hand: the view can be dragged.
    Hand,
    /// A closed hand: the view (or a shape) is being dragged.
    Grab,
    ZoomIn,
    ZoomOut,
    /// Four arrows: move a shape.
    Move,
    /// A turning arrow: rotate a shape.
    Rotate,
    /// A paintbrush.
    Brush,
    /// An eraser.
    Eraser,
    /// A ring with a plus: add to a mask.
    MaskAdd,
    /// A ring with a minus: subtract from a mask.
    MaskSubtract,
    /// The tool cannot act here.
    NotAllowed,
}

impl ToolCursor {
    /// Every tool cursor (the gallery shows them).
    pub const ALL: &'static [ToolCursor] = &[
        ToolCursor::Crosshair,
        ToolCursor::Picker,
        ToolCursor::Hand,
        ToolCursor::Grab,
        ToolCursor::ZoomIn,
        ToolCursor::ZoomOut,
        ToolCursor::Move,
        ToolCursor::Rotate,
        ToolCursor::Brush,
        ToolCursor::Eraser,
        ToolCursor::MaskAdd,
        ToolCursor::MaskSubtract,
        ToolCursor::NotAllowed,
    ];

    /// Its name in kebab case (`zoom-in`).
    pub fn name(self) -> &'static str {
        match self {
            ToolCursor::Crosshair => "crosshair",
            ToolCursor::Picker => "picker",
            ToolCursor::Hand => "hand",
            ToolCursor::Grab => "grab",
            ToolCursor::ZoomIn => "zoom-in",
            ToolCursor::ZoomOut => "zoom-out",
            ToolCursor::Move => "move",
            ToolCursor::Rotate => "rotate",
            ToolCursor::Brush => "brush",
            ToolCursor::Eraser => "eraser",
            ToolCursor::MaskAdd => "mask-add",
            ToolCursor::MaskSubtract => "mask-subtract",
            ToolCursor::NotAllowed => "not-allowed",
        }
    }

    /// The tool cursor of a name, as [`ToolCursor::name`] gives it.
    pub fn by_name(name: &str) -> Option<ToolCursor> {
        ToolCursor::ALL.iter().copied().find(|t| t.name() == name)
    }

    /// The system cursor shown instead when painted cursors are off, or
    /// where bitmap cursors cannot be shown.
    pub fn system(self) -> CursorIcon {
        match self {
            ToolCursor::Hand => CursorIcon::Grab,
            ToolCursor::Grab => CursorIcon::Grabbing,
            ToolCursor::ZoomIn => CursorIcon::ZoomIn,
            ToolCursor::ZoomOut => CursorIcon::ZoomOut,
            ToolCursor::Move => CursorIcon::Move,
            ToolCursor::NotAllowed => CursorIcon::NotAllowed,
            ToolCursor::Crosshair
            | ToolCursor::Picker
            | ToolCursor::Rotate
            | ToolCursor::Brush
            | ToolCursor::Eraser
            | ToolCursor::MaskAdd
            | ToolCursor::MaskSubtract => CursorIcon::Crosshair,
        }
    }

    /// Its hot spot, as a fraction of the cursor's side.
    pub fn hotspot(self) -> [f32; 2] {
        let h = shape(self).1;
        [h[0] / GRID, h[1] / GRID]
    }
}

/// What a brush outline carries beside the pointer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BrushMark {
    #[default]
    None,
    /// A plus: the brush adds (to a mask).
    Add,
    /// A minus: the brush subtracts.
    Subtract,
    /// A cross: the brush erases.
    Erase,
}

impl BrushMark {
    pub fn name(self) -> &'static str {
        match self {
            BrushMark::None => "none",
            BrushMark::Add => "add",
            BrushMark::Subtract => "subtract",
            BrushMark::Erase => "erase",
        }
    }

    fn by_name(name: &str) -> BrushMark {
        match name {
            "add" => BrushMark::Add,
            "subtract" => BrushMark::Subtract,
            "erase" => BrushMark::Erase,
            _ => BrushMark::None,
        }
    }
}

/// A brush outline: a ring of the brush's radius, in image pixels, so it
/// follows the zoom, and an inner ring where its hardness ends (the
/// feather between them).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BrushOutline {
    /// The brush's radius, in image pixels.
    pub radius: f64,
    /// The share of the radius painted at full strength, 0 to 1; the
    /// inner ring is drawn below 1.
    pub hardness: f32,
    pub mark: BrushMark,
}

impl BrushOutline {
    /// Its radius on screen, in points, at a zoom (physical pixels per
    /// image pixel) on a window of `ppp` physical pixels per point.
    pub fn screen_radius(&self, zoom: f64, ppp: f32) -> f32 {
        (self.radius * zoom / f64::from(ppp.max(1e-3))) as f32
    }
}

/// A cursor.
#[derive(Clone, Debug, PartialEq)]
pub enum Cursor {
    /// One of the system's cursors.
    System(CursorIcon),
    /// One of egui-pop's tool cursors.
    Tool(ToolCursor),
    /// A bitmap the application registered by name ([`register_image`]).
    Image(String),
    /// A brush outline painted over the image, the system cursor hidden.
    Brush(BrushOutline),
}

impl Default for Cursor {
    fn default() -> Self {
        Cursor::System(CursorIcon::Default)
    }
}

impl Cursor {
    /// A short name for traces and the debug bridge (`tool:zoom-in`).
    pub fn name(&self) -> String {
        match self {
            Cursor::System(i) => format!("system:{i:?}"),
            Cursor::Tool(t) => format!("tool:{}", t.name()),
            Cursor::Image(n) => format!("image:{n}"),
            Cursor::Brush(b) => format!("brush:{:.1}", b.radius),
        }
    }

    /// As JSON: `{"system": "Crosshair"}`, `{"tool": "zoom-in"}`,
    /// `{"image": "lasso"}` or `{"brush": {"radius", "hardness", "mark"}}`.
    pub fn to_json(&self) -> Json {
        match self {
            Cursor::System(i) => json!({ "system": format!("{i:?}") }),
            Cursor::Tool(t) => json!({ "tool": t.name() }),
            Cursor::Image(n) => json!({ "image": n }),
            Cursor::Brush(b) => json!({ "brush": {
                "radius": b.radius,
                "hardness": b.hardness,
                "mark": b.mark.name(),
            }}),
        }
    }

    /// From [`Cursor::to_json`]'s form; `None` when it is not one.
    pub fn from_json(v: &Json) -> Option<Cursor> {
        if let Some(s) = v["system"].as_str() {
            return CursorIcon::ALL
                .iter()
                .find(|i| format!("{i:?}") == s)
                .map(|i| Cursor::System(*i));
        }
        if let Some(s) = v["tool"].as_str() {
            return ToolCursor::by_name(s).map(Cursor::Tool);
        }
        if let Some(s) = v["image"].as_str() {
            return Some(Cursor::Image(s.to_string()));
        }
        let b = &v["brush"];
        let radius = b["radius"].as_f64()?;
        Some(Cursor::Brush(BrushOutline {
            radius,
            hardness: b["hardness"].as_f64().unwrap_or(1.0) as f32,
            mark: BrushMark::by_name(b["mark"].as_str().unwrap_or("none")),
        }))
    }
}

/// A brush tool's size and hardness.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Brush {
    /// The radius, in image pixels.
    pub radius: f64,
    /// The share of the radius at full strength, 0 to 1.
    pub hardness: f32,
}

impl Brush {
    pub fn new(radius: f64, hardness: f32) -> Brush {
        Brush {
            radius: radius.max(0.0),
            hardness: hardness.clamp(0.0, 1.0),
        }
    }

    fn outline(self, mark: BrushMark) -> BrushOutline {
        BrushOutline {
            radius: self.radius,
            hardness: self.hardness,
            mark,
        }
    }
}

/// The tool active over a viewport, which decides its cursor.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Tool {
    /// No tool: the system's arrow (a hand while panning, a move cursor
    /// over a handle).
    #[default]
    None,
    /// Drag the view: an open hand, closed while dragging.
    Pan,
    /// Zoom in where clicked: Alt zooms out.
    Zoom,
    /// Pick a value: an eyedropper.
    Picker,
    /// Place a point: a crosshair.
    Crosshair,
    /// Move a shape.
    Move,
    /// Rotate a shape.
    Rotate,
    /// Paint: the brush's outline.
    Brush(Brush),
    /// Erase: the brush's outline, marked with a cross.
    Eraser(Brush),
    /// Paint a mask: the brush's outline marked with a plus, a minus
    /// with Alt.
    Mask(Brush),
    /// The tool cannot act here (a locked layer).
    Disabled,
    /// A tool of the application's own, by name: its cursor comes from
    /// the `on_cursor` hook (the system's arrow by default).
    Named(String),
}

impl Tool {
    /// Its name (`zoom`, `brush`, or a named tool's name).
    pub fn name(&self) -> &str {
        match self {
            Tool::None => "none",
            Tool::Pan => "pan",
            Tool::Zoom => "zoom",
            Tool::Picker => "picker",
            Tool::Crosshair => "crosshair",
            Tool::Move => "move",
            Tool::Rotate => "rotate",
            Tool::Brush(_) => "brush",
            Tool::Eraser(_) => "eraser",
            Tool::Mask(_) => "mask",
            Tool::Disabled => "disabled",
            Tool::Named(n) => n,
        }
    }

    /// Its brush, for the tools that have one.
    pub fn brush(&self) -> Option<Brush> {
        match self {
            Tool::Brush(b) | Tool::Eraser(b) | Tool::Mask(b) => Some(*b),
            _ => None,
        }
    }
}

/// What a viewport knows when it asks which cursor to show. It asks
/// again only when this changes.
#[derive(Clone, Debug, PartialEq)]
pub struct CursorQuery {
    pub tool: Tool,
    /// The keys held.
    pub modifiers: Modifiers,
    /// A button is held down on the viewport (a drag of the tool, of the
    /// view or of a handle).
    pub dragging: bool,
    /// The view is being panned, or the pan key is held.
    pub panning: bool,
    /// The handle under the pointer, or the one dragged.
    pub handle: Option<HandleKind>,
    /// The pointer is over the image (not the background around it).
    pub over_image: bool,
}

impl CursorQuery {
    /// A query for a tool, nothing held.
    pub fn new(tool: Tool) -> CursorQuery {
        CursorQuery {
            tool,
            modifiers: Modifiers::NONE,
            dragging: false,
            panning: false,
            handle: None,
            over_image: true,
        }
    }

    /// As JSON, for the `cursor.resolve` step.
    pub fn to_json(&self) -> Json {
        let m = self.modifiers;
        let mut v = json!({
            "tool": self.tool.name(),
            "modifiers": {
                "alt": m.alt,
                "ctrl": m.ctrl,
                "shift": m.shift,
                "command": m.command,
            },
            "dragging": self.dragging,
            "panning": self.panning,
            "handle": self.handle.map(HandleKind::name),
            "over_image": self.over_image,
        });
        if let Some(b) = self.tool.brush() {
            v["brush"] = json!({ "radius": b.radius, "hardness": b.hardness });
        }
        v
    }
}

/// The cursor egui-pop shows for a query, before the application's hook:
///
/// - panning (the pan key held, or a pan drag): an open hand, closed
///   while dragging;
/// - over a handle: a move cursor (a rotate cursor on a rotation
///   handle), a closed hand while dragging it;
/// - else the tool's: zoom in (zoom out with Alt), an eyedropper, a
///   crosshair, a brush outline (mask brushes marked with a plus, a
///   minus with Alt; erasers with a cross)...
pub fn default_cursor(q: &CursorQuery) -> Cursor {
    if q.panning {
        return Cursor::Tool(if q.dragging {
            ToolCursor::Grab
        } else {
            ToolCursor::Hand
        });
    }
    if let Some(kind) = q.handle {
        return Cursor::Tool(match (kind, q.dragging) {
            (HandleKind::Rotate, _) => ToolCursor::Rotate,
            (_, true) => ToolCursor::Grab,
            (_, false) => ToolCursor::Move,
        });
    }
    match &q.tool {
        Tool::None | Tool::Named(_) => Cursor::System(CursorIcon::Default),
        Tool::Pan => Cursor::Tool(if q.dragging {
            ToolCursor::Grab
        } else {
            ToolCursor::Hand
        }),
        Tool::Zoom => Cursor::Tool(if q.modifiers.alt {
            ToolCursor::ZoomOut
        } else {
            ToolCursor::ZoomIn
        }),
        Tool::Picker => Cursor::Tool(ToolCursor::Picker),
        Tool::Crosshair => Cursor::Tool(ToolCursor::Crosshair),
        Tool::Move => Cursor::Tool(if q.dragging {
            ToolCursor::Grab
        } else {
            ToolCursor::Move
        }),
        Tool::Rotate => Cursor::Tool(ToolCursor::Rotate),
        Tool::Brush(b) => Cursor::Brush(b.outline(BrushMark::None)),
        Tool::Eraser(b) => Cursor::Brush(b.outline(BrushMark::Erase)),
        Tool::Mask(b) => Cursor::Brush(b.outline(if q.modifiers.alt {
            BrushMark::Subtract
        } else {
            BrushMark::Add
        })),
        Tool::Disabled => Cursor::Tool(ToolCursor::NotAllowed),
    }
}

/// How cursors are shown in a context ([`set_settings`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CursorSettings {
    /// egui-pop's own cursors (tool bitmaps, registered images, brush
    /// outlines); off, system cursors only.
    pub painted: bool,
    /// The cursor size, in points; `None` follows the system's
    /// ([`system_size`]).
    pub size: Option<f32>,
}

impl Default for CursorSettings {
    fn default() -> Self {
        CursorSettings {
            painted: true,
            size: None,
        }
    }
}

fn settings_id() -> Id {
    Id::new("egui_pop_viewport::cursor::settings")
}

/// The cursor settings of a context.
pub fn settings(ctx: &Context) -> CursorSettings {
    ctx.data(|d| d.get_temp(settings_id())).unwrap_or_default()
}

/// Sets the cursor settings of a context (an application maps its own
/// settings here).
pub fn set_settings(ctx: &Context, settings: CursorSettings) {
    ctx.data_mut(|d| d.insert_temp(settings_id(), settings));
}

/// The system's cursor size, in points: `XCURSOR_SIZE` where it is set
/// (X11 and Wayland desktops), else [`DEFAULT_SIZE`].
pub fn system_size() -> f32 {
    // upstream: winit does not report the cursor size (docs/upstream.md).
    parse_size(std::env::var("XCURSOR_SIZE").ok().as_deref())
}

/// A cursor size from the text of `XCURSOR_SIZE`: 8 to 256, else the
/// default.
pub fn parse_size(text: Option<&str>) -> f32 {
    text.and_then(|s| s.trim().parse::<f32>().ok())
        .filter(|s| (8.0..=256.0).contains(s))
        .unwrap_or(DEFAULT_SIZE)
}

/// The cursor size in use in a context, in points.
pub fn size(ctx: &Context) -> f32 {
    settings(ctx)
        .size
        .unwrap_or_else(system_size)
        .clamp(8.0, 256.0)
}

/// A bitmap of a cursor: straight (not premultiplied) RGBA.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bitmap {
    pub rgba: Arc<[u8]>,
    /// Width and height, in physical pixels.
    pub size: [u16; 2],
    /// The hot spot, in physical pixels from the top left.
    pub hotspot: [u16; 2],
}

impl Bitmap {
    /// A bitmap, or `None` when the pixels do not fill `size` or the hot
    /// spot falls outside (or the side is over winit's 2048).
    pub fn new(rgba: impl Into<Arc<[u8]>>, size: [u16; 2], hotspot: [u16; 2]) -> Option<Bitmap> {
        let rgba = rgba.into();
        let ok = rgba.len() == usize::from(size[0]) * usize::from(size[1]) * 4
            && size[0] > 0
            && size[1] > 0
            && size[0] <= 2048
            && size[1] <= 2048
            && hotspot[0] < size[0]
            && hotspot[1] < size[1];
        ok.then_some(Bitmap {
            rgba,
            size,
            hotspot,
        })
    }

    fn image(&self) -> CustomCursorImage {
        CustomCursorImage {
            rgba: self.rgba.clone(),
            size: self.size,
            hotspot: self.hotspot,
        }
    }
}

/// A cursor of the application's: bitmaps at several sizes (give one per
/// scale used: 1x, 1.5x, 2x...), a dark theme's variant, more frames
/// for an animation, and the system cursor shown where bitmaps cannot be.
#[derive(Clone, Debug, PartialEq)]
pub struct CursorImage {
    pub sizes: Vec<Bitmap>,
    /// The bitmaps for a dark theme (empty: `sizes` in every theme).
    pub dark: Vec<Bitmap>,
    /// The frames after the first (`sizes`) of an animated cursor, each
    /// at its sizes; shown only when motion is not reduced.
    pub frames: Vec<Vec<Bitmap>>,
    /// Seconds per frame.
    pub frame_time: f32,
    pub fallback: CursorIcon,
}

impl CursorImage {
    /// An image with no bitmap yet, falling back to `fallback`.
    pub fn new(fallback: CursorIcon) -> CursorImage {
        CursorImage {
            sizes: Vec::new(),
            dark: Vec::new(),
            frames: Vec::new(),
            frame_time: 0.1,
            fallback,
        }
    }

    /// Adds a size.
    pub fn size(mut self, bitmap: Bitmap) -> CursorImage {
        self.sizes.push(bitmap);
        self
    }

    /// Adds a size of the dark theme's variant.
    pub fn dark(mut self, bitmap: Bitmap) -> CursorImage {
        self.dark.push(bitmap);
        self
    }

    /// Adds a frame (its sizes) to an animation.
    pub fn frame(mut self, sizes: Vec<Bitmap>) -> CursorImage {
        self.frames.push(sizes);
        self
    }

    /// The number of frames (1 for a still cursor).
    pub fn frame_count(&self) -> usize {
        1 + self.frames.len()
    }

    /// The bitmap for a side in physical pixels: the smallest at least
    /// that wide, else the largest; in a dark theme the dark variant
    /// when there is one; frame `frame` of an animation.
    pub fn pick(&self, side: u32, dark: bool, frame: usize) -> Option<&Bitmap> {
        let set = match frame % self.frame_count() {
            0 if dark && !self.dark.is_empty() => &self.dark,
            0 => &self.sizes,
            i => &self.frames[i - 1],
        };
        set.iter()
            .filter(|b| u32::from(b.size[0]) >= side)
            .min_by_key(|b| b.size[0])
            .or_else(|| set.iter().max_by_key(|b| b.size[0]))
    }
}

fn images_id() -> Id {
    Id::new("egui_pop_viewport::cursor::images")
}

/// Registers a cursor image under a name, for [`Cursor::Image`].
pub fn register_image(ctx: &Context, name: impl Into<String>, image: CursorImage) {
    let name = name.into();
    ctx.data_mut(|d| {
        d.get_temp_mut_or_default::<HashMap<String, CursorImage>>(images_id())
            .insert(name, image);
    });
}

/// The cursor image registered under a name.
pub fn image(ctx: &Context, name: &str) -> Option<CursorImage> {
    ctx.data(|d| {
        d.get_temp::<HashMap<String, CursorImage>>(images_id())
            .and_then(|m| m.get(name).cloned())
    })
}

/// What reaches the screen for a cursor.
#[derive(Clone, Debug, PartialEq)]
pub enum Shown {
    /// A system cursor.
    System(CursorIcon),
    /// A tool cursor's bitmap.
    Tool(ToolCursor),
    /// A registered image's bitmap.
    Image(String),
    /// A brush outline of `radius` points (its hardness ring at `inner`
    /// points, 0 for none), the system cursor hidden.
    Outline {
        radius: f32,
        inner: f32,
        mark: BrushMark,
    },
}

/// What a cursor shows in a context at a zoom (physical pixels per image
/// pixel) and scale: with painted cursors off, the system cursor
/// standing for it; a brush outline under [`MIN_OUTLINE_RADIUS`], a
/// crosshair; an image nobody registered, its name's system arrow.
pub fn shown(ctx: &Context, cursor: &Cursor, zoom: f64, ppp: f32) -> Shown {
    let painted = settings(ctx).painted;
    match cursor {
        Cursor::System(i) => Shown::System(*i),
        Cursor::Tool(t) if painted => Shown::Tool(*t),
        Cursor::Tool(t) => Shown::System(t.system()),
        Cursor::Image(name) => match image(ctx, name) {
            Some(_) if painted => Shown::Image(name.clone()),
            Some(img) => Shown::System(img.fallback),
            None => Shown::System(CursorIcon::Default),
        },
        Cursor::Brush(b) => {
            let radius = b.screen_radius(zoom, ppp);
            if !painted {
                Shown::System(CursorIcon::Crosshair)
            } else if radius < MIN_OUTLINE_RADIUS {
                Shown::Tool(ToolCursor::Crosshair)
            } else {
                let inner = radius * b.hardness.clamp(0.0, 1.0);
                let inner = if b.hardness < 0.98 && inner >= 2.0 {
                    inner
                } else {
                    0.0
                };
                Shown::Outline {
                    radius,
                    inner,
                    mark: b.mark,
                }
            }
        }
    }
}

/// Shows a cursor for this frame at `pos`: sets the system cursor, hands
/// a bitmap to the integration, or paints an outline with `painter`.
/// Returns whether a bitmap was set: the caller clears it
/// ([`clear_image`]) once the pointer leaves, as egui keeps it from frame
/// to frame.
pub fn apply(ctx: &Context, painter: &Painter, pos: Pos2, shown: &Shown) -> bool {
    match shown {
        Shown::System(i) => {
            if *i != CursorIcon::Default {
                ctx.set_cursor_icon(*i);
            }
            false
        }
        Shown::Tool(t) => {
            // upstream: the bitmap is dropped on the web and in immediate
            // viewports; the system cursor below stands in there
            // (docs/upstream.md).
            ctx.set_cursor_image(Some(tool_image(ctx, *t)));
            ctx.set_cursor_icon(t.system());
            true
        }
        Shown::Image(name) => {
            let Some(img) = image(ctx, name) else {
                return false;
            };
            let side = (size(ctx) * ctx.pixels_per_point()).round() as u32;
            let tokens = egui_pop_core::theme::Tokens::of(ctx);
            let dark = tokens.dark_mode();
            let reduced = tokens.motion_duration == 0.0;
            let frame = if reduced || img.frame_count() == 1 {
                0
            } else {
                let t = img.frame_time.max(0.01);
                ctx.request_repaint_after(std::time::Duration::from_secs_f32(t));
                (ctx.input(|i| i.time) / f64::from(t)) as usize
            };
            ctx.set_cursor_icon(img.fallback);
            match img.pick(side, dark, frame) {
                Some(b) => {
                    ctx.set_cursor_image(Some(b.image()));
                    true
                }
                None => false,
            }
        }
        Shown::Outline {
            radius,
            inner,
            mark,
        } => {
            ctx.set_cursor_icon(CursorIcon::None);
            paint_outline(painter, pos, *radius, *inner, *mark);
            false
        }
    }
}

/// Clears a bitmap cursor set by [`apply`].
pub fn clear_image(ctx: &Context) {
    ctx.set_cursor_image(None);
}

/// Paints a brush outline: a ring of `radius` points, a lighter one at
/// `inner` (none at 0), a point at the centre and the mark beside it,
/// each a line over a wider one in the outline colour so it reads on any
/// image.
pub fn paint_outline(painter: &Painter, pos: Pos2, radius: f32, inner: f32, mark: BrushMark) {
    let t = ViewportTokens::of(painter.ctx());
    let w = |x: f32| egui_pop_core::scale::stroke(painter.ctx(), x);
    let under = Stroke::new(w(3.0), t.cursor_outline);
    let line = Stroke::new(w(1.0), t.cursor_fill);
    painter.circle_stroke(pos, radius, under);
    painter.circle_stroke(pos, radius, line);
    if inner > 0.0 {
        let faint = Stroke::new(w(1.0), t.cursor_fill.gamma_multiply(0.6));
        painter.circle_stroke(
            pos,
            inner,
            Stroke::new(w(2.0), t.cursor_outline.gamma_multiply(0.6)),
        );
        painter.circle_stroke(pos, inner, faint);
    }
    painter.circle_filled(pos, w(2.5), t.cursor_outline);
    painter.circle_filled(pos, w(1.0), t.cursor_fill);
    let s = 4.0;
    let c = pos + egui::vec2(radius.min(24.0) * 0.5 + 8.0, radius.min(24.0) * 0.5 + 8.0);
    let mut segments: Vec<[Pos2; 2]> = Vec::new();
    match mark {
        BrushMark::None => {}
        BrushMark::Add => {
            segments.push([c - egui::vec2(s, 0.0), c + egui::vec2(s, 0.0)]);
            segments.push([c - egui::vec2(0.0, s), c + egui::vec2(0.0, s)]);
        }
        BrushMark::Subtract => {
            segments.push([c - egui::vec2(s, 0.0), c + egui::vec2(s, 0.0)]);
        }
        BrushMark::Erase => {
            segments.push([c - egui::vec2(s, s) * 0.8, c + egui::vec2(s, s) * 0.8]);
            segments.push([c + egui::vec2(-s, s) * 0.8, c + egui::vec2(s, -s) * 0.8]);
        }
    }
    for seg in &segments {
        painter.line_segment(*seg, Stroke::new(w(4.0), t.cursor_outline));
    }
    for seg in &segments {
        painter.line_segment(*seg, Stroke::new(w(2.0), t.cursor_fill));
    }
}

fn cache_id() -> Id {
    Id::new("egui_pop_viewport::cursor::bitmaps")
}

type CacheKey = (ToolCursor, u32, [u8; 4], [u8; 4]);

/// A tool cursor's bitmap at the context's cursor size and scale, in the
/// theme's colours; drawn once, then kept.
pub fn tool_image(ctx: &Context, tool: ToolCursor) -> CustomCursorImage {
    let side = ((size(ctx) * ctx.pixels_per_point()).round() as u32).clamp(8, 512);
    let t = ViewportTokens::of(ctx);
    let key: CacheKey = (
        tool,
        side,
        t.cursor_fill.to_array(),
        t.cursor_outline.to_array(),
    );
    if let Some(img) = ctx.data(|d| {
        d.get_temp::<HashMap<CacheKey, CustomCursorImage>>(cache_id())
            .and_then(|m| m.get(&key).cloned())
    }) {
        return img;
    }
    let rgba = rasterize(tool, side, t.cursor_fill, t.cursor_outline);
    let h = shape(tool).1;
    let at = |v: f32| ((v / GRID * side as f32).floor() as u32).min(side - 1) as u16;
    let img = CustomCursorImage {
        rgba: rgba.into(),
        size: [side as u16, side as u16],
        hotspot: [at(h[0]), at(h[1])],
    };
    ctx.data_mut(|d| {
        d.get_temp_mut_or_default::<HashMap<CacheKey, CustomCursorImage>>(cache_id())
            .insert(key, img.clone());
    });
    img
}

/// One stroke or fill of a built-in cursor, on a 32-unit grid.
#[derive(Clone, Copy, Debug)]
enum Prim {
    /// A line with round ends: from, to, width.
    Seg([f32; 2], [f32; 2], f32),
    /// A circle's outline: centre, radius, width.
    Ring([f32; 2], f32, f32),
    /// An arc: centre, radius, from and to (radians, clockwise on
    /// screen, `to` after `from`), width.
    Arc([f32; 2], f32, f32, f32, f32),
    /// A filled disc: centre, radius.
    Disc([f32; 2], f32),
    /// A filled polygon.
    Poly(&'static [[f32; 2]]),
}

const UP: &[[f32; 2]] = &[[16.0, 2.0], [11.0, 8.0], [21.0, 8.0]];
const DOWN: &[[f32; 2]] = &[[16.0, 30.0], [11.0, 24.0], [21.0, 24.0]];
const LEFT: &[[f32; 2]] = &[[2.0, 16.0], [8.0, 11.0], [8.0, 21.0]];
const RIGHT: &[[f32; 2]] = &[[30.0, 16.0], [24.0, 11.0], [24.0, 21.0]];
const PALM: &[[f32; 2]] = &[
    [8.0, 17.0],
    [24.0, 17.0],
    [23.0, 25.0],
    [19.0, 29.0],
    [12.0, 29.0],
    [8.0, 24.0],
];
const ROTATE_HEAD: &[[f32; 2]] = &[[20.0, 3.0], [27.0, 7.5], [20.0, 12.0]];
const BRUSH_TIP: &[[f32; 2]] = &[
    [10.0, 17.0],
    [15.0, 22.0],
    [6.0, 30.0],
    [2.0, 30.0],
    [2.0, 26.0],
];

/// A built-in cursor's strokes and its hot spot, on the 32-unit grid.
fn shape(tool: ToolCursor) -> (Vec<Prim>, [f32; 2]) {
    use Prim::{Arc, Disc, Poly, Ring, Seg};
    match tool {
        ToolCursor::Crosshair => (
            vec![
                Seg([16.0, 3.0], [16.0, 12.0], 2.0),
                Seg([16.0, 20.0], [16.0, 29.0], 2.0),
                Seg([3.0, 16.0], [12.0, 16.0], 2.0),
                Seg([20.0, 16.0], [29.0, 16.0], 2.0),
                Disc([16.0, 16.0], 1.2),
            ],
            [16.0, 16.0],
        ),
        ToolCursor::Picker => (
            vec![
                Seg([3.5, 28.5], [18.0, 14.0], 3.0),
                Seg([13.0, 9.0], [23.0, 19.0], 2.5),
                Seg([19.0, 13.0], [25.0, 7.0], 6.0),
                Disc([26.0, 6.0], 4.0),
            ],
            [3.0, 29.0],
        ),
        ToolCursor::Hand => (
            vec![
                Seg([10.0, 9.0], [10.0, 19.0], 3.5),
                Seg([14.0, 5.0], [14.0, 18.0], 3.5),
                Seg([18.0, 5.0], [18.0, 18.0], 3.5),
                Seg([22.0, 8.0], [22.0, 19.0], 3.5),
                Seg([5.5, 15.0], [9.5, 21.0], 3.5),
                Poly(PALM),
            ],
            [16.0, 16.0],
        ),
        ToolCursor::Grab => (
            vec![
                Seg([10.0, 14.0], [10.0, 19.0], 3.5),
                Seg([14.0, 12.0], [14.0, 18.0], 3.5),
                Seg([18.0, 12.0], [18.0, 18.0], 3.5),
                Seg([22.0, 14.0], [22.0, 19.0], 3.5),
                Seg([7.5, 18.0], [9.5, 21.0], 3.5),
                Poly(PALM),
            ],
            [16.0, 16.0],
        ),
        ToolCursor::ZoomIn | ToolCursor::ZoomOut => {
            let mut v = vec![
                Ring([13.0, 13.0], 8.5, 2.5),
                Seg([19.5, 19.5], [28.0, 28.0], 4.0),
                Seg([9.0, 13.0], [17.0, 13.0], 2.0),
            ];
            if tool == ToolCursor::ZoomIn {
                v.push(Seg([13.0, 9.0], [13.0, 17.0], 2.0));
            }
            (v, [13.0, 13.0])
        }
        ToolCursor::Move => (
            vec![
                Seg([16.0, 6.0], [16.0, 26.0], 2.0),
                Seg([6.0, 16.0], [26.0, 16.0], 2.0),
                Poly(UP),
                Poly(DOWN),
                Poly(LEFT),
                Poly(RIGHT),
            ],
            [16.0, 16.0],
        ),
        ToolCursor::Rotate => (
            vec![
                Arc([16.0, 16.0], 10.0, -PI * 0.45, PI * 1.25, 2.5),
                Poly(ROTATE_HEAD),
                Disc([16.0, 16.0], 1.5),
            ],
            [16.0, 16.0],
        ),
        ToolCursor::Brush => (
            vec![
                Seg([28.0, 4.0], [15.0, 17.0], 3.0),
                Seg([16.5, 15.5], [12.5, 19.5], 5.0),
                Poly(BRUSH_TIP),
            ],
            [3.0, 29.0],
        ),
        ToolCursor::Eraser => (
            vec![
                Seg([6.0, 22.0], [18.0, 10.0], 2.0),
                Seg([18.0, 10.0], [26.0, 18.0], 2.0),
                Seg([26.0, 18.0], [14.0, 30.0], 2.0),
                Seg([14.0, 30.0], [6.0, 22.0], 2.0),
                Seg([10.0, 18.0], [18.0, 26.0], 2.0),
            ],
            [9.0, 25.0],
        ),
        ToolCursor::MaskAdd | ToolCursor::MaskSubtract => {
            let mut v = vec![
                Ring([12.0, 12.0], 8.0, 2.0),
                Disc([12.0, 12.0], 1.2),
                Seg([21.0, 26.0], [29.0, 26.0], 2.5),
            ];
            if tool == ToolCursor::MaskAdd {
                v.push(Seg([25.0, 22.0], [25.0, 30.0], 2.5));
            }
            (v, [12.0, 12.0])
        }
        ToolCursor::NotAllowed => (
            vec![
                Ring([16.0, 16.0], 11.0, 3.0),
                Seg([8.5, 23.5], [23.5, 8.5], 3.0),
            ],
            [16.0, 16.0],
        ),
    }
}

fn sub(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn len(v: [f32; 2]) -> f32 {
    v[0].hypot(v[1])
}

fn segment_distance(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let ab = sub(b, a);
    let ap = sub(p, a);
    let l2 = ab[0] * ab[0] + ab[1] * ab[1];
    let t = if l2 > 0.0 {
        ((ap[0] * ab[0] + ap[1] * ab[1]) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    len([ap[0] - ab[0] * t, ap[1] - ab[1] * t])
}

/// The signed distance from `p` to a primitive (negative inside), in
/// grid units.
fn distance(prim: &Prim, p: [f32; 2]) -> f32 {
    match *prim {
        Prim::Seg(a, b, w) => segment_distance(p, a, b) - w / 2.0,
        Prim::Ring(c, r, w) => (len(sub(p, c)) - r).abs() - w / 2.0,
        Prim::Arc(c, r, from, to, w) => {
            let d = sub(p, c);
            let a = d[1].atan2(d[0]);
            let along = from + (a - from).rem_euclid(2.0 * PI);
            if along <= to {
                (len(d) - r).abs() - w / 2.0
            } else {
                let end = |t: f32| [c[0] + r * t.cos(), c[1] + r * t.sin()];
                len(sub(p, end(from))).min(len(sub(p, end(to)))) - w / 2.0
            }
        }
        Prim::Disc(c, r) => len(sub(p, c)) - r,
        Prim::Poly(pts) => {
            let mut d = f32::MAX;
            let mut inside = false;
            for (i, &a) in pts.iter().enumerate() {
                let b = pts[(i + 1) % pts.len()];
                d = d.min(segment_distance(p, a, b));
                if (a[1] > p[1]) != (b[1] > p[1])
                    && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
                {
                    inside = !inside;
                }
            }
            if inside { -d } else { d }
        }
    }
}

/// Draws a tool cursor at `side` physical pixels: its strokes in `fill`,
/// ringed by `outline` (about a pixel at 24, more when larger), with
/// anti-aliased edges. Straight RGBA, row by row.
pub fn rasterize(tool: ToolCursor, side: u32, fill: Color32, outline: Color32) -> Vec<u8> {
    let prims = shape(tool).0;
    let side_f = side as f32;
    let units = GRID / side_f;
    let halo = (side_f / 24.0).max(1.0);
    let mut out = Vec::with_capacity((side * side * 4) as usize);
    for y in 0..side {
        for x in 0..side {
            let p = [(x as f32 + 0.5) * units, (y as f32 + 0.5) * units];
            let d = prims
                .iter()
                .map(|prim| distance(prim, p))
                .fold(f32::MAX, f32::min)
                / units;
            let f = (0.5 - d).clamp(0.0, 1.0);
            let o = (0.5 - (d - halo)).clamp(0.0, 1.0);
            let a = f + (1.0 - f) * o;
            if a <= 0.0 {
                out.extend_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            let mix = |c1: u8, c2: u8| {
                ((f32::from(c1) * f + f32::from(c2) * (1.0 - f) * o) / a)
                    .round()
                    .clamp(0.0, 255.0) as u8
            };
            out.extend_from_slice(&[
                mix(fill.r(), outline.r()),
                mix(fill.g(), outline.g()),
                mix(fill.b(), outline.b()),
                (a * 255.0).round() as u8,
            ]);
        }
    }
    out
}

/// Paints a tool cursor with `painter` in a square (the gallery's
/// samples, a legend): the bitmap the system would show, as a texture.
pub fn paint_sample(ui: &egui::Ui, rect: egui::Rect, tool: ToolCursor) {
    let ppp = ui.ctx().pixels_per_point();
    let side = ((rect.width() * ppp).round() as u32).clamp(8, 512);
    let t = ViewportTokens::of(ui.ctx());
    let id = Id::new(("egui_pop_viewport::cursor::sample", tool, side));
    let key = (t.cursor_fill.to_array(), t.cursor_outline.to_array());
    let texture = ui
        .ctx()
        .data(|d| d.get_temp::<([[u8; 4]; 2], egui::TextureHandle)>(id));
    let handle = match texture {
        Some((k, h)) if k == [key.0, key.1] => h,
        _ => {
            let rgba = rasterize(tool, side, t.cursor_fill, t.cursor_outline);
            let image =
                egui::ColorImage::from_rgba_unmultiplied([side as usize, side as usize], &rgba);
            let h = ui.ctx().load_texture(
                format!("egui-pop-cursor-{}-{side}", tool.name()),
                image,
                egui::TextureOptions::NEAREST,
            );
            ui.ctx()
                .data_mut(|d| d.insert_temp(id, ([key.0, key.1], h.clone())));
            h
        }
    };
    ui.painter().image(
        handle.id(),
        rect,
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        Color32::WHITE,
    );
}
