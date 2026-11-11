//! Several OS windows through egui-pop's API (`AppHandle::open_window`,
//! ADR 0025): the main window and two application windows, "A" animating
//! and "B" idle, each drawn by its own pass, their state `Rc`.
//!
//! - Titles come from each window's content: B counts the keys typed in
//!   it and names itself after the count.
//! - Closing B keeps A and the main window; closing the main window while
//!   A is open moves A into it; closing the last window quits.
//! - `F` in A raises B; `W` in any window lists the windows (a `windows`
//!   line).
//! - Each window's geometry is kept in `windows.json` under the directory
//!   `MW_STATE` names, and given back at the next start.
//!
//! It prints JSON lines on stdout for a driver (`tools/wbox/multi_window.py`):
//! `ready`, `counts` (frames per window, every 30 frames of A), `focus`,
//! `opened`, `closed`, `moved`, `raise`, `windows` and `quit`.
//! `MW_AUTOPILOT=1` drives it without a window driver: B is closed at A's
//! 120th frame, the main window at its 200th (A moves into it), and A at
//! its 300th, which quits; past a deadline the process exits 2.
//! `MW_PRESENT_MODE` (`autovsync`, `autonovsync`, `fifo`, `mailbox`)
//! overrides the present mode `AppBuilder::multi_window` picks.
//!
//! Never run it on a desktop in use by someone else; the driver runs it in
//! a headless compositor.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

use egui_pop::api::eframe;
use egui_pop::api::windows::{WindowId, WindowSpec, WindowUi};
use egui_pop::api::{App, AppHandle};
use egui_pop::egui;

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
    let ticks: u64 = fields.get(11)?.parse::<u64>().ok()? + fields.get(12)?.parse::<u64>().ok()?;
    Some(ticks * 10)
}

/// Frames drawn, per window, and what each window reported.
#[derive(Default)]
struct Counts {
    root: Cell<u64>,
    a: Cell<u64>,
    b: Cell<u64>,
}

struct Demo {
    app: App,
    counts: Rc<Counts>,
    seen: u64,
    quit_sent: bool,
}

/// What both windows do each pass: focus changes, the first `opened`
/// line, `W` listing the windows.
fn common(ui: &egui::Ui, win: &WindowUi<'_>, frames: u64, focused: &Cell<Option<bool>>) {
    let now = ui.input(|i| i.viewport().focused);
    if now != focused.get() {
        focused.set(now);
        emit(serde_json::json!({"event": "focus", "window": win.id(), "focused": now}));
    }
    if frames == 3 {
        emit(serde_json::json!({
            "event": "opened",
            "window": win.id(),
            "embedded": win.embedded(),
            "geometry": win.app().window_geometry(win.id()),
        }));
    }
    if ui.input(|i| i.key_pressed(egui::Key::W)) {
        emit(serde_json::json!({"event": "windows", "windows": win.app().windows()}));
    }
}

fn open_a(app: &AppHandle, counts: Rc<Counts>, start: Instant) {
    let focused = Cell::new(None);
    app.open_window(
        WindowSpec::new("a", "Spike A").size([360.0, 240.0]),
        move |ui, win| {
            let n = counts.a.get() + 1;
            counts.a.set(n);
            common(ui, win, n, &focused);
            // Named after what it does; the main window it may move into
            // takes this title too.
            win.set_title("Spike A");
            ui.ctx().request_repaint();
            if n.is_multiple_of(30) {
                emit(serde_json::json!({
                    "event": "counts",
                    "t": start.elapsed().as_secs_f64(),
                    "root": counts.root.get(),
                    "windows": {"a": n, "b": counts.b.get()},
                    "in_root": ui.ctx().viewport_id() == egui::ViewportId::ROOT,
                    "cpu_ms": cpu_ms(),
                }));
            }
            if std::env::var_os("MW_AUTOPILOT").is_some() {
                match n {
                    120 => win.app().close_window(&WindowId::new("b")),
                    200 => win.app().close_window(&WindowId::main()),
                    300 => win.close(),
                    3000.. => {
                        emit(serde_json::json!({"event": "error", "error": "autopilot deadline"}));
                        std::process::exit(2);
                    }
                    _ => {}
                }
            }
            if ui.input(|i| i.key_pressed(egui::Key::F)) {
                emit(serde_json::json!({"event": "raise", "window": "b"}));
                win.app().raise_window(&WindowId::new("b"));
            }
            egui::CentralPanel::default().show(ui, |ui| {
                ui.heading("Window A");
                ui.label(format!("frames: {n}"));
                let t = ui.input(|i| i.time) as f32;
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(200.0, 20.0), egui::Sense::hover());
                let x = rect.left() + (t.sin() * 0.5 + 0.5) * rect.width();
                ui.painter().circle_filled(
                    egui::pos2(x, rect.center().y),
                    8.0,
                    egui::Color32::GOLD,
                );
            });
        },
    );
}

