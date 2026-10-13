# Cursors and the loupe

The image viewport (`egui-pop-viewport`, the facade's `viewport`
feature) shows the cursor of the tool in use, drawn crisp at the
window's scale, and a loupe on the pixels under the pointer.

![the cursors page](../crates/egui-pop-gallery/tests/snapshots/cursors-pop-dark.png)

## Cursors

A `cursor::Cursor` is one of:

| Variant | What it is | How it reaches the screen |
|---|---|---|
| `System(CursorIcon)` | one of egui's system cursors | the system draws it |
| `Tool(ToolCursor)` | one of egui-pop's tool cursors | a bitmap drawn at the cursor size and the window's scale, in the theme's colours, handed to the system |
| `Image(name)` | a bitmap the application registered (`cursor::register_image`) | the size closest to the one wanted, its dark variant in a dark theme |
| `Brush(BrushOutline)` | a brush outline: its radius in image pixels and its hardness | painted over the image, the system cursor hidden |

The tool cursors are `Crosshair`, `Picker` (an eyedropper), `Hand`,
`Grab` (a closed hand), `ZoomIn`, `ZoomOut`, `Move`, `Rotate`, `Brush`,
`Eraser`, `MaskAdd`, `MaskSubtract` and `NotAllowed`. They are drawn
from vector strokes in the icon set's line style, ringed in the
outline colour so they read on any image (`cursor.fill`,
`cursor.outline`), at the exact number of physical pixels of the
cursor size, then kept.

### The tool decides, the situation changes it

The application gives the viewport its tool (`Viewport::tool`); the
viewport asks `cursor::default_cursor` each time the situation under
the pointer changes (a `CursorQuery`: the tool, the keys held, a drag,
the pan key, a handle under the pointer, whether it is over the image):

| Situation | Cursor |
|---|---|
| The pan key held, or a pan | an open hand, closed while dragging |
| Over a handle | move (rotate on a rotation handle), a closed hand while dragging it |
| `Tool::Zoom` | zoom in; with Alt, zoom out |
| `Tool::Pan` | an open hand, closed while dragging |
| `Tool::Picker`, `Crosshair`, `Move`, `Rotate` | their tool cursor |
| `Tool::Brush`, `Eraser`, `Mask` | the brush outline (an eraser's marked with a cross, a mask brush's with a plus, a minus with Alt) |
| `Tool::Disabled` | not allowed |
| `Tool::None`, `Tool::Named` | the system's arrow |

`Viewport::on_cursor` gets the query and that default and returns the
cursor to show; a change of answer is traced (`on_cursor`,
`redirect`). With `egui-pop-imaging`, `imaging::cursor` runs it as the
`cursor.resolve` step, where plugins give another cursor:

```rust
use egui_pop::egui;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::imaging::{self, STEP_CURSOR};
use egui_pop::model::ids::PluginId;
use egui_pop::viewport::cursor::{Brush, Tool};
use egui_pop::viewport::{View, Viewport, ViewportState};
use serde_json::json;

/// The brushes cannot paint outside the image.
struct InsideOnly;

impl Plugin for InsideOnly {
    fn id(&self) -> PluginId {
        PluginId::from_static("inside_only")
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![imaging::IMAGING_PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.replace(STEP_CURSOR, |_, args| {
            if args["query"]["tool"] == "brush" && args["query"]["over_image"] == false {
                Ok(json!({ "tool": "not-allowed" }))
            } else {
                Ok(args["default"].clone())
            }
        });
        Ok(())
    }
}

fn darkroom(ui: &mut egui::Ui, app: &AppHandle, state: &mut ViewportState, view: &mut View) {
    Viewport::new("main")
        .image_size([6000, 4000])
        .tool(Tool::Brush(Brush::new(40.0, 0.5)))
        .on_cursor(imaging::cursor(app, "main"))
        .show(ui, state, view);
}

let _ = (InsideOnly, darkroom);
```

The step's arguments are `viewport`, `query` (`{tool, modifiers,
dragging, panning, handle, over_image, brush}`) and `default`; a cursor
in JSON is `{"system": "Crosshair"}`, `{"tool": "zoom-in"}`,
`{"image": "lasso"}` or `{"brush": {"radius", "hardness", "mark"}}`.
The answer is kept until the situation changes;
`ViewportState::refresh_cursor` asks again.

### The brush outline

A brush outline is sized in image pixels: its radius on screen is the
radius times the zoom (physical pixels per image pixel), in points.
Below 4 points it gives way to a crosshair. Under 1 in hardness, an
inner, fainter ring shows where the full strength ends; a dot marks the
centre. Lines are whole physical pixels, a light line over a dark one
(or the reverse in a light theme).

### Settings and accessibility

- `cursor::CursorSettings` (`cursor::set_settings`) turns the painted
  cursors off: system cursors stand in for the tool cursors (zoom in
  and out, grab, move, not allowed, a crosshair for the rest), for the
  registered images (their fallback) and for the brush outline (a
  crosshair).
- The cursor size follows the system's: `XCURSOR_SIZE` on X11 and
  Wayland desktops, else 24 points; the settings override it. Bitmaps
  are drawn at that size times the window's scale.
- Reduced motion: an application's animated cursor (`CursorImage`
  frames) holds its first frame. egui-pop's own cursors never move.
- `ViewportState::cursor` and `cursor_shown` say what was resolved and
  shown; the debug bridge's `viewport` collector shows both.

### Where bitmaps reach the system

Bitmap cursors go through `egui::Context::set_cursor_image`, which
eframe's native integration (wgpu and glow) hands to winit as a custom
cursor. Where they cannot reach the system (the web, immediate
viewports), the tool cursor's system fallback shows instead (see
[Upstream workarounds](upstream.md)). A bitmap is kept by egui from
frame to frame: the viewport clears it when the pointer leaves, and
`cursor::apply` says whether one was set for a widget of the
application's own.

## The loupe

The loupe shows the pixels under the pointer, magnified, in a square
beside it (above and to the right, flipped at the viewport's edges):

```rust
use egui_pop::egui::Key;
use egui_pop::viewport::ViewportState;
use egui_pop::viewport::loupe::Values;

