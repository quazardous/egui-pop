//! Spike: several OS windows that behave almost as separate applications.
//!
//! What it checks, with the smallest code (plain eframe, no egui-pop):
//!
//! - a **hidden coordinator root** (`ViewportCommand::Visible(false)`) whose
//!   pass keeps running, so it can declare the other windows;
//! - two **deferred** windows (`show_viewport_deferred`), "A" animating and
//!   "B" idle, each repainting on its own;
//! - **own close**: closing one keeps the other and the application; closing
//!   the last quits;
//! - **geometry saved by the application** (eframe saves the root's only),
//!   restored at the next start;
//! - **focus and activation**: each window reports its focus; `F` in A asks
//!   the OS to raise B (`ViewportCommand::Focus`);
//! - the **`Rc` workaround**: the application state is `Rc<RefCell<…>>`,
//!   reached from the `Send + Sync` deferred callbacks through a
//!   thread-local registry keyed by viewport; each callback holds only its
//!   id and checks it runs on the main thread.
//!
//! It prints JSON lines on stdout for a driver (`tools/wbox/multi_window.py`):
//! `ready`, `counts` (frames per window, every 30 frames of A), `focus`,
//! `closed` (with the geometry saved), `opened` (with the geometry seen) and
//! `quit`. `SPIKE_STATE` names the file the geometry is saved to.
//! `SPIKE_AUTOPILOT=1` drives it without a window driver (a CI runner's
//! desktop): B is closed at A's 120th frame, A at its 300th, and the
//! process exits 0 by itself, 2 past a deadline.
//!
//! Variants, for the compositors where a window stops painting (headless
//! mutter): `SPIKE_PRESENT_MODE` for wgpu (`autovsync`, the default, `autonovsync`,
//! `fifo`, `mailbox`, `immediate`), and `SPIKE_A_SIZE` (`WxH`, A's size,
//! so that B opened on top of it does not cover it whole) and
//! `SPIKE_ROOT_SIZE` (`WxH`, the root's size, so that A and B do not cover
//! the root whole). The `ready`
//! line repeats them.
//!
//! Never run it on a desktop in use by someone else; the driver runs it in a
//! headless compositor.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::ThreadId;
use std::time::Instant;

use eframe::egui;
use egui::{ViewportBuilder, ViewportClass, ViewportCommand, ViewportId};

/// Callbacks that ran off the main thread (the shim's assumption broken).
static OFF_THREAD: AtomicUsize = AtomicUsize::new(0);

/// One window's state: deliberately not `Send`.
struct Win {
    name: &'static str,
    title: &'static str,
    animate: bool,
    frames: u64,
    focused: Option<bool>,
    inner_size: Option<egui::Vec2>,
    outer_pos: Option<egui::Pos2>,
    restored: Option<Geometry>,
    reported_open: bool,
    closed: bool,
}

#[derive(Clone, Copy, Debug)]
struct Geometry {
    size: Option<[f32; 2]>,
    pos: Option<[f32; 2]>,
}

/// The application's shared state, `Rc` as egui-pop's `AppHandle` is.
struct Shared {
    start: Instant,
    root_frames: u64,
    windows: BTreeMap<&'static str, Rc<RefCell<Win>>>,
    state_file: Option<std::path::PathBuf>,
}

/// A viewport's id to its window state and the application's, on the main thread.
type Registry = BTreeMap<ViewportId, (Rc<RefCell<Shared>>, Rc<RefCell<Win>>)>;

thread_local! {
    /// The main thread's id, set once at start.
    static MAIN: RefCell<Option<ThreadId>> = const { RefCell::new(None) };
    /// The registry: a viewport's id to its window state, on the main thread.
    static REGISTRY: RefCell<Registry> = const { RefCell::new(BTreeMap::new()) };
}

fn emit(value: serde_json::Value) {
    use std::io::Write as _;
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{value}");
    let _ = out.flush();
}

