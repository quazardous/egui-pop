//! The viewport's tile intake on a real GPU (docs/threading.md, section
//! 5.2): frame times, the time to the first full-resolution tile, and
//! memory, for a tile storm drawn and rendered with egui-wgpu offscreen.
//!
//! Each run builds frames on this thread at 60 Hz as eframe would, then
//! tessellates and renders them with egui-wgpu into an offscreen target,
//! submits, and waits for the GPU (as a present at most one frame ahead
//! does). The view jumps every `period` frames across zooms and places
//! of a 4096 × 3072 image whose tiles come from two worker threads.
//!
//! Intakes (the first argument, one per process so memory is its own):
//!
//! - `gpu-worker`: the GPU path, tiles converted to half floats on the
//!   source's workers, uploaded within the default budget (the default);
//! - `gpu-ui`: the GPU path, converted and uploaded on the interface
//!   thread, all at once (the old default);
//! - `cpu-worker`, `cpu-ui`: the same without the GPU path (the display
//!   transform on the CPU, egui textures);
//! - `gpu-render`: the workers render each tile on the GPU into a
//!   texture of their own and hand it over (`TileSink::deliver_device`):
//!   no pixel crosses the CPU.
//!
//! The second argument is the period (8: a storm; 30: views held).
//!
//! ```sh
//! cargo run --release -p egui-pop-viewport --example gpu_tiles -- gpu-worker 8
//! ```
//!
//! One JSON line on stdout (and appended to the file `GPU_TILES_OUT`).

use std::collections::VecDeque;
use std::io::Write as _;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use egui::{Context, Pos2, RawInput, Rect, vec2};
use egui_pop_viewport::demo::Procedural;
use egui_pop_viewport::device::DeviceTile;
use egui_pop_viewport::gpu::egui_wgpu::{self, wgpu};
use egui_pop_viewport::source::{TileRequest, TileSink, TileSource};
use egui_pop_viewport::tiles::UploadBudget;
use egui_pop_viewport::{View, Viewport, ViewportState};
use serde_json::{Value as Json, json};

/// Measured frames: six seconds at 60 Hz, after a warm-up.
const FRAMES: usize = 360;
const WARMUP: usize = 30;
/// The frame budget at 60 Hz.
const BUDGET: Duration = Duration::from_micros(16_667);
/// The window, in physical pixels (one point per pixel).
const SCREEN: [u32; 2] = [1280, 800];
/// The image, as the gallery's viewport page has it.
const IMAGE: [u32; 2] = [4096, 3072];
const WORKERS: usize = 2;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

// ------------------------------------------------------------ the device

/// The machine's GPU (a discrete one first), on Vulkan.
fn render_state() -> egui_wgpu::RenderState {
    let mut setup = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = wgpu::Backends::VULKAN;
    setup.native_adapter_selector = Some(Arc::new(|adapters, _surface| {
        let rank = |a: &wgpu::Adapter| match a.get_info().device_type {
            wgpu::DeviceType::DiscreteGpu => 0,
            wgpu::DeviceType::IntegratedGpu => 1,
            wgpu::DeviceType::VirtualGpu | wgpu::DeviceType::Other => 2,
            wgpu::DeviceType::Cpu => 3,
        };
        adapters
            .iter()
            .min_by_key(|a| rank(a))
            .cloned()
            .ok_or_else(|| "no Vulkan adapter".to_owned())
    }));
    egui_kittest::wgpu::create_render_state(
        egui_wgpu::WgpuSetup::CreateNew(setup),
        egui_wgpu::RendererOptions::default(),
    )
}

// ------------------------------------------- tiles rendered on the GPU

/// The demo source's image (`demo::sample`), in WGSL: each fragment is
/// a scaled pixel, averaged over 2 × 2 samples below 100 %.
const SHADER: &str = r#"
struct P {
    origin: vec2<f32>,
    size: vec2<f32>,
    scale: f32,
    shift: f32,
    n: f32,
    pad: f32,
};

@group(0) @binding(0) var<uniform> p: P;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    var corners = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    return vec4<f32>(corners[i], 0.0, 1.0);
}

