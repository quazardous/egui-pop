//! Frame times under background load, headless (docs/threading.md).
//!
//! Each scenario builds egui frames on this thread at 60 Hz, as eframe
//! would, while worker threads load the interface the way an application
//! does: a terminal flooded with output, a slow code assist provider, a
//! viewport's tile storm, a host publishing job events. It reports, per
//! scenario:
//!
//! - the frame's build time (`run_ui`: egui-pop's logic and drawing) and
//!   its tessellation, as percentiles, and the frames over budget;
//! - the textures uploaded per frame (count and bytes);
//! - for the viewport, the latency from a view change to its first
//!   full-resolution tile on screen, and to every wanted tile on screen;
//! - the wakeups egui would send to the event loop (its repaint
//!   callback) and the frames asking for another one at once;
//! - the time spent in egui-pop's profiling scopes, on this thread (`ui`)
//!   and on the others (`worker`): the `*.lock` scopes are waits for a
//!   lock, the others work (some of it under a lock).
//!
//! Rendering is not measured: there is no GPU here, only what the
//! interface thread does before egui-wgpu takes over.
//!
//! ```sh
//! cargo run --release -p egui-pop-gallery --features trace --example threading
//! cargo run --release -p egui-pop-gallery --features trace --example threading -- terminal-flood tiles
//! ```
//!
//! One JSON line per scenario on stdout, then a table; `THREADING_OUT`
//! names a file to write the JSON lines to as well.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::io::Write as _;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use egui::{Context, Event, Pos2, RawInput, Rect, Ui, ViewportId, vec2};
use egui_pop::api::host::{Dispatch, Host, Journal, Reply, Seq, Waker};
use egui_pop::api::{App, DefaultPlugins};
use egui_pop::core::binding::SharedStore;
use egui_pop::editors::assist::{AssistProvider, CompletionItem, Reply as AssistReply, Request};
use egui_pop::editors::terminal::Console;
use egui_pop::editors::{CodeEditor, EditorState, Terminal, TerminalState};
use egui_pop::viewport::View;
use egui_pop::viewport::tiles::UploadBudget;
use egui_pop_gallery::pages::viewport::{PageState, show as show_viewport};
use egui_pop_gallery::{Gallery, Options};
use serde_json::{Value as Json, json};

/// Frames per scenario: six seconds at 60 Hz.
const FRAMES: usize = 360;
/// The frame budget at 60 Hz.
const BUDGET: Duration = Duration::from_micros(16_667);
/// The window, in points.
const SCREEN: [f32; 2] = [1280.0, 800.0];

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

// ------------------------------------------------------------------ spans

/// Time spent in a scope: how often, in all, at most.
#[derive(Clone, Copy, Debug, Default)]
struct Stat {
    count: u64,
    total: Duration,
    max: Duration,
}

impl Stat {
    fn add(&mut self, d: Duration) {
        self.count += 1;
        self.total += d;
        self.max = self.max.max(d);
    }
}

type Stats = Arc<Mutex<HashMap<&'static str, Stat>>>;

/// Every thread's stats, with the thread's name.
static THREADS: Mutex<Vec<(String, Stats)>> = Mutex::new(Vec::new());

/// A thread's open spans and its stats (its own lock, never contended
/// but by the report).
struct Local {
    names: HashMap<u64, &'static str>,
    stack: Vec<(u64, Instant)>,
    stats: Stats,
}

thread_local! {
    static LOCAL: RefCell<Option<Local>> = const { RefCell::new(None) };
}

fn with_local(f: impl FnOnce(&mut Local)) {
    let _ = LOCAL.try_with(|l| {
        let Ok(mut l) = l.try_borrow_mut() else {
            return;
        };
        let l = l.get_or_insert_with(|| {
            let stats = Stats::default();
            let name = std::thread::current()
                .name()
                .unwrap_or("unnamed")
                .to_string();
            lock(&THREADS).push((name, stats.clone()));
            Local {
                names: HashMap::new(),
                stack: Vec::new(),
                stats,
            }
        });
        f(l);
    });
}

/// A minimal subscriber: the time between entering and leaving each
/// named scope (`profiling::scope!`), per thread. egui's and wgpu's
/// function scopes are left out.
struct Spans {
    next: AtomicU64,
}

fn wanted(m: &tracing::Metadata<'_>) -> bool {
    m.is_span() && m.name() != "function_scope"
}