/// The process's CPU time so far, in milliseconds (Linux only).
fn cpu_ms() -> Option<u64> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    let rest = stat.rsplit_once(')')?.1;
    let fields: Vec<&str> = rest.split_whitespace().collect();
    // utime and stime are fields 14 and 15, the 12th and 13th after ')'
    let ticks: u64 = fields.get(11)?.parse::<u64>().ok()? + fields.get(12)?.parse::<u64>().ok()?;
    Some(ticks * 10) // USER_HZ is 100 on Linux
}

fn load_geometry(file: Option<&std::path::Path>) -> BTreeMap<String, Geometry> {
    let mut out = BTreeMap::new();
    let Some(text) = file.and_then(|f| std::fs::read_to_string(f).ok()) else {
        return out;
    };
    let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(&text)
    else {
        return out;
    };
    let pair = |v: &serde_json::Value| -> Option<[f32; 2]> {
        let a = v.as_array()?;
        Some([a.first()?.as_f64()? as f32, a.get(1)?.as_f64()? as f32])
    };
    for (name, g) in map {
        out.insert(
            name,
            Geometry {
                size: g.get("size").and_then(pair),
                pos: g.get("pos").and_then(pair),
            },
        );
    }
    out
}

fn save_geometry(shared: &Shared, win: &Win) {
    let Some(file) = &shared.state_file else {
        return;
    };
    let mut all: serde_json::Map<String, serde_json::Value> = std::fs::read_to_string(file)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    all.insert(
        win.name.to_owned(),
        serde_json::json!({
            "size": win.inner_size.map(|s| [s.x, s.y]),
            "pos": win.outer_pos.map(|p| [p.x, p.y]),
        }),
    );
    let _ = std::fs::write(file, serde_json::Value::Object(all).to_string());
}

fn viewport_id(name: &str) -> ViewportId {
    ViewportId::from_hash_of(("spike", name))
}

