//! An application implementing the viewport's `TileSource`: a Mandelbrot
//! set computed on a thread, delivered as scene pixels, shown with the
//! GPU path when eframe runs on wgpu.
//!
//! ```sh
//! cargo run -p egui-pop-viewport --example viewer
//! ```
//!
//! The source answers at once (`request` only queues), its thread skips
//! what the viewport cancelled, and a change of the iteration count is
//! announced with `TileSink::changed`.

use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use egui_pop_viewport::source::{Pixels, Tile, TileRequest, TileSink, TileSource};
use egui_pop_viewport::{View, Viewport, ViewportState};

/// The image's size, in pixels.
const SIZE: [u32; 2] = [8192, 6144];

/// The application's source: a queue to a worker thread.
struct Mandelbrot {
    jobs: Sender<TileRequest>,
    iterations: Arc<Mutex<u32>>,
    sink: TileSink,
}

fn colour(x: f64, y: f64, iterations: u32) -> [f32; 3] {
    // The image spans [-2.5, 1] × [-1.3125, 1.3125].
    let cx = -2.5 + x / f64::from(SIZE[0]) * 3.5;
    let cy = -1.3125 + y / f64::from(SIZE[1]) * 2.625;
    let (mut zx, mut zy, mut n) = (0.0f64, 0.0f64, 0);
    while zx * zx + zy * zy < 4.0 && n < iterations {
        (zx, zy) = (zx * zx - zy * zy + cx, 2.0 * zx * zy + cy);
        n += 1;
    }
    if n == iterations {
        return [0.0, 0.0, 0.0];
    }
    let t = f64::from(n) / f64::from(iterations);
    [
        (9.0 * (1.0 - t) * t * t * t) as f32,
        (15.0 * (1.0 - t).powi(2) * t * t) as f32,
        (8.5 * (1.0 - t).powi(3) * t) as f32,
    ]
}

fn worker(jobs: Receiver<TileRequest>, sink: TileSink, iterations: Arc<Mutex<u32>>) {
    while let Ok(r) = jobs.recv() {
        if r.is_cancelled() {
            continue;
        }
        let its = *iterations.lock().unwrap();
        let [w, h] = r.size;
        let mut data = Vec::with_capacity(w as usize * h as usize * 4);
        for j in 0..h {
            for i in 0..w {
                let p = r.image_point(i, j);
                let [cr, cg, cb] = colour(p[0], p[1], its);
                data.extend([cr, cg, cb, 1.0]);
            }
        }
        if !r.is_cancelled() {
            sink.deliver(Tile::answer(&r, Pixels::Scene(data)));
        }
    }
}

impl Mandelbrot {
    fn new(sink: TileSink) -> Mandelbrot {
        let (jobs, rx) = channel();
        let iterations = Arc::new(Mutex::new(200));
        let (s, its) = (sink.clone(), iterations.clone());
        std::thread::spawn(move || worker(rx, s, its));
        Mandelbrot {
            jobs,
            iterations,
            sink,
        }
    }

    fn set_iterations(&self, n: u32) {
        *self.iterations.lock().unwrap() = n;
        self.sink.changed(None);
    }
}

impl TileSource for Mandelbrot {
    fn size(&self) -> Option<[u32; 2]> {
        Some(SIZE)
    }

    fn request(&self, requests: Vec<TileRequest>) {
        for r in requests {
            let _ = self.jobs.send(r);
        }
    }
}

struct Viewer {
    view: View,
    state: ViewportState,
    source: Mandelbrot,
    iterations: u32,
}

impl eframe::App for Viewer {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.horizontal(|ui| {
            if ui
                .add(egui::Slider::new(&mut self.iterations, 20..=2000).text("Iterations"))
                .changed()
            {
                self.source.set_iterations(self.iterations);
            }
            ui.label(format!(
                "{} tiles, {} waiting",
                self.state.tile_count(),
                self.state.outstanding()
            ));
        });
        Viewport::new("mandelbrot")
            .source(&self.source)
            .show(ui, &mut self.state, &mut self.view);
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Viewer")
            .with_inner_size([1000.0, 720.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Viewer",
        options,
        Box::new(|cc| {
            if let Some(rs) = cc.wgpu_render_state.as_ref() {
                egui_pop_viewport::gpu::install(&cc.egui_ctx, rs);
            }
            let state = ViewportState::new();
            let source = Mandelbrot::new(state.sink());
            Ok(Box::new(Viewer {
                view: View::fit(),
                state,
                source,
                iterations: 200,
            }))
        }),
    )
}