fn sample(x: f32, y: f32) -> vec3<f32> {
    let w = p.size.x;
    let h = p.size.y;
    let u = x / w;
    let v = y / h;
    let tau = 6.283185307;
    let s = p.shift;
    var rgb = vec3<f32>(
        0.5 + 0.5 * cos(tau * (u + s)),
        0.5 + 0.5 * cos(tau * (v + s + 1.0 / 3.0)),
        0.5 + 0.5 * cos(tau * (u + v + s + 2.0 / 3.0)),
    );
    let d = sqrt((x - w / 2.0) * (x - w / 2.0) + (y - h / 2.0) * (y - h / 2.0));
    let ring = 0.75 + 0.25 * cos(d * 0.06);
    var checker = 0.92;
    if ((i32(floor(x)) + i32(floor(y))) & 1) == 0 {
        checker = 1.0;
    }
    let c = rgb * ring * checker;
    return c * c * 0.9;
}

@fragment
fn fs(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let px = floor(pos.xy);
    let step = 1.0 / p.scale / p.n;
    var acc = vec3<f32>(0.0);
    let n = i32(p.n);
    for (var a = 0; a < n; a++) {
        for (var b = 0; b < n; b++) {
            let x = (p.origin.x + px.x) / p.scale + (f32(a) + 0.5) * step;
            let y = (p.origin.y + px.y) / p.scale + (f32(b) + 0.5) * step;
            acc += sample(x, y);
        }
    }
    return vec4<f32>(acc / (p.n * p.n), 1.0);
}
"#;

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
}

#[derive(Default)]
struct Queue {
    jobs: VecDeque<TileRequest>,
    served: u64,
    skipped: u64,
}

struct Shared {
    queue: Mutex<Queue>,
    work: Condvar,
    stop: AtomicBool,
    sink: TileSink,
    gpu: Gpu,
}

/// A tile source whose workers render on the GPU path's device.
struct GpuSource {
    shared: Arc<Shared>,
    workers: Vec<JoinHandle<()>>,
}

impl GpuSource {
    fn new(sink: TileSink, rs: &egui_wgpu::RenderState, half: bool) -> GpuSource {
        let device = rs.device.clone();
        let format = if half {
            wgpu::TextureFormat::Rgba16Float
        } else {
            wgpu::TextureFormat::Rgba32Float
        };
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gpu-tiles"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gpu-tiles"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gpu-tiles"),
            bind_group_layouts: &[Some(&layout)],
            ..Default::default()
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("gpu-tiles"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let shared = Arc::new(Shared {
            queue: Mutex::default(),
            work: Condvar::new(),
            stop: AtomicBool::new(false),
            sink,
            gpu: Gpu {
                device,
                queue: rs.queue.clone(),
                pipeline,
                layout,
                format,
            },
        });
        let workers = (0..WORKERS)
            .map(|i| {
                let s = shared.clone();
                std::thread::Builder::new()
                    .name(format!("gpu-tiles-{i}"))
                    .spawn(move || gpu_work(&s))
                    .expect("a worker thread")
            })
            .collect();
        GpuSource { shared, workers }
    }

    fn counts(&self) -> (u64, u64) {
        let q = lock(&self.shared.queue);
        (q.served, q.skipped)
    }
}

/// Renders one tile into a texture of its own and submits it.
fn render_tile(g: &Gpu, r: &TileRequest) -> wgpu::Texture {
    let scale = r.scale();
    let values: [f32; 8] = [
        r.origin[0] as f32,
        r.origin[1] as f32,
        IMAGE[0] as f32,
        IMAGE[1] as f32,
        scale as f32,
        0.0,
        if scale < 1.0 { 2.0 } else { 1.0 },
        0.0,
    ];
    let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
    let buffer = g.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("gpu-tiles"),
        size: bytes.len() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    g.queue.write_buffer(&buffer, 0, &bytes);
    let group = g.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("gpu-tiles"),
        layout: &g.layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
    });
    let texture = g.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("gpu-tiles.tile"),
        size: wgpu::Extent3d {
            width: r.size[0],
            height: r.size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: g.format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = g
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("gpu-tiles"),
        });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("gpu-tiles"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&g.pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }
    g.queue.submit(std::iter::once(encoder.finish()));
    texture
}

