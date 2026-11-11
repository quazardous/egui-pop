//! Frame time and memory of the gallery's heavy pages, headless and
//! deterministic (docs/testing.md, "Performance").
//!
//! Each page runs on a fresh context for a fixed number of frames, with a
//! fixed clock (1/60 s a frame) and fixed input: the pointer sweeps the
//! window on a set path and the wheel scrolls down then up. Nothing is
//! paced: a frame starts when the last one ends. Per page it reports:
//!
//! - the frame's build time (`run_ui`: egui-pop's logic and drawing) and
//!   its tessellation, as percentiles;
//! - what the frame draws: shapes and vertices, the same on every run;
//! - memory: the process's resident memory before the page, its peak
//!   while the page runs and after its last frame (Linux; another system
//!   reports none).
//!
//! Rendering is not measured: there is no GPU here, only what the
//! interface thread does before egui-wgpu takes over.
//!
//! ```sh
//! cargo run --release -p egui-pop-gallery --example perf
//! cargo run --release -p egui-pop-gallery --example perf -- node-graph table
//! cargo run --release -p egui-pop-gallery --example perf -- --baseline last.jsonl
//! ```
//!
//! One JSON line per page on stdout (and in the file `PERF_OUT` names,
//! appended; `PERF_COMMIT` is copied into each line),
//! then a table. With `--baseline <file>` (the JSON lines of an earlier
//! run), every figure beyond its threshold is reported as a warning line
//! (`warning: ...`) and in a last JSON line (`"regressions"`); the exit
//! code stays 0: a regression warns, it does not fail.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::time::{Duration, Instant};

use egui::{Context, Event, MouseWheelUnit, Pos2, RawInput, Rect, vec2};
use egui_pop_gallery::{Gallery, Options};
use serde_json::{Value as Json, json};

// ---------------------------------------------------------------- memory

/// The process's resident memory and its peak, in kB (`VmRSS`, `VmHWM`
/// of `/proc/self/status`): Linux only, `None` elsewhere.
fn memory() -> Option<(u64, u64)> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let field = |name: &str| {
        status
            .lines()
            .find_map(|l| l.strip_prefix(name))?
            .split_whitespace()
            .next()?
            .parse::<u64>()
            .ok()
    };
    Some((field("VmRSS:")?, field("VmHWM:")?))
}

/// Resets the peak to the resident memory now (Linux: `5` to
/// `/proc/self/clear_refs`), so each page has its own peak.
fn reset_peak() {
    let _ = std::fs::write("/proc/self/clear_refs", "5");
}

// ----------------------------------------------------------------- pages

/// The pages measured by default: the heaviest to draw.
const PAGES: [&str; 6] = [
    "node-graph",
    "viewport",
    "file-browser",
    "table",
    "text-log",
    "sections",
];
/// Frames run before measuring: the first layout, fonts, caches.
const WARMUP: usize = 60;
/// Frames measured per page.
const FRAMES: usize = 300;
/// The window, in points.
const SCREEN: [f32; 2] = [1280.0, 800.0];
/// A frame's clock step: 60 Hz.
const DT: f64 = 1.0 / 60.0;

/// What a figure may grow by before a warning, in percent of the
/// baseline. Times vary from run to run; counts do not.
const THRESHOLDS: [(&str, f64); 6] = [
    ("build_p50_ms", 25.0),
    ("build_p95_ms", 40.0),
    ("tessellate_p50_ms", 25.0),
    ("vertices", 10.0),
    ("rss_peak_mb", 15.0),
    ("rss_growth_mb", 50.0),
];

/// The input of frame `n`: the pointer on a Lissajous path over the
/// window, the wheel down for a second then up, every four seconds.
fn input(n: usize) -> Vec<Event> {
    let t = n as f32 * DT as f32;
    let pos = Pos2::new(
        SCREEN[0] * (0.5 + 0.45 * (t * 0.9).sin()),
        SCREEN[1] * (0.5 + 0.45 * (t * 1.3).cos()),
    );
    let mut events = vec![Event::PointerMoved(pos)];
    match n % 240 {
        0..60 => events.push(wheel(-40.0)),
        120..180 => events.push(wheel(40.0)),
        _ => {}
    }
    events
}

fn wheel(dy: f32) -> Event {
    Event::MouseWheel {
        unit: MouseWheelUnit::Point,
        delta: vec2(0.0, dy),
        modifiers: egui::Modifiers::NONE,
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Wheel,
    }
}

// ------------------------------------------------------------------ runs

fn ms(d: Duration) -> f64 {
    (d.as_secs_f64() * 1e5).round() / 100.0
}

fn percentile(sorted: &[Duration], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    ms(sorted[((sorted.len() - 1) as f64 * p).round() as usize])
}

fn mb(kb: u64) -> f64 {
    (kb as f64 / 1e1).round() / 100.0
}