impl tracing::Subscriber for Spans {
    fn register_callsite(
        &self,
        m: &'static tracing::Metadata<'static>,
    ) -> tracing::subscriber::Interest {
        if wanted(m) {
            tracing::subscriber::Interest::always()
        } else {
            tracing::subscriber::Interest::never()
        }
    }

    fn enabled(&self, m: &tracing::Metadata<'_>) -> bool {
        wanted(m)
    }

    fn new_span(&self, a: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let name = a.metadata().name();
        with_local(|l| {
            l.names.insert(id, name);
        });
        tracing::span::Id::from_u64(id)
    }

    fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}

    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}

    fn event(&self, _: &tracing::Event<'_>) {}

    fn enter(&self, id: &tracing::span::Id) {
        let now = Instant::now();
        with_local(|l| l.stack.push((id.into_u64(), now)));
    }

    fn exit(&self, id: &tracing::span::Id) {
        let now = Instant::now();
        let id = id.into_u64();
        with_local(|l| {
            if let Some(pos) = l.stack.iter().rposition(|(i, _)| *i == id) {
                let (_, at) = l.stack.remove(pos);
                if let Some(name) = l.names.get(&id) {
                    lock(&l.stats).entry(*name).or_default().add(now - at);
                }
            }
        });
    }

    fn try_close(&self, id: tracing::span::Id) -> bool {
        with_local(|l| {
            l.names.remove(&id.into_u64());
        });
        true
    }
}

fn reset_spans() {
    for (_, s) in lock(&THREADS).iter() {
        lock(s).clear();
    }
}

/// The scopes' times, `ui` (this thread) and `worker` (the others).
fn span_report() -> Json {
    let mut out: BTreeMap<String, Stat> = BTreeMap::new();
    for (thread, s) in lock(&THREADS).iter() {
        let side = if thread == "main" { "ui" } else { "worker" };
        for (name, st) in lock(s).iter() {
            let e = out.entry(format!("{side}:{name}")).or_default();
            e.count += st.count;
            e.total += st.total;
            e.max = e.max.max(st.max);
        }
    }
    Json::Object(
        out.into_iter()
            .map(|(k, s)| {
                (
                    k,
                    json!({
                        "count": s.count,
                        "total_ms": ms(s.total),
                        "max_ms": ms(s.max),
                    }),
                )
            })
            .collect(),
    )
}

fn ms(d: Duration) -> f64 {
    (d.as_secs_f64() * 1e6).round() / 1e3
}

// -------------------------------------------------------------- scenarios

/// A load and what the interface draws of it.
trait Scenario {
    /// The input of frame `n` (keys typed...).
    fn input(&mut self, _n: usize) -> Vec<Event> {
        Vec::new()
    }
    /// Draws frame `n`.
    fn frame(&mut self, ui: &mut Ui, n: usize);
    /// Stops the load (the drawing goes on).
    fn stop(&mut self) {}
    /// What the load produced (once stopped).
    fn finish(&mut self) -> Json;
}

/// The gallery's overview page, no load: the baseline.
struct Idle {
    gallery: Gallery,
}

impl Scenario for Idle {
    fn frame(&mut self, ui: &mut Ui, _n: usize) {
        self.gallery.show(ui);
    }
    fn finish(&mut self) -> Json {
        json!({})
    }
}

/// An egui-pop app with the default plugins, its logic run each frame;
/// none when another part of the scenario has one on this context.
fn app(ctx: &Context, own: bool) -> Option<App> {
    own.then(|| {
        App::builder()
            .plugins(DefaultPlugins)
            .build(ctx)
            .expect("the app builds")
    })
}

fn logic(app: &mut Option<App>, ui: &Ui) {
    if let Some(app) = app {
        let ctx = ui.ctx().clone();
        app.logic(&ctx);
    }
}