/// The shim: the `Send + Sync` callback holds only the id and finds the
/// window's `Rc` state in the main thread's registry.
fn window_ui(id: ViewportId, ui: &mut egui::Ui, class: ViewportClass) {
    let on_main = MAIN.with(|m| *m.borrow() == Some(std::thread::current().id()));
    if !on_main {
        OFF_THREAD.fetch_add(1, Ordering::Relaxed);
        emit(
            serde_json::json!({"event": "error", "error": "deferred callback off the main thread"}),
        );
        return;
    }
    let Some((shared, win)) = REGISTRY.with(|r| r.borrow().get(&id).cloned()) else {
        return;
    };
    let ctx = ui.ctx().clone();
    let (focused, inner, outer, close) = ui.input(|i| {
        let v = i.viewport();
        (
            v.focused,
            v.inner_rect.map(|r| r.size()),
            v.outer_rect.map(|r| r.min),
            v.close_requested(),
        )
    });
    let mut w = win.borrow_mut();
    w.frames += 1;
    if inner.is_some() {
        w.inner_size = inner;
    }
    if outer.is_some() {
        w.outer_pos = outer;
    }
    if focused != w.focused {
        w.focused = focused;
        emit(serde_json::json!({"event": "focus", "window": w.name, "focused": focused}));
    }
    if !w.reported_open && w.frames >= 3 {
        w.reported_open = true;
        emit(serde_json::json!({
            "event": "opened",
            "window": w.name,
            "embedded": matches!(class, ViewportClass::EmbeddedWindow),
            "inner_size": w.inner_size.map(|s| [s.x, s.y]),
            "outer_pos": w.outer_pos.map(|p| [p.x, p.y]),
            "restored": w.restored.map(|g| serde_json::json!({"size": g.size, "pos": g.pos})),
        }));
    }
    if close && !w.closed {
        // Our own close: forget this window; the root's next pass stops
        // declaring it, which closes it.
        w.closed = true;
        save_geometry(&shared.borrow(), &w);
        emit(serde_json::json!({
            "event": "closed",
            "window": w.name,
            "frames": w.frames,
            "saved": {"size": w.inner_size.map(|s| [s.x, s.y]), "pos": w.outer_pos.map(|p| [p.x, p.y])},
        }));
        ctx.request_repaint_of(ViewportId::ROOT);
        return;
    }
    if w.animate {
        // A keeps animating: it asks only for its own repaint.
        ctx.request_repaint();
        if w.frames.is_multiple_of(30) {
            let s = shared.borrow();
            let mut counts = serde_json::Map::new();
            for (name, other) in &s.windows {
                // This window's state is already borrowed, mutably, above.
                let frames = if *name == w.name {
                    w.frames
                } else {
                    other.borrow().frames
                };
                counts.insert((*name).to_owned(), frames.into());
            }
            emit(serde_json::json!({
                "event": "counts",
                "t": s.start.elapsed().as_secs_f64(),
                "root": s.root_frames,
                "windows": counts,
                "root_visible": ui.input(|i| i.raw.viewports.get(&ViewportId::ROOT).and_then(|v| v.visible())),
                "cpu_ms": cpu_ms(),
            }));
        }
        // Autopilot (CI runners without a window driver): the application
        // closes B, then A, as the OS would; a deadline ends a stuck run.
        if std::env::var_os("SPIKE_AUTOPILOT").is_some() {
            match w.frames {
                120 => ctx.send_viewport_cmd_to(viewport_id("b"), ViewportCommand::Close),
                300 => ctx.send_viewport_cmd(ViewportCommand::Close),
                3000.. => {
                    emit(serde_json::json!({"event": "error", "error": "autopilot deadline"}));
                    std::process::exit(2);
                }
                _ => {}
            }
        }
        if ui.input(|i| i.key_pressed(egui::Key::F)) {
            emit(serde_json::json!({"event": "raise", "window": "b"}));
            ctx.send_viewport_cmd_to(viewport_id("b"), ViewportCommand::Focus);
        }
    }
    let name = w.name;
    let frames = w.frames;
    drop(w);
    egui::CentralPanel::default().show(ui, |ui| {
        ui.heading(format!("Window {name}"));
        ui.label(format!("frames: {frames}"));
        if name == "a" {
            let t = ui.input(|i| i.time) as f32;
            let (rect, _) = ui.allocate_exact_size(egui::vec2(200.0, 20.0), egui::Sense::hover());
            let x = rect.left() + (t.sin() * 0.5 + 0.5) * rect.width();
            ui.painter()
                .circle_filled(egui::pos2(x, rect.center().y), 8.0, egui::Color32::GOLD);
        }
    });
}

struct Spike {
    shared: Rc<RefCell<Shared>>,
    hidden_sent: bool,
    quit_sent: bool,
}

impl Spike {
    fn new() -> Self {
        MAIN.with(|m| *m.borrow_mut() = Some(std::thread::current().id()));
        let state_file = std::env::var_os("SPIKE_STATE").map(std::path::PathBuf::from);
        let saved = load_geometry(state_file.as_deref());
        let shared = Rc::new(RefCell::new(Shared {
            start: Instant::now(),
            root_frames: 0,
            windows: BTreeMap::new(),
            state_file,
        }));
        for (name, title, animate) in [("a", "Spike A", true), ("b", "Spike B", false)] {
            let win = Rc::new(RefCell::new(Win {
                name,
                title,
                animate,
                frames: 0,
                focused: None,
                inner_size: None,
                outer_pos: None,
                restored: saved.get(name).copied(),
                reported_open: false,
                closed: false,
            }));
            shared.borrow_mut().windows.insert(name, Rc::clone(&win));
            REGISTRY.with(|r| {
                r.borrow_mut()
                    .insert(viewport_id(name), (Rc::clone(&shared), win))
            });
        }
        emit(serde_json::json!({
            "event": "ready",
            "pid": std::process::id(),
            "wayland": std::env::var_os("WAYLAND_DISPLAY").is_some(),
            "x11": std::env::var_os("DISPLAY").is_some(),
            "restored": saved.keys().collect::<Vec<_>>(),
            "present_mode": std::env::var("SPIKE_PRESENT_MODE").ok(),
            "a_size": std::env::var("SPIKE_A_SIZE").ok(),
            "root_size": std::env::var("SPIKE_ROOT_SIZE").ok(),
        }));
        Self {
            shared,
            hidden_sent: false,
            quit_sent: false,
        }
    }

