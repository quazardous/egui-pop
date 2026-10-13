# The image viewport

`egui-pop-viewport` (the facade's `viewport` feature, re-exported as
`egui_pop::viewport`) shows an image the application computes, in tiles:
pan and zoom in physical pixels (100 % is one image pixel per screen
pixel, in every window), a proxy first and exact tiles after, a display
transform where colour management plugs in, overlays with handles,
before and after, a pixel readout. The crate also has thumbnails in a
virtualised grid and a histogram.

```sh
cargo add egui-pop --features viewport
```

The application implements the tile source; the viewport asks, never
waits, and says what changed with signals.

```rust
use egui_pop::egui;
use egui_pop::viewport::source::{Pixels, Tile, TileRequest, TileSink, TileSource};
use egui_pop::viewport::{Signal, View, Viewport, ViewportState};

/// A flat mid-grey image, answered at once (a real source works on a
/// thread and delivers through the sink when done).
struct Grey {
    sink: TileSink,
}

impl TileSource for Grey {
    fn size(&self) -> Option<[u32; 2]> {
        Some([6000, 4000])
    }

    fn request(&self, requests: Vec<TileRequest>) {
        for r in requests {
            let n = r.size[0] as usize * r.size[1] as usize;
            // Scene-referred floats: the display transform runs on them.
            self.sink.deliver(Tile::answer(&r, Pixels::Scene(vec![0.18; n * 4])));
        }
    }
}

fn draw(ui: &mut egui::Ui, state: &mut ViewportState, view: &mut View, source: &Grey) {
    let out = Viewport::new("main").source(source).readout(true).show(ui, state, view);
    for signal in out.signals {
        if let Signal::View { view, .. } = signal {
            // Keep the view in the document, one undo step per gesture...
            let _ = view;
        }
    }
}

let state = ViewportState::new();
let source = Grey { sink: state.sink() };
let _ = (draw, source);
```

A source may also answer the pixel readout (`TileSource::readout`, then
`TileSink::readout`). Requests the viewport stops wanting get their
cancel token set.

The `viewer` example (`cargo run -p egui-pop-viewport --example viewer`,
a Mandelbrot set computed on a thread) and the gallery's `viewport` page
show it in an application:

![the viewport page: before and after, overlays](screenshots/viewport-pop-dark.png)

## Overlays

The application owns its overlays (`overlay::Overlay`: a shape in image
pixels, its handles, a label) and gives them to the viewport each frame
(`Viewport::overlays`). Dragging a handle is one gesture
(`Signal::Handle`, begin, update, end, or cancel on Escape), which the
application applies to its own data (`Overlay::apply` does it for the
standard shapes); a double click resets (`Signal::HandleReset`), the
wheel adjusts (`Signal::HandleWheel`). The `before_handle` hook may veto
or snap a move. Lines and handle outlines are drawn in whole physical
pixels at the window's scale (`scale::stroke`), so they stay sharp at
125 % and 150 %.

## Wide gamut and HDR

A window's surface has an encoding (`display::DisplayTarget`: primaries,
transfer, headroom, format); the viewport's display stage converts the
scene into it, on the GPU (its shader) and on the CPU
(`Stage::encode`): Display P3 and BT.2020 primaries (matrices computed
from the chromaticities), the sRGB curve, extended linear (scRGB) and
extended sRGB, SMPTE ST 2084 (PQ) and BT.2100 HLG.

An application says what it wants for a window; the viewport draws for
what the surface carries, and says why when that is less:

```rust
use egui_pop::egui;
use egui_pop::viewport::display::{self, ColorSpace, DisplayTarget, SurfaceFormat, TargetSource};

let ctx = egui::Context::default();
display::want(&ctx, egui::ViewportId::ROOT, DisplayTarget::display_p3());

// Under eframe the surface is 8-bit sRGB: sRGB is drawn, with the reason.
let r = display::resolve(&DisplayTarget::display_p3(), &DisplayTarget::srgb(), true);
assert!(!r.faithful());
assert_eq!(r.drawn.color_space(), Some(ColorSpace::Srgb));
println!("{}", r.reason().unwrap());

// A host that configured an extended sRGB surface on half floats says so,
// and P3 (or HDR) is drawn faithfully.
let surface = DisplayTarget::for_color_space(
    ColorSpace::ExtendedSrgb,
    SurfaceFormat::Rgba16Float,
    4.0,
    TargetSource::Tagged,
);
display::set_target(&ctx, egui::ViewportId::ROOT, surface);
assert!(display::resolve(&DisplayTarget::display_p3(), &surface, true).faithful());
```

What works where:

| Surface | Who configures it | Wide gamut / HDR |
|---|---|---|
| 8-bit sRGB | eframe 0.36, every platform | no: sRGB drawn, the reason given |
| extended sRGB, half floats | a host with its own surface (`gpu::choose_surface`), where wgpu offers it (Metal on macOS; Vulkan drivers exposing `EXTENDED_SRGB_NONLINEAR`) | yes, and egui's interface stays right |
| scRGB, HDR10 (PQ), HLG | a host with its own surface (`SurfacePolicy::Any`): DX12 and Vulkan on Windows, Metal | the image yes; egui's interface is written sRGB-encoded, so the host composes it itself |
| any, CPU path | (no GPU path installed) | no: tiles are 8-bit sRGB textures |

- `gpu::install` records what eframe configured (its format, wgpu's
  default colour space); `gpu::install_for_surface` is for a host that
  configured its surface with another colour space, and
  `gpu::choose_surface` picks the format and colour space to configure
  for a wanted target among the surface's capabilities.
- `ViewportState::display_resolution` gives what the last frame was
  drawn for; the debug bridge's `viewport` collector shows it, and the
  `before_display` hook may veto (draw sRGB) or redirect (a soft proof)
  it.
- Why eframe cannot do more today, and what is missing on Wayland, is
  in [Upstream workarounds](upstream.md).

The gallery's `display` page shows the resolution for each wish and
surface, and the transfers' curves.