fn gpu_work(s: &Shared) {
    loop {
        let job = {
            let mut q = lock(&s.queue);
            loop {
                if s.stop.load(Ordering::Relaxed) {
                    return;
                }
                if let Some(j) = q.jobs.pop_front() {
                    break j;
                }
                q = s.work.wait(q).unwrap_or_else(PoisonError::into_inner);
            }
        };
        if job.is_cancelled() {
            lock(&s.queue).skipped += 1;
            continue;
        }
        let texture = render_tile(&s.gpu, &job);
        // Submitted before the hand-over: the interface's frame, later
        // on the same queue, samples a finished texture.
        s.sink
            .deliver_device(&job, DeviceTile::texture(&s.gpu.device, texture));
        lock(&s.queue).served += 1;
    }
}

impl Drop for GpuSource {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        self.shared.work.notify_all();
        for w in self.workers.drain(..) {
            let _ = w.join();
        }
    }
}

impl TileSource for GpuSource {
    fn size(&self) -> Option<[u32; 2]> {
        Some(IMAGE)
    }

    fn request(&self, requests: Vec<TileRequest>) {
        lock(&self.shared.queue).jobs.extend(requests);
        self.shared.work.notify_all();
    }
}

// ------------------------------------------------------------ the runs

enum Source {
    Cpu(Procedural),
    Gpu(GpuSource),
}

impl Source {
    fn get(&self) -> &dyn TileSource {
        match self {
            Source::Cpu(s) => s,
            Source::Gpu(s) => s,
        }
    }

    fn counts(&self) -> (u64, u64) {
        match self {
            Source::Cpu(s) => s.counts(),
            Source::Gpu(s) => s.counts(),
        }
    }
}

fn ms(d: Duration) -> f64 {
    (d.as_secs_f64() * 1e6).round() / 1e3
}

fn summary(mut v: Vec<Duration>) -> Json {
    v.sort();
    let at = |p: f64| {
        if v.is_empty() {
            0.0
        } else {
            ms(v[((v.len() - 1) as f64 * p).round() as usize])
        }
    };
    json!({
        "n": v.len(),
        "p50_ms": at(0.5),
        "p95_ms": at(0.95),
        "p99_ms": at(0.99),
        "max_ms": at(1.0),
    })
}

/// A line of /proc/self/status, in MB.
fn status_mb(key: &str) -> Option<f64> {
    let s = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = s.lines().find(|l| l.starts_with(key))?;
    let kb: f64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some((kb / 1024.0 * 10.0).round() / 10.0)
}

fn mb(bytes: u64) -> f64 {
    (bytes as f64 / 1e5).round() / 10.0
}

