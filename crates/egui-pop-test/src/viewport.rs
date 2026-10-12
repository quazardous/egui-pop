//! A tile source for tests (feature `viewport`): every image pixel has
//! its own colour, tiles are served at once or when the test says, and
//! everything the viewport asked for and cancelled is counted. No thread
//! and no sleep: a test steps the harness and calls
//! [`TestSource::deliver_all`] when it wants the tiles to arrive.
//!
//! ```
//! use egui_pop_test::viewport::TestSource;
//! use egui_pop_viewport::ViewportState;
//!
//! let state = ViewportState::new();
//! let source = TestSource::manual(state.sink(), [1000, 800]);
//! // ... draw the viewport with `&source`, step, then:
//! assert_eq!(source.deliver_all(), 0);
//! ```

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use egui_pop_viewport::source::{Pixels, Tile, TileRequest, TileSink, TileSource};

/// When requests are served.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delivery {
    /// As soon as asked (the tiles show on the next frame).
    Immediate,
    /// When the test calls [`TestSource::deliver_all`].
    Manual,
}

#[derive(Debug, Default)]
struct Inner {
    pending: Vec<TileRequest>,
    requested: u64,
    served: u64,
    cancelled: u64,
    revision: u8,
    readouts: Vec<[i64; 2]>,
}

/// The test source. Clone it to keep a handle while the viewport uses
/// another.
#[derive(Clone, Debug)]
pub struct TestSource {
    sink: TileSink,
    size: Arc<Mutex<Option<[u32; 2]>>>,
    delivery: Delivery,
    scene: bool,
    inner: Arc<Mutex<Inner>>,
}

impl TestSource {
    /// Serves tiles as soon as they are asked for.
    pub fn immediate(sink: TileSink, size: [u32; 2]) -> TestSource {
        TestSource::new(sink, size, Delivery::Immediate)
    }

    /// Serves tiles when the test calls [`TestSource::deliver_all`].
    pub fn manual(sink: TileSink, size: [u32; 2]) -> TestSource {
        TestSource::new(sink, size, Delivery::Manual)
    }

    fn new(sink: TileSink, size: [u32; 2], delivery: Delivery) -> TestSource {
        TestSource {
            sink,
            size: Arc::new(Mutex::new(Some(size))),
            delivery,
            scene: false,
            inner: Arc::default(),
        }
    }

    /// Delivers scene pixels (linear floats, through the viewport's
    /// display transform) instead of display pixels.
    pub fn scene(mut self) -> TestSource {
        self.scene = true;
        self
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The display colour of image pixel `(x, y)` at a revision: red is
    /// `x % 256`, green `y % 256`, blue the 256-pixel block and the
    /// revision.
    pub fn pixel(x: i64, y: i64, revision: u8) -> [u8; 4] {
        let block = ((x / 256) + 4 * (y / 256)) as u8;
        [
            (x % 256) as u8,
            (y % 256) as u8,
            block.wrapping_mul(16).wrapping_add(revision),
            255,
        ]
    }

    /// The current revision (bumped by [`TestSource::change`]).
    pub fn revision(&self) -> u8 {
        self.lock().revision
    }

    fn serve(&self, r: &TileRequest, revision: u8) {
        let [w, h] = r.size;
        let mut bytes = Vec::with_capacity(w as usize * h as usize * 4);
        for j in 0..h {
            for i in 0..w {
                let p = r.image_point(i, j);
                bytes.extend(TestSource::pixel(
                    p[0].floor() as i64,
                    p[1].floor() as i64,
                    revision,
                ));
            }
        }
        let pixels = if self.scene {
            Pixels::Scene(
                bytes
                    .chunks_exact(4)
                    .flat_map(|c| {
                        let lin = |v: u8| egui::ecolor::linear_f32_from_gamma_u8(v);
                        [lin(c[0]), lin(c[1]), lin(c[2]), f32::from(c[3]) / 255.0]
                    })
                    .collect(),
            )
        } else {
            Pixels::Display(bytes)
        };
        self.sink.deliver(Tile::answer(r, pixels));
    }

    /// Serves every pending request not cancelled; returns how many.
    pub fn deliver_all(&self) -> usize {
        let (pending, revision) = {
            let mut i = self.lock();
            (std::mem::take(&mut i.pending), i.revision)
        };
        let mut n = 0;
        for r in &pending {
            if r.is_cancelled() {
                self.lock().cancelled += 1;
                continue;
            }
            self.serve(r, revision);
            n += 1;
        }
        self.lock().served += n as u64;
        n
    }

    /// The requests waiting (manual delivery).
    pub fn pending(&self) -> Vec<TileRequest> {
        self.lock().pending.clone()
    }

    /// Requests received so far.
    pub fn requested(&self) -> u64 {
        self.lock().requested
    }

    /// Tiles served so far.
    pub fn served(&self) -> u64 {
        self.lock().served
    }

    /// Requests found cancelled when serving.
    pub fn cancelled(&self) -> u64 {
        self.lock().cancelled
    }

    /// The pixels the viewport asked a readout for.
    pub fn readouts(&self) -> Vec<[i64; 2]> {
        self.lock().readouts.clone()
    }

    /// The image changes in `region` (or everywhere): bumps the revision
    /// and tells the viewport.
    pub fn change(&self, region: Option<[f64; 4]>) {
        let mut i = self.lock();
        i.revision = i.revision.wrapping_add(1);
        drop(i);
        self.sink.changed(region);
    }

    /// A new image size (or none).
    pub fn resize(&self, size: Option<[u32; 2]>) {
        *self.size.lock().unwrap_or_else(PoisonError::into_inner) = size;
        self.sink.changed(None);
    }
}

impl TileSource for TestSource {
    fn size(&self) -> Option<[u32; 2]> {
        *self.size.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn request(&self, requests: Vec<TileRequest>) {
        {
            let mut i = self.lock();
            i.requested += requests.len() as u64;
            i.pending.extend(requests);
        }
        if self.delivery == Delivery::Immediate {
            self.deliver_all();
        }
    }

    fn readout(&self, pixel: [i64; 2]) {
        let revision = {
            let mut i = self.lock();
            i.readouts.push(pixel);
            i.revision
        };
        let [r, g, b, _] = TestSource::pixel(pixel[0], pixel[1], revision);
        self.sink.readout(
            pixel,
            vec![
                ("R".into(), f32::from(r)),
                ("G".into(), f32::from(g)),
                ("B".into(), f32::from(b)),
            ],
        );
    }
}