/// A thread writing to a console in 16 KiB chunks (a program's output as
/// a PTY reader gets it), unthrottled or at a rate.
struct Flood {
    app: Option<App>,
    state: TerminalState,
    stop: Arc<AtomicBool>,
    bytes: Arc<AtomicU64>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl Flood {
    fn new(ctx: &Context, rate: Option<u64>, own: bool) -> Flood {
        let app = app(ctx, own);
        let console = Console::new();
        let state = TerminalState::new("bench.flood", console.clone());
        state.spawn(None, "bench").expect("the console starts");
        let stop = Arc::new(AtomicBool::new(false));
        let bytes = Arc::new(AtomicU64::new(0));
        let worker = {
            let (stop, bytes) = (stop.clone(), bytes.clone());
            std::thread::Builder::new()
                .name("bench-flood".into())
                .spawn(move || {
                    let mut chunk = String::new();
                    let mut n = 0u64;
                    while chunk.len() < 16 * 1024 {
                        n += 1;
                        chunk.push_str(&format!(
                            "\x1b[1;32m   Compiling\x1b[0m crate-{n:05} v0.1.0 (/work/crates/crate-{n:05}) \x1b[2m[{n} of 9999]\x1b[0m\n"
                        ));
                    }
                    let started = Instant::now();
                    while !stop.load(Ordering::Relaxed) {
                        console.write(&chunk);
                        let total = bytes.fetch_add(chunk.len() as u64, Ordering::Relaxed)
                            + chunk.len() as u64;
                        if let Some(rate) = rate {
                            let due = Duration::from_secs_f64(total as f64 / rate as f64);
                            if let Some(wait) = due.checked_sub(started.elapsed()) {
                                std::thread::sleep(wait);
                            }
                        }
                    }
                })
                .expect("a thread")
        };
        Flood {
            app,
            state,
            stop,
            bytes,
            worker: Some(worker),
        }
    }
}

impl Scenario for Flood {
    fn frame(&mut self, ui: &mut Ui, _n: usize) {
        logic(&mut self.app, ui);
        let _ = Terminal::new()
            .height(SCREEN[1] - 40.0)
            .show(ui, &self.state);
    }
    fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
    fn finish(&mut self) -> Json {
        Scenario::stop(self);
        json!({
            "bytes": self.bytes.load(Ordering::Relaxed),
            "lines": self.state.line_count(),
        })
    }
}

/// A code assist provider taking `delay` to answer: on the interface
/// thread (a naive provider) or on its own thread (the contract).
struct Slow {
    inline: bool,
    delay: Duration,
    asked: Arc<AtomicU64>,
}

impl AssistProvider for Slow {
    fn name(&self) -> String {
        "bench.slow".into()
    }

    fn complete_while_typing(&self) -> bool {
        true
    }

    fn complete(&mut self, _request: &Request, reply: AssistReply<Vec<CompletionItem>>) {
        self.asked.fetch_add(1, Ordering::Relaxed);
        let delay = self.delay;
        if self.inline {
            std::thread::sleep(delay);
            reply.deliver(Vec::new());
        } else {
            let _ = std::thread::Builder::new()
                .name("bench-assist".into())
                .spawn(move || {
                    std::thread::sleep(delay);
                    reply.deliver(Vec::new());
                });
        }
    }
}

/// Typing in a code editor, a key every six frames, each asking a slow
/// provider for completions.
struct Assist {
    app: Option<App>,
    state: EditorState,
    asked: Arc<AtomicU64>,
}

impl Assist {
    fn new(ctx: &Context, inline: bool, delay: Duration) -> Assist {
        let asked = Arc::new(AtomicU64::new(0));
        let text = (0..400)
            .map(|i| format!("fn item_{i}(value: u32) -> u32 {{ value * {i} + 1 }}\n"))
            .collect::<String>();
        let state = EditorState::new("bench.assist", &text)
            .language("rs")
            .assist(Box::new(Slow {
                inline,
                delay,
                asked: asked.clone(),
            }));
        state.request_focus();
        Assist {
            app: app(ctx, true),
            state,
            asked,
        }
    }
}

impl Scenario for Assist {
    fn input(&mut self, n: usize) -> Vec<Event> {
        if n > 10 && n % 6 == 0 {
            vec![Event::Text("v".into())]
        } else {
            Vec::new()
        }
    }
    fn frame(&mut self, ui: &mut Ui, _n: usize) {
        logic(&mut self.app, ui);
        let _ = CodeEditor::new()
            .height(SCREEN[1] - 40.0)
            .show(ui, &self.state);
    }
    fn finish(&mut self) -> Json {
        json!({ "requests": self.asked.load(Ordering::Relaxed) })
    }
}

/// How the viewport takes tiles in: as by default (converted on the
/// source's workers, uploaded within `UploadBudget::DEFAULT`), as it did
/// before (converted and uploaded on the interface thread, all at once),
/// converted on the workers with no budget, on the interface thread
/// within a 1 MiB budget, or on the workers within 1 MiB.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Intake {
    Default,
    Interface,
    Worker,
    Budget,
    Delegated,
}