fn run(intake: &str, period: usize) -> Json {
    let rs = render_state();
    let info = rs.adapter.get_info();
    let ctx = Context::default();
    let gpu_path = intake.starts_with("gpu");
    let installed = gpu_path.then(|| egui_pop_viewport::gpu::install(&ctx, &rs));
    let mut state = ViewportState::new();
    match intake {
        "gpu-worker" | "cpu-worker" | "gpu-render" => {}
        "gpu-ui" | "cpu-ui" => {
            state.set_prepare_on_worker(false);
            state.set_upload_budget(UploadBudget::UNLIMITED);
        }
        other => panic!("unknown intake `{other}`"),
    }
    let source = if intake == "gpu-render" {
        let half = installed.is_some_and(|i| i.half_float);
        Source::Gpu(GpuSource::new(state.sink(), &rs, half))
    } else {
        Source::Cpu(Procedural::new(state.sink(), IMAGE, WORKERS))
    };
    let mut view = View::fit();

    let target = rs.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("gpu-tiles.target"),
        size: wgpu::Extent3d {
            width: SCREEN[0],
            height: SCREEN[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: rs.target_format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let screen = egui_wgpu::ScreenDescriptor {
        size_in_pixels: SCREEN,
        pixels_per_point: 1.0,
    };
    let screen_rect = Rect::from_min_size(Pos2::ZERO, vec2(SCREEN[0] as f32, SCREEN[1] as f32));

    let start = Instant::now();
    let (mut build, mut render, mut ui_total, mut gpu_wait) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let (mut over, mut max_tiles, mut max_bytes, mut max_held) = (0usize, 0usize, 0usize, 0usize);
    let mut max_gpu_alloc: Option<u64> = None;
    let (mut first_exact, mut complete) = (Vec::new(), Vec::new());
    let mut since: Option<(Instant, bool, bool)> = None;
    let mut views = 0usize;
    for n in 0..WARMUP + FRAMES {
        let measured = n >= WARMUP;
        if n % period == 0 {
            const ZOOMS: [f64; 4] = [0.25, 0.5, 1.0, 2.0];
            let step = (n / period) as f64;
            let zoom = ZOOMS[(n / period) % ZOOMS.len()];
            let center = [
                2048.0 + 1500.0 * (step * 0.7).sin(),
                1536.0 + 1100.0 * (step * 0.45).cos(),
            ];
            view = View::at(zoom, center);
            if measured {
                since = Some((Instant::now(), false, false));
                views += 1;
            }
        }
        let t0 = Instant::now();
        let input = RawInput {
            screen_rect: Some(screen_rect),
            time: Some(start.elapsed().as_secs_f64()),
            predicted_dt: BUDGET.as_secs_f32(),
            focused: true,
            ..RawInput::default()
        };
        let out = ctx.run_ui(input, |ui| {
            let _ = Viewport::new("bench")
                .source(source.get())
                .show(ui, &mut state, &mut view);
        });
        let t1 = Instant::now();
        let primitives = ctx.tessellate(out.shapes, out.pixels_per_point);
        {
            let mut renderer = rs.renderer.write();
            for (id, deltas) in &out.textures_delta.set {
                for d in deltas {
                    renderer.update_texture(&rs.device, &rs.queue, *id, d);
                }
            }
            let mut encoder = rs
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("gpu-tiles.frame"),
                });
            let user =
                renderer.update_buffers(&rs.device, &rs.queue, &mut encoder, &primitives, &screen);
            {
                let mut pass = encoder
                    .begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("gpu-tiles.frame"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &target_view,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        ..Default::default()
                    })
                    .forget_lifetime();
                renderer.render(&mut pass, &primitives, &screen);
            }
            rs.queue
                .submit(user.into_iter().chain(std::iter::once(encoder.finish())));
            for id in &out.textures_delta.free {
                renderer.free_texture(id);
            }
        }
        let t2 = Instant::now();
        let _ = rs.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(10)),
        });
        let t3 = Instant::now();
        let report = state.upload_report();
        if let Some((at, exact, done)) = &mut since {
            if !*exact && report.fresh_exact_drawn > 0 {
                *exact = true;
                first_exact.push(at.elapsed());
            }
            if !*done && state.tiles_complete() {
                *done = true;
                complete.push(at.elapsed());
            }
        }
        if measured {
            build.push(t1 - t0);
            render.push(t2 - t1);
            ui_total.push(t2 - t0);
            gpu_wait.push(t3 - t2);
            if t3 - t0 > BUDGET {
                over += 1;
            }
            max_tiles = max_tiles.max(report.tiles);
            max_bytes = max_bytes.max(report.bytes);
            max_held = max_held.max(state.texture_bytes());
            if n % 30 == 0
                && let Some(r) = rs.device.generate_allocator_report()
            {
                max_gpu_alloc = Some(max_gpu_alloc.unwrap_or(0).max(r.total_reserved_bytes));
            }
        }
        if let Some(wait) = BUDGET.checked_sub(t0.elapsed()) {
            std::thread::sleep(wait);
        }
    }
    let (served, skipped) = source.counts();
    json!({
        "intake": intake,
        "period": period,
        "adapter": format!("{} ({:?}, {:?})", info.name, info.device_type, info.backend),
        "gpu_path": installed.is_some(),
        "frames": FRAMES,
        "ui_thread": summary(ui_total),
        "build": summary(build),
        "render": summary(render),
        "gpu_wait": summary(gpu_wait),
        "over_budget": over,
        "uploads": {
            "max_tiles_per_frame": max_tiles,
            "max_mb_per_frame": mb(max_bytes as u64),
        },
        "latency": {
            "views": views,
            "first_exact": summary(first_exact),
            "complete": summary(complete),
        },
        "memory": {
            "tile_textures_max_mb": mb(max_held as u64),
            "gpu_reserved_max_mb": max_gpu_alloc.map(mb),
            "rss_mb": status_mb("VmRSS:"),
            "rss_peak_mb": status_mb("VmHWM:"),
        },
        "tiles_served": served,
        "requests_skipped": skipped,
    })
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let intake = args.first().map_or("gpu-worker", String::as_str);
    let period = args.get(1).and_then(|p| p.parse().ok()).unwrap_or(8);
    let r = run(intake, period);
    println!("{r}");
    if let Some(path) = std::env::var_os("GPU_TILES_OUT") {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .expect("GPU_TILES_OUT is writable");
        let _ = writeln!(f, "{r}");
    }
}