let mut state = ViewportState::new();
let loupe = state.loupe_mut();
loupe.key = Some(Key::L); // L turns it on and off over the viewport
loupe.magnification = 8; // 4 to 16 points per displayed pixel
loupe.values = Values::Display; // RGB and Lab as displayed (or Scene)
loupe.on = true; // a tool option: the picker's loupe
```

- **Displayed pixels**: the loupe draws the viewport's own tile
  textures again, after the display transform, through a magnifying
  mapping clipped to its square. On the GPU path a second paint
  callback samples the half-float tiles with the viewport's shader
  (its own uniforms), so nothing is read back to the CPU; on the CPU
  path the tiles' egui textures (nearest filtering) are drawn again.
- From 100 % up a displayed pixel is an image pixel; below, a screen
  pixel. Each becomes a block of whole physical pixels, the one under
  the pointer at the centre, with a grid between them and a marker
  around the centre.
- **Readout**: the pixel's coordinates, then its values as displayed
  (the source's `R`, `G` and `B` through the display stage, in 8 bits,
  and their CIE Lab, D65), or as the source gave them (`Values::Scene`).
- With the image on a Wayland subsurface of its own (`image_surface`),
  the loupe still draws on the window's surface, above the interface,
  with the display stage of the image's target: in a wide-gamut setup
  its colours are those of the image's encoding read as sRGB. Drawing
  it into the subsurface is not done yet.
- The key's toggles come out as `Signal::Loupe`; `loupe::Sampling`,
  `loupe::place` and `loupe::readout_lines` are the maths, and
  `ViewportState::loupe_frame` what was shown.

Tokens: `loupe.background`, `loupe.frame`, `loupe.grid`,
`loupe.marker`.

## Survey (2026-10-13)

What was checked before writing the cursors and the loupe, and the
decision for each.

| What | Licence | Finding | Decision |
|---|---|---|---|
| winit 0.30 `CustomCursor` | Apache-2.0 | `CustomCursor::from_rgba` (straight RGBA, a hot spot, up to 2048 px a side) made into a cursor by the event loop; on Wayland the buffer gets the window's integer scale, on X11 and Windows the bitmap is used as is | **adopt**, through egui |
| eframe and egui-winit 0.36.2 | MIT or Apache-2.0 | `Context::set_cursor_image(CustomCursorImage)` exists: egui-winit registers the bitmap with the event loop (cached by the `Arc`'s address) when it has one, which eframe's wgpu and glow integrations give for the root and deferred viewports; immediate viewports and the web drop it and show `cursor_icon` | **adopt**: the viewport hands bitmaps there, with a system cursor as the fallback |
| egui `CursorIcon` | MIT or Apache-2.0 | 35 CSS-like cursors: crosshair, grab and grabbing, zoom in and out, move, not allowed, none (hides the cursor); no eyedropper, brush or rotate | **adopt** as the fallbacks and for `Cursor::System` |
| GIMP | GPL-3.0 | tool cursors composed from a pointer, a tool image and a modifier badge (plus, minus, intersect, move...), with high-DPI variants; the brush outline drawn on the canvas; preferences for the outline, the pointer mode (tool icon, crosshair, both) and black-and-white or colour rendering | behaviour only: badges as the brush marks, the outline on the canvas, a setting for system cursors |
| Krita | GPL-3.0 | a cursor shape setting (tool icon, arrow, small circle, crosshair, a pixel) separate from the brush outline shape (none, circle, the brush's preview), the outline kept while painting, a small cursor when the outline is too small | behaviour only: the outline separate from the cursor, a crosshair under a minimum size |
| darktable | GPL-3.0 | system cursors by name for the views; drawn masks' brushes painted in the centre view with their border and their hardness as two circles, the system cursor hidden while placing a shape | behaviour only: the hardness ring, the hidden cursor |
| crates.io: "egui magnifier", "egui loupe", "egui zoom lens", "egui cursor" | | no loupe or tool cursor crate for egui; `egui-phosphor` (MIT) has the icon glyphs, but a glyph needs a font rasteriser to become a bitmap, which egui does not expose | **own**: the loupe redraws the viewport's tiles, the tool cursors are rasterised from vector strokes |

Photo editors with a loupe (Lightroom, Photoshop, a system colour
meter) magnify what is displayed and read out the values under it;
none of the GPL projects above has one in the pointer. egui-pop's loupe
magnifies the displayed tiles because the viewport already holds them
on the GPU, and the values come from the source's readout through the
same display stage the shader runs.