/// The upload budget of the budgeted scenarios: 1 MiB a frame, four
/// 256-pixel tiles.
const UPLOAD_BUDGET: usize = 1 << 20;

/// The viewport over the gallery's procedural source (two worker
/// threads), the view moving every `period` frames: a storm of tiles
/// (every 8 frames), or views held long enough to fill (every 30).
struct Tiles {
    app: Option<App>,
    page: PageState,
    moving: bool,
    period: usize,
    /// When the current view was set, and whether it got its first
    /// full-resolution tile and all its tiles yet.
    since: Option<(Instant, bool, bool)>,
    views: usize,
    first_exact: Vec<Duration>,
    complete: Vec<Duration>,
}

impl Tiles {
    fn new(ctx: &Context, own: bool, period: usize, intake: Intake) -> Tiles {
        let mut page = PageState::default();
        let (on_worker, budget) = match intake {
            Intake::Default => (true, UploadBudget::DEFAULT),
            Intake::Interface => (false, UploadBudget::UNLIMITED),
            Intake::Worker => (true, UploadBudget::UNLIMITED),
            Intake::Budget => (false, UploadBudget::bytes(UPLOAD_BUDGET)),
            Intake::Delegated => (true, UploadBudget::bytes(UPLOAD_BUDGET)),
        };
        page.state.set_prepare_on_worker(on_worker);
        page.state.set_upload_budget(budget);
        Tiles {
            app: app(ctx, own),
            page,
            moving: true,
            period,
            since: None,
            views: 0,
            first_exact: Vec::new(),
            complete: Vec::new(),
        }
    }
}

impl Scenario for Tiles {
    fn frame(&mut self, ui: &mut Ui, n: usize) {
        logic(&mut self.app, ui);
        if self.moving && n % self.period == 0 {
            const ZOOMS: [f64; 4] = [0.25, 0.5, 1.0, 2.0];
            let step = (n / self.period) as f64;
            let zoom = ZOOMS[(n / self.period) % ZOOMS.len()];
            let center = [
                2048.0 + 1500.0 * (step * 0.7).sin(),
                1536.0 + 1100.0 * (step * 0.45).cos(),
            ];
            self.page.view = View::at(zoom, center);
            self.since = Some((Instant::now(), false, false));
            self.views += 1;
        }
        show_viewport(ui, &mut self.page);
        if let Some((at, exact, complete)) = &mut self.since {
            if !*exact && self.page.state.upload_report().fresh_exact_drawn > 0 {
                *exact = true;
                self.first_exact.push(at.elapsed());
            }
            if !*complete && self.page.state.tiles_complete() {
                *complete = true;
                self.complete.push(at.elapsed());
            }
        }
    }
    fn stop(&mut self) {
        self.moving = false;
        self.since = None;
    }
    fn finish(&mut self) -> Json {
        let (served, skipped) = self.page.source.as_ref().map_or((0, 0), |s| s.counts());
        // Each latency in ms; `views_*`: how many views got there before
        // the next one.
        json!({
            "tiles_served": served,
            "requests_skipped": skipped,
            "latency": {
                "views": self.views,
                "views_with_first_exact": self.first_exact.len(),
                "views_complete": self.complete.len(),
                "first_exact": summary(std::mem::take(&mut self.first_exact)),
                "complete": summary(std::mem::take(&mut self.complete)),
            },
        })
    }
}

/// A host keeping its journal behind a lock, so its own threads publish:
/// what an application with a kernel does (ADR 0010).
struct LockedHost {
    journal: Arc<Mutex<Journal>>,
    store: SharedStore,
    waker: Arc<Mutex<Waker>>,
}

impl Host for LockedHost {
    fn dispatch(&mut self, d: &Dispatch<'_>) -> Reply {
        Reply::Done(
            d.run_local()
                .unwrap_or_else(|| Err(format!("no handler for `{}`", d.command))),
        )
    }

    fn publish(&mut self, source: &str, cause: Option<Seq>, name: &str, data: Json) -> Option<Seq> {
        Some(journal_lock(&self.journal).publish(source, cause, name, data))
    }