fn open_b(app: &AppHandle, counts: Rc<Counts>) {
    let focused = Cell::new(None);
    let typed = Rc::new(RefCell::new(String::new()));
    app.open_window(
        WindowSpec::new("b", "Spike B").size([360.0, 240.0]),
        move |ui, win| {
            let n = counts.b.get() + 1;
            counts.b.set(n);
            common(ui, win, n, &focused);
            let keys = typed.borrow().chars().count();
            // The title follows the content, live.
            if keys == 0 {
                win.set_title("Spike B");
            } else {
                win.set_title(format!("Spike B — {keys} typed"));
            }
            egui::CentralPanel::default().show(ui, |ui| {
                ui.heading("Window B");
                ui.label(format!("frames: {n}"));
                ui.text_edit_singleline(&mut *typed.borrow_mut());
            });
        },
    );
}

impl Demo {
    /// The journal's window events, as lines.
    fn relay_events(&mut self) {
        let h = self.app.handle().clone();
        for e in h.events_since(self.seen) {
            self.seen = e.seq;
            match e.name.as_str() {
                "window.closed" => emit(serde_json::json!({
                    "event": "closed",
                    "window": e.data["window"],
                    "frames": {"a": self.counts.a.get(), "b": self.counts.b.get()},
                })),
                "window.moved" => emit(serde_json::json!({
                    "event": "moved",
                    "window": e.data["window"],
                    "to": e.data["to"],
                })),
                _ => {}
            }
        }
    }
}

impl eframe::App for Demo {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.app.logic(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.counts.root.set(self.counts.root.get() + 1);
        if ui.input(|i| i.viewport().close_requested())
            && self.app.handle().windows().len() == 1
            && !self.quit_sent
        {
            self.quit_sent = true;
            emit(serde_json::json!({
                "event": "quit",
                "root_frames": self.counts.root.get(),
                "frames": {"a": self.counts.a.get(), "b": self.counts.b.get()},
                "cpu_ms": cpu_ms(),
            }));
        }
        self.app.show_wrapped(ui);
        self.relay_events();
    }

    fn on_exit(&mut self) {
        let _ = self.app.exit();
    }
}

fn main() -> eframe::Result {
    let state = std::env::var_os("MW_STATE").map(std::path::PathBuf::from);
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_app_id("egui-pop-multi-window")
            .with_inner_size([240.0, 120.0]),
        persist_window: false,
        ..Default::default()
    };
    match std::env::var("MW_PRESENT_MODE").as_deref() {
        Ok("autovsync") => {
            // eframe's default: `multi_window` would replace it on
            // Wayland, so the choice is made explicit with Fifo.
            options.wgpu_options.surface.present_mode = eframe::wgpu::PresentMode::Fifo;
        }
        Ok("autonovsync") => {
            options.wgpu_options.surface.present_mode = eframe::wgpu::PresentMode::AutoNoVsync;
        }
        Ok("fifo") => options.wgpu_options.surface.present_mode = eframe::wgpu::PresentMode::Fifo,
        Ok("mailbox") => {
            options.wgpu_options.surface.present_mode = eframe::wgpu::PresentMode::Mailbox;
        }
        _ => {}
    }
    let mut builder = App::builder()
        .name("Spike root")
        .plugin(egui_pop::api::windows::WindowsPlugin)
        .multi_window(true);
    if let Some(dir) = &state {
        builder = builder.persist(dir);
    }
    let mut chosen = options.clone();
    let mode = egui_pop::api::windows::present_mode(&mut chosen);
    builder.run_native_with("Spike root", options, move |app, _cc| {
        let counts = Rc::new(Counts::default());
        let start = Instant::now();
        emit(serde_json::json!({
            "event": "ready",
            "api": true,
            "pid": std::process::id(),
            "wayland": std::env::var_os("WAYLAND_DISPLAY").is_some(),
            "x11": std::env::var_os("DISPLAY").is_some(),
            "present_mode": format!("{mode:?}"),
            "state": state,
        }));
        open_a(app.handle(), counts.clone(), start);
        open_b(app.handle(), counts.clone());
        let seen = app.handle().last_event();
        Ok(Box::new(Demo {
            app,
            counts,
            seen,
            quit_sent: false,
        }))
    })
}
