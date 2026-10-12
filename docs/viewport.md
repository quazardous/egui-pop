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