/// One page on a fresh context.
fn run(page: &str) -> Json {
    let ctx = Context::default();
    let mut gallery = Gallery::new(
        &ctx,
        Options {
            page: page.to_string(),
            no_persistence: true,
            reduced_motion: false,
            ..Options::default()
        },
    )
    .expect("the gallery builds");
    let screen = Rect::from_min_size(Pos2::ZERO, vec2(SCREEN[0], SCREEN[1]));
    let mut build = Vec::with_capacity(FRAMES);
    let mut tess = Vec::with_capacity(FRAMES);
    let (mut shapes, mut vertices) = (0usize, 0usize);
    let mut rss_start = 0;
    for n in 0..WARMUP + FRAMES {
        let measured = n >= WARMUP;
        if n == WARMUP {
            reset_peak();
            rss_start = memory().map_or(0, |m| m.0);
        }
        let raw = RawInput {
            screen_rect: Some(screen),
            time: Some(n as f64 * DT),
            predicted_dt: DT as f32,
            focused: true,
            events: input(n),
            ..RawInput::default()
        };
        let t0 = Instant::now();
        let out = ctx.run_ui(raw, |ui| {
            gallery.logic(ui.ctx());
            gallery.show(ui);
        });
        let t1 = Instant::now();
        let count = out.shapes.len();
        let primitives = ctx.tessellate(out.shapes, out.pixels_per_point);
        let t2 = Instant::now();
        if measured {
            build.push(t1 - t0);
            tess.push(t2 - t1);
            if n + 1 == WARMUP + FRAMES {
                shapes = count;
                vertices = primitives
                    .iter()
                    .map(|p| match &p.primitive {
                        egui::epaint::Primitive::Mesh(m) => m.vertices.len(),
                        egui::epaint::Primitive::Callback(_) => 0,
                    })
                    .sum();
            }
        }
    }
    let page_shown = gallery.page().to_string();
    let mem = memory();
    drop(gallery);
    build.sort();
    tess.sort();
    json!({
        "page": page,
        "shown": page_shown,
        "frames": FRAMES,
        "build_p50_ms": percentile(&build, 0.5),
        "build_p95_ms": percentile(&build, 0.95),
        "build_max_ms": percentile(&build, 1.0),
        "tessellate_p50_ms": percentile(&tess, 0.5),
        "tessellate_p95_ms": percentile(&tess, 0.95),
        "shapes": shapes,
        "vertices": vertices,
        "rss_start_mb": mem.map(|_| mb(rss_start)),
        "rss_peak_mb": mem.map(|m| mb(m.1)),
        "rss_end_mb": mem.map(|m| mb(m.0)),
        "rss_growth_mb": mem.map(|m| mb(m.0.saturating_sub(rss_start))),
    })
}

/// The pages of an earlier run's JSON lines, by name.
fn baseline(path: &str) -> BTreeMap<String, Json> {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("no baseline at {path}: {e}");
        String::new()
    });
    text.lines()
        .filter_map(|l| serde_json::from_str::<Json>(l).ok())
        .filter_map(|j| Some((j.get("page")?.as_str()?.to_string(), j)))
        .collect()
}

/// The figures of `now` beyond their threshold over `then`.
fn regressions(now: &Json, then: &Json) -> Vec<Json> {
    THRESHOLDS
        .iter()
        .filter_map(|&(key, pct)| {
            let a = then.get(key)?.as_f64()?;
            let b = now.get(key)?.as_f64()?;
            // Below a tenth of a millisecond or a megabyte, noise is all
            // there is.
            if a <= 0.0 || (key.ends_with("_ms") && b < 0.1) || (key.ends_with("_mb") && a < 1.0) {
                return None;
            }
            let change = (b - a) / a * 100.0;
            (change > pct).then(|| {
                json!({
                    "page": now["page"], "figure": key, "baseline": a, "now": b,
                    "change_pct": (change * 10.0).round() / 10.0, "threshold_pct": pct,
                })
            })
        })
        .collect()
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let base = args.iter().position(|a| a == "--baseline").map(|i| {
        args.remove(i);
        if i < args.len() {
            args.remove(i)
        } else {
            eprintln!("--baseline needs a file");
            std::process::exit(2);
        }
    });
    let pages: Vec<String> = if args.is_empty() {
        PAGES.iter().map(|p| p.to_string()).collect()
    } else {
        args
    };
    for p in &pages {
        if !egui_pop_gallery::PAGES.contains(&p.as_str()) {
            eprintln!("`{p}` is not a gallery page");
            std::process::exit(2);
        }
    }
    let mut out_file = std::env::var_os("PERF_OUT").map(|p| {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(p)
            .expect("PERF_OUT opens")
    });
    // Who this run measured, in every line: the commit (`PERF_COMMIT`)
    // and when, in seconds since the epoch.
    let commit = std::env::var("PERF_COMMIT").ok();
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let mut emit = |line: &Json| {
        println!("{line}");
        if let Some(f) = out_file.as_mut() {
            writeln!(f, "{line}").expect("PERF_OUT writes");
        }
    };
    let mut results = Vec::new();
    for page in &pages {
        let mut r = run(page);
        r["commit"] = json!(commit);
        r["at"] = json!(at);
        emit(&r);
        results.push(r);
    }
    println!();
    println!(
        "{:<14} {:>9} {:>9} {:>9} {:>9} {:>8} {:>9} {:>9}",
        "page", "build p50", "p95", "tess p50", "vertices", "shapes", "RSS peak", "growth"
    );
    for r in &results {
        println!(
            "{:<14} {:>9} {:>9} {:>9} {:>9} {:>8} {:>9} {:>9}",
            r["page"].as_str().unwrap_or(""),
            r["build_p50_ms"],
            r["build_p95_ms"],
            r["tessellate_p50_ms"],
            r["vertices"],
            r["shapes"],
            r["rss_peak_mb"],
            r["rss_growth_mb"],
        );
    }
    if let Some(path) = base {
        let then = baseline(&path);
        let found: Vec<Json> = results
            .iter()
            .filter_map(|r| Some(regressions(r, then.get(r["page"].as_str()?)?)))
            .flatten()
            .collect();
        println!();
        for f in &found {
            println!(
                "warning: {} {} {} -> {} (+{} %, threshold {} %)",
                f["page"].as_str().unwrap_or(""),
                f["figure"].as_str().unwrap_or(""),
                f["baseline"],
                f["now"],
                f["change_pct"],
                f["threshold_pct"]
            );
        }
        if found.is_empty() {
            println!("no regression beyond the thresholds against {path}");
        }
        let summary = json!({ "baseline": path, "regressions": found });
        println!("{summary}");
    }
}