    fn since(&self, seq: Seq) -> Option<Vec<egui_pop::api::host::Event>> {
        Some(journal_lock(&self.journal).since(seq))
    }

    fn last_seq(&self) -> Option<Seq> {
        Some(journal_lock(&self.journal).last())
    }

    fn store(&self) -> SharedStore {
        self.store.clone()
    }

    fn set_waker(&mut self, waker: Waker) {
        *lock(&self.waker) = waker;
    }
}

/// The journal's lock, its wait measured.
fn journal_lock(j: &Mutex<Journal>) -> MutexGuard<'_, Journal> {
    let _span = tracing::info_span!("bench.journal.lock").entered();
    lock(j)
}

/// Four jobs on the host's threads, each publishing `job.progress` about
/// 1,250 times a second and waking the interface each time; the
/// interface folds the events each frame, as a jobs view does.
struct Jobs {
    app: App,
    mark: Seq,
    progress: BTreeMap<String, f64>,
    events: u64,
    stop: Arc<AtomicBool>,
    workers: Vec<std::thread::JoinHandle<()>>,
}

impl Jobs {
    fn new(ctx: &Context) -> Jobs {
        let journal = Arc::new(Mutex::new(Journal::default()));
        let waker = Arc::new(Mutex::new(Waker::default()));
        let app = App::builder()
            .host(LockedHost {
                journal: journal.clone(),
                store: Arc::new(Mutex::new(egui_pop::model::values::Store::new())),
                waker: waker.clone(),
            })
            .plugins(DefaultPlugins)
            .build(ctx)
            .expect("the app builds");
        let stop = Arc::new(AtomicBool::new(false));
        let workers = (0..4)
            .map(|j| {
                let (journal, waker, stop) = (journal.clone(), waker.clone(), stop.clone());
                std::thread::Builder::new()
                    .name(format!("bench-job-{j}"))
                    .spawn(move || {
                        let id = format!("job-{j}");
                        let mut done = 0u64;
                        while !stop.load(Ordering::Relaxed) {
                            done += 1;
                            journal_lock(&journal).publish(
                                "host",
                                None,
                                "job.progress",
                                json!({ "id": id, "done": done % 1000, "total": 1000, "item": format!("IMG_{done:05}.raw") }),
                            );
                            let w = lock(&waker).clone();
                            w.wake();
                            std::thread::sleep(Duration::from_micros(800));
                        }
                    })
                    .expect("a thread")
            })
            .collect();
        Jobs {
            app,
            mark: 0,
            progress: BTreeMap::new(),
            events: 0,
            stop,
            workers,
        }
    }
}

impl Scenario for Jobs {
    fn frame(&mut self, ui: &mut Ui, _n: usize) {
        let ctx = ui.ctx().clone();
        self.app.logic(&ctx);
        let events = self.app.handle().events_since(self.mark);
        if let Some(last) = events.last() {
            self.mark = last.seq;
        }
        for e in &events {
            if e.name == "job.progress" {
                self.events += 1;
                let id = e.data["id"].as_str().unwrap_or_default().to_string();
                let done = e.data["done"].as_f64().unwrap_or(0.0);
                let total = e.data["total"].as_f64().unwrap_or(1.0).max(1.0);
                self.progress.insert(id, done / total);
            }
        }
        for (id, f) in &self.progress {
            ui.label(id);
            ui.add(egui::ProgressBar::new(*f as f32).show_percentage());
        }
    }
    fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        for w in self.workers.drain(..) {
            let _ = w.join();
        }
    }
    fn finish(&mut self) -> Json {
        Scenario::stop(self);
        json!({ "events_folded": self.events })
    }
}

/// The terminal at a steady rate, the tile storm and the jobs at once,
/// side by side.
struct All {
    flood: Flood,
    tiles: Tiles,
    jobs: Jobs,
}

impl Scenario for All {
    fn frame(&mut self, ui: &mut Ui, n: usize) {
        ui.columns(3, |cols| {
            self.flood.frame(&mut cols[0], n);
            self.tiles.frame(&mut cols[1], n);
            self.jobs.frame(&mut cols[2], n);
        });
    }
    fn stop(&mut self) {
        self.flood.stop();
        self.tiles.stop();
        self.jobs.stop();
    }
    fn finish(&mut self) -> Json {
        json!({
            "terminal": self.flood.finish(),
            "tiles": self.tiles.finish(),
            "jobs": self.jobs.finish(),
        })
    }
}

