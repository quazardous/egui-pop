//! A procedural tile source, for galleries, examples and benchmarks: an
//! image of any size computed on worker threads, scene-referred (linear
//! floats), with a parameter that changes the whole image.
//!
//! It shows how a source behaves: requests return at once, workers skip
//! cancelled requests, tiles come back through the sink, and a change
//! is announced with [`TileSink::changed`]. [`Procedural::wait_idle`]
//! blocks until every request taken has been answered or dropped: tests
//! wait on it instead of sleeping.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;

use crate::source::{Pixels, Tile, TileRequest, TileSink, TileSource};

#[derive(Debug, Default)]
struct Queue {
    jobs: VecDeque<TileRequest>,
    busy: usize,
    shift: f32,
    served: u64,
    skipped: u64,
}

#[derive(Debug)]
struct Shared {
    queue: Mutex<Queue>,
    work: Condvar,
    idle: Condvar,
    stop: AtomicBool,
    sink: TileSink,
    size: [u32; 2],
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Queue> {
        self.queue.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The procedural source. Dropping it stops its workers.
#[derive(Debug)]
pub struct Procedural {
    shared: Arc<Shared>,
    workers: Vec<JoinHandle<()>>,
}

/// The image's colour at image point `(x, y)` (continuous) for a size
/// and a shift: smooth hues, rings, and a faint one-pixel checker so
/// 100 % and above show single pixels. Linear RGB.
pub fn sample(x: f64, y: f64, size: [u32; 2], shift: f32) -> [f32; 3] {
    let (w, h) = (f64::from(size[0]), f64::from(size[1]));
    let (u, v) = (x / w, y / h);
    let tau = std::f64::consts::TAU;
    let s = f64::from(shift);
    let mut rgb = [
        0.5 + 0.5 * (tau * (u + s)).cos(),
        0.5 + 0.5 * (tau * (v + s + 1.0 / 3.0)).cos(),
        0.5 + 0.5 * (tau * (u + v + s + 2.0 / 3.0)).cos(),
    ];
    let d = ((x - w / 2.0).powi(2) + (y - h / 2.0).powi(2)).sqrt();
    let ring = 0.75 + 0.25 * (d * 0.06).cos();
    let checker = if (x.floor() as i64 + y.floor() as i64) & 1 == 0 {
        1.0
    } else {
        0.92
    };
    for c in &mut rgb {
        *c = (*c * ring * checker).powi(2) * 0.9;
    }
    rgb.map(|c| c as f32)
}

fn render(r: &TileRequest, size: [u32; 2], shift: f32) -> Vec<f32> {
    let [tw, th] = r.size;
    let s = r.scale();
    // Below 100 %, average a few samples over each scaled pixel.
    let n = if s < 1.0 { 2 } else { 1 };
    let step = 1.0 / s / f64::from(n);
    let mut out = Vec::with_capacity(tw as usize * th as usize * 4);
    for j in 0..th {
        for i in 0..tw {
            let x0 = (r.origin[0] as f64 + f64::from(i)) / s;
            let y0 = (r.origin[1] as f64 + f64::from(j)) / s;
            let mut acc = [0.0f32; 3];
            for a in 0..n {
                for b in 0..n {
                    let c = sample(
                        x0 + (f64::from(a) + 0.5) * step,
                        y0 + (f64::from(b) + 0.5) * step,
                        size,
                        shift,
                    );
                    for k in 0..3 {
                        acc[k] += c[k];
                    }
                }
            }
            let m = (n * n) as f32;
            out.extend([acc[0] / m, acc[1] / m, acc[2] / m, 1.0]);
        }
    }
    out
}

fn work(shared: &Shared) {
    loop {
        let (job, shift) = {
            let mut q = shared.lock();
            loop {
                if shared.stop.load(Ordering::Relaxed) {
                    return;
                }
                if let Some(j) = q.jobs.pop_front() {
                    q.busy += 1;
                    break (j, q.shift);
                }
                q = shared.work.wait(q).unwrap_or_else(PoisonError::into_inner);
            }
        };
        let served = if job.is_cancelled() {
            false
        } else {
            let data = render(&job, shared.size, shift);
            if job.is_cancelled() {
                false
            } else {
                shared.sink.deliver(Tile::answer(&job, Pixels::Scene(data)));
                true
            }
        };
        let mut q = shared.lock();
        q.busy -= 1;
        if served {
            q.served += 1;
        } else {
            q.skipped += 1;
        }
        if q.jobs.is_empty() && q.busy == 0 {
            shared.idle.notify_all();
        }
    }
}

impl Procedural {
    /// An image of `size` pixels, computed by `workers` threads (none:
    /// computed when asked, on the caller's thread, for platforms
    /// without threads).
    pub fn new(sink: TileSink, size: [u32; 2], workers: usize) -> Procedural {
        let shared = Arc::new(Shared {
            queue: Mutex::default(),
            work: Condvar::new(),
            idle: Condvar::new(),
            stop: AtomicBool::new(false),
            sink,
            size,
        });
        let workers = (0..workers)
            .map(|i| {
                let s = shared.clone();
                std::thread::Builder::new()
                    .name(format!("egui-pop-viewport-demo-{i}"))
                    .spawn(move || work(&s))
                    .expect("a worker thread")
            })
            .collect();
        Procedural { shared, workers }
    }

    /// Moves the hues: the whole image changes, tiles on screen stay
    /// until their replacements arrive.
    pub fn set_shift(&self, shift: f32) {
        self.shared.lock().shift = shift;
        self.shared.sink.changed(None);
    }

    /// The current shift.
    pub fn shift(&self) -> f32 {
        self.shared.lock().shift
    }

    /// Blocks until every request taken was answered or skipped (never
    /// call it on the interface thread of a running application).
    pub fn wait_idle(&self) {
        let mut q = self.shared.lock();
        while !(q.jobs.is_empty() && q.busy == 0) {
            q = self
                .shared
                .idle
                .wait(q)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// Tiles answered and requests skipped because they were cancelled.
    pub fn counts(&self) -> (u64, u64) {
        let q = self.shared.lock();
        (q.served, q.skipped)
    }
}

impl Drop for Procedural {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        self.shared.work.notify_all();
        for w in self.workers.drain(..) {
            let _ = w.join();
        }
    }
}

impl TileSource for Procedural {
    fn size(&self) -> Option<[u32; 2]> {
        Some(self.shared.size)
    }

    fn request(&self, requests: Vec<TileRequest>) {
        if self.workers.is_empty() {
            // No threads (the web): computed here, a frame's work.
            let shift = self.shift();
            for r in requests {
                let data = render(&r, self.shared.size, shift);
                self.shared
                    .sink
                    .deliver(Tile::answer(&r, Pixels::Scene(data)));
                self.shared.lock().served += 1;
            }
            return;
        }
        self.shared.lock().jobs.extend(requests);
        self.shared.work.notify_all();
    }

    fn readout(&self, pixel: [i64; 2]) {
        let shift = self.shift();
        let c = sample(
            pixel[0] as f64 + 0.5,
            pixel[1] as f64 + 0.5,
            self.shared.size,
            shift,
        );
        self.shared.sink.readout(
            pixel,
            vec![("R".into(), c[0]), ("G".into(), c[1]), ("B".into(), c[2])],
        );
    }
}