    /// Windows still open.
    fn open(&self) -> Vec<Rc<RefCell<Win>>> {
        let s = self.shared.borrow();
        s.windows
            .values()
            .filter(|w| !w.borrow().closed)
            .cloned()
            .collect()
    }

    fn quit_if_empty(&mut self, ctx: &egui::Context) {
        if !self.quit_sent && self.open().is_empty() {
            self.quit_sent = true;
            emit(serde_json::json!({
                "event": "quit",
                "root_frames": self.shared.borrow().root_frames,
                "off_thread": OFF_THREAD.load(Ordering::Relaxed),
                "cpu_ms": cpu_ms(),
            }));
            ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close);
        }
    }
}

impl eframe::App for Spike {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Runs even while the root is hidden and gets no pass.
        self.quit_if_empty(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.shared.borrow_mut().root_frames += 1;
        if !self.hidden_sent {
            self.hidden_sent = true;
            ctx.send_viewport_cmd(ViewportCommand::Visible(false));
            let visible = ctx.input(|i| i.viewport().visible());
            emit(serde_json::json!({"event": "root_hide_sent", "visible_before": visible}));
        }
        ui.label("Spike coordinator: this window should be hidden.");
        for win in self.open() {
            let w = win.borrow();
            let id = viewport_id(w.name);
            let size = if w.name == "a" {
                env_size("SPIKE_A_SIZE").unwrap_or([360.0, 240.0])
            } else {
                [360.0, 240.0]
            };
            let mut builder = ViewportBuilder::default()
                .with_title(w.title)
                .with_app_id("egui-pop-spike")
                .with_inner_size(size);
            if let Some(g) = w.restored {
                if let Some(size) = g.size {
                    builder = builder.with_inner_size(size);
                }
                if let Some(pos) = g.pos {
                    builder = builder.with_position(pos);
                }
            }
            drop(w);
            ctx.show_viewport_deferred(id, builder, move |ui, class| window_ui(id, ui, class));
        }
        self.quit_if_empty(&ctx);
    }
}

/// A size given as `WxH` in the environment variable `name`.
fn env_size(name: &str) -> Option<[f32; 2]> {
    let text = std::env::var(name).ok()?;
    let (w, h) = text.split_once('x')?;
    Some([w.parse().ok()?, h.parse().ok()?])
}

fn main() -> eframe::Result {
    let mut options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_title("Spike root")
            .with_app_id("egui-pop-spike")
            .with_inner_size(env_size("SPIKE_ROOT_SIZE").unwrap_or([240.0, 120.0])),
        persist_window: false,
        ..Default::default()
    };
    let present_mode = match std::env::var("SPIKE_PRESENT_MODE").as_deref() {
        Ok("autonovsync") => Some(eframe::wgpu::PresentMode::AutoNoVsync),
        Ok("fifo") => Some(eframe::wgpu::PresentMode::Fifo),
        Ok("mailbox") => Some(eframe::wgpu::PresentMode::Mailbox),
        Ok("immediate") => Some(eframe::wgpu::PresentMode::Immediate),
        _ => None,
    };
    if let Some(mode) = present_mode {
        options.wgpu_options.surface.present_mode = mode;
    }
    eframe::run_native(
        "egui-pop multi-window spike",
        options,
        Box::new(|_cc| Ok(Box::new(Spike::new()))),
    )
}