const SCENARIOS: [&str; 16] = [
    "idle",
    "terminal-steady",
    "terminal-flood",
    "assist-thread",
    "assist-inline",
    "tiles",
    "tiles-ui",
    "tiles-worker",
    "tiles-budget",
    "tiles-delegated",
    "tiles-views",
    "tiles-views-ui",
    "jobs",
    "all",
    "all-ui",
    "idle-after",
];

/// The tile scenarios: the period between view changes and the intake.
fn tiles_of(name: &str) -> Option<(usize, Intake)> {
    Some(match name {
        "tiles" => (8, Intake::Default),
        "tiles-ui" => (8, Intake::Interface),
        "tiles-worker" => (8, Intake::Worker),
        "tiles-budget" => (8, Intake::Budget),
        "tiles-delegated" => (8, Intake::Delegated),
        "tiles-views" => (30, Intake::Default),
        "tiles-views-ui" => (30, Intake::Interface),
        _ => return None,
    })
}

fn make(name: &str, ctx: &Context) -> Box<dyn Scenario> {
    match name {
        "idle" => Box::new(Idle {
            gallery: Gallery::new(ctx, Options::default()).expect("the gallery builds"),
        }),
        // A build log: 256 KB/s.
        "terminal-steady" => Box::new(Flood::new(ctx, Some(256 * 1024), true)),
        "terminal-flood" => Box::new(Flood::new(ctx, None, true)),
        "assist-thread" => Box::new(Assist::new(ctx, false, Duration::from_millis(300))),
        "assist-inline" => Box::new(Assist::new(ctx, true, Duration::from_millis(30))),
        tiles if tiles_of(tiles).is_some() => {
            let (period, intake) = tiles_of(tiles).expect("matched");
            Box::new(Tiles::new(ctx, true, period, intake))
        }
        "jobs" => Box::new(Jobs::new(ctx)),
        // One app (the jobs' host's) for the three parts.
        "all" | "all-ui" | "idle-after" => Box::new(All {
            flood: Flood::new(ctx, Some(256 * 1024), false),
            tiles: Tiles::new(
                ctx,
                false,
                8,
                if name == "all-ui" {
                    Intake::Interface
                } else {
                    Intake::Default
                },
            ),
            jobs: Jobs::new(ctx),
        }),
        other => panic!("unknown scenario `{other}`; known: {SCENARIOS:?}"),
    }
}

// ------------------------------------------------------------------ runs

fn percentile(sorted: &[Duration], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let i = ((sorted.len() - 1) as f64 * p).round() as usize;
    ms(sorted[i])
}

fn summary(mut v: Vec<Duration>) -> Json {
    v.sort();
    json!({
        "p50_ms": percentile(&v, 0.5),
        "p95_ms": percentile(&v, 0.95),
        "p99_ms": percentile(&v, 0.99),
        "max_ms": percentile(&v, 1.0),
        "mean_ms": ms(v.iter().sum::<Duration>() / v.len().max(1) as u32),
    })
}

/// Runs a scenario on a fresh context: `FRAMES` frames paced at 60 Hz.
fn run(name: &str) -> Json {
    let ctx = Context::default();
    let wakeups = Arc::new(AtomicU64::new(0));
    let wakeups_off_ui = Arc::new(AtomicU64::new(0));
    {
        let (w, o) = (wakeups.clone(), wakeups_off_ui.clone());
        ctx.set_request_repaint_callback(move |_| {
            w.fetch_add(1, Ordering::Relaxed);
            if std::thread::current().name() != Some("main") {
                o.fetch_add(1, Ordering::Relaxed);
            }
        });
    }
    let mut scenario = make(name, &ctx);
    // `idle-after`: `all` once its load stopped (does everything
    // settle?). The others are measured after a short warm-up.
    let after = name == "idle-after";
    let (warmup, frames) = if after {
        (FRAMES, FRAMES)
    } else {
        (30, FRAMES)
    };
    let screen = Rect::from_min_size(Pos2::ZERO, vec2(SCREEN[0], SCREEN[1]));
    let start = Instant::now();
    let mut build = Vec::with_capacity(frames);
    let mut tess = Vec::with_capacity(frames);
    let (mut over, mut immediate) = (0usize, 0usize);
    let (mut textures, mut texture_bytes, mut max_textures, mut max_bytes) = (0, 0, 0, 0);
    for n in 0..warmup + frames {
        let measured = n >= warmup;
        if n == warmup {
            if after {
                scenario.stop();
            }
            reset_spans();
            wakeups.store(0, Ordering::Relaxed);
            wakeups_off_ui.store(0, Ordering::Relaxed);
        }
        let t0 = Instant::now();
        let input = RawInput {
            screen_rect: Some(screen),
            time: Some(start.elapsed().as_secs_f64()),
            predicted_dt: BUDGET.as_secs_f32(),
            focused: true,
            events: scenario.input(n),
            ..RawInput::default()
        };
        let out = ctx.run_ui(input, |ui| scenario.frame(ui, n));
        let t1 = Instant::now();
        let ppp = out.pixels_per_point;
        let shapes = out.shapes;
        let _primitives = ctx.tessellate(shapes, ppp);
        let t2 = Instant::now();
        if measured {
            build.push(t1 - t0);
            tess.push(t2 - t1);
            if t2 - t0 > BUDGET {
                over += 1;
            }
            let (count, bytes) =
                out.textures_delta
                    .set
                    .values()
                    .flatten()
                    .fold((0usize, 0usize), |(c, b), d| {
                        (
                            c + 1,
                            b + d.image.width() * d.image.height() * d.image.bytes_per_pixel(),
                        )
                    });
            textures += count;
            texture_bytes += bytes;
            max_textures = max_textures.max(count);
            max_bytes = max_bytes.max(bytes);
            if out
                .viewport_output
                .get(&ViewportId::ROOT)
                .is_some_and(|v| v.repaint_delay.is_zero())
            {
                immediate += 1;
            }
        }
        if let Some(wait) = BUDGET.checked_sub(t0.elapsed()) {
            std::thread::sleep(wait);
        }
    }
    let secs = frames as f64 / 60.0;
    let produced = scenario.finish();
    json!({
        "scenario": name,
        "frames": frames,
        "build": summary(build),
        "tessellate": summary(tess),
        "over_budget": over,
        "textures": {
            "uploads": textures,
            "mb": (texture_bytes as f64 / 1e5).round() / 10.0,
            "max_per_frame": max_textures,
            "max_mb_per_frame": (max_bytes as f64 / 1e5).round() / 10.0,
        },
        "repaint": {
            "wakeups_per_s": (wakeups.load(Ordering::Relaxed) as f64 / secs).round(),
            "wakeups_off_ui_per_s": (wakeups_off_ui.load(Ordering::Relaxed) as f64 / secs).round(),
            "immediate_frames": immediate,
        },
        "load": produced,
        "spans": span_report(),
    })
}

fn main() {
    // The interface thread is `main` in the reports.
    tracing::subscriber::set_global_default(Spans {
        next: AtomicU64::new(0),
    })
    .expect("no other subscriber");
    let asked: Vec<String> = std::env::args().skip(1).collect();
    let names: Vec<&str> = if asked.is_empty() {
        SCENARIOS.to_vec()
    } else {
        asked.iter().map(String::as_str).collect()
    };
    let mut file = std::env::var_os("THREADING_OUT")
        .map(|p| std::fs::File::create(p).expect("THREADING_OUT is writable"));
    let mut rows = Vec::new();
    for name in names {
        let r = run(name);
        println!("{r}");
        if let Some(f) = file.as_mut() {
            let _ = writeln!(f, "{r}");
        }
        rows.push(r);
    }
    println!();
    println!(
        "{:<16} {:>8} {:>8} {:>8} {:>8} {:>6} {:>8} {:>9} {:>9}",
        "scenario", "p50 ms", "p95 ms", "p99 ms", "max ms", "over", "tex/fr", "wake/s", "immediate"
    );
    for r in &rows {
        println!(
            "{:<16} {:>8} {:>8} {:>8} {:>8} {:>6} {:>8} {:>9} {:>9}",
            r["scenario"].as_str().unwrap_or(""),
            r["build"]["p50_ms"],
            r["build"]["p95_ms"],
            r["build"]["p99_ms"],
            r["build"]["max_ms"],
            r["over_budget"],
            r["textures"]["max_per_frame"],
            r["repaint"]["wakeups_per_s"],
            r["repaint"]["immediate_frames"],
        );
    }
}
