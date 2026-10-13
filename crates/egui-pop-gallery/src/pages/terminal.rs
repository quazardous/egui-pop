//! The terminal's page: a build log on a console source (deterministic,
//! the snapshot's fixture), an echoing demo program whose input a hook
//! rewrites, and on the desktop a real shell in a pseudo-terminal, on
//! demand.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use egui::Ui;
use egui_pop::api::steps::Before;
use egui_pop::api::stories::Story;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::editors::terminal::steps::{TERMINAL_EXIT, TERMINAL_INPUT, TERMINAL_PLUGIN};
use egui_pop::editors::terminal::{
    Console, Sink, Spawn, Terminal, TerminalSignal, TerminalSource, TerminalState,
};
use egui_pop::model::ids::{PluginId, StoryId};
use serde_json::{Value as Json, json};

use super::section;
use crate::shell::SHELL;

pub const TERMINAL_PAGE: PluginId = PluginId::from_static("gallery.terminal");

/// The context keys the hooks read.
const UPPER: &str = "galleryTerminalUpper";
const KEEP: &str = "galleryTerminalKeep";

/// The build log the console starts with.
pub const LOG: &[&str] = &[
    "\x1b[1;32m   Compiling\x1b[0m exposure v0.3.1 (/work/exposure)",
    "\x1b[1;32m   Compiling\x1b[0m gallery v0.1.0 (/work/gallery)",
    "\x1b[1;33mwarning\x1b[0m\x1b[1m: unused variable: `unused`\x1b[0m",
    "  \x1b[1;34m-->\x1b[0m src/main.rs:11:9",
    "   \x1b[1;34m|\x1b[0m",
    "\x1b[1;34m11\x1b[0m \x1b[1;34m|\x1b[0m     let unused = 3;",
    "   \x1b[1;34m|\x1b[0m         \x1b[1;33m^^^^^^\x1b[0m \x1b[1;33mhelp: prefix it with an underscore: `_unused`\x1b[0m",
    "\x1b[1;31merror[E0425]\x1b[0m\x1b[1m: cannot find value `shadow` in this scope\x1b[0m",
    "   \x1b[2mfor more, see https://doc.rust-lang.org/error_codes/E0425.html\x1b[0m",
    "Styles: \x1b[1mbold\x1b[0m \x1b[2mdim\x1b[0m \x1b[3mitalic\x1b[0m \x1b[4munderline\x1b[0m \x1b[7minverse\x1b[0m",
    "ANSI:   \x1b[30;47m 0 \x1b[0m\x1b[41m 1 \x1b[42m 2 \x1b[43m 3 \x1b[44m 4 \x1b[45m 5 \x1b[46m 6 \x1b[47m 7 \x1b[0m \x1b[90mbright:\x1b[0m \x1b[91m9\x1b[92m10\x1b[93m11\x1b[94m12\x1b[95m13\x1b[96m14\x1b[97m15\x1b[0m",
];

/// The xterm 256-colour ramp and an RGB gradient, one line each.
fn colour_lines() -> Vec<String> {
    let mut cube = String::from("256:    ");
    for i in (16..232).step_by(6) {
        cube.push_str(&format!("\x1b[48;5;{i}m "));
    }
    cube.push_str("\x1b[0m");
    let mut rgb = String::from("RGB:    ");
    for i in 0..36 {
        let r = 255 - i * 7;
        let b = i * 7;
        rgb.push_str(&format!("\x1b[48;2;{r};96;{b}m "));
    }
    rgb.push_str("\x1b[0m");
    vec![cube, rgb]
}

/// A demo program: echoes what is typed, answers each line.
#[derive(Clone, Debug, Default)]
pub struct Echo {
    inner: Arc<Mutex<EchoInner>>,
}

#[derive(Debug, Default)]
struct EchoInner {
    sink: Option<Sink>,
    line: String,
}

impl TerminalSource for Echo {
    fn name(&self) -> String {
        "echo".into()
    }

    fn writable(&self) -> bool {
        true
    }

    fn start(&mut self, _spawn: &Spawn, sink: Sink) -> Result<Json, String> {
        sink.output(b"Type a line and press Enter.\r\n\x1b[1;32m>\x1b[0m ");
        let mut i = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        i.sink = Some(sink);
        i.line.clear();
        Ok(json!({}))
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        let mut i = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let mut out = String::new();
        for c in String::from_utf8_lossy(bytes).chars() {
            match c {
                '\r' => {
                    let line = std::mem::take(&mut i.line);
                    out.push_str(&format!(
                        "\r\nyou said: \x1b[36m{line}\x1b[0m\r\n\x1b[1;32m>\x1b[0m "
                    ));
                }
                '\x7f' if i.line.pop().is_some() => out.push_str("\x08 \x08"),
                '\x7f' => {}
                c if c >= ' ' => {
                    i.line.push(c);
                    out.push(c);
                }
                _ => {}
            }
        }
        if let Some(s) = &i.sink {
            s.output(out.as_bytes());
        }
        Ok(())
    }
}

/// The page's state.
#[derive(Debug)]
struct Page {
    log: TerminalState,
    console: Console,
    echo: TerminalState,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    shell: Option<TerminalState>,
    written: usize,
    signals: Vec<String>,
}

impl Default for Page {
    fn default() -> Self {
        let console = Console::new();
        for l in LOG {
            console.line(l);
        }
        for l in colour_lines() {
            console.line(&l);
        }
        Page {
            log: TerminalState::new("gallery.build-log", console.clone()),
            console,
            echo: TerminalState::new("gallery.echo", Echo::default()),
            shell: None,
            written: 0,
            signals: Vec::new(),
        }
    }
}

impl Page {
    fn signal(&mut self, from: &str, s: &TerminalSignal) {
        let text = match s {
            TerminalSignal::Started => "started".to_string(),
            TerminalSignal::Ended { code } => format!("ended ({code:?})"),
            TerminalSignal::Refused { step, reason } => format!("{step} refused: {reason}"),
            TerminalSignal::Copied { chars } => format!("copied {chars} characters"),
            TerminalSignal::Link { url } => format!("opened {url}"),
            TerminalSignal::Left => "left (Shift+Escape)".to_string(),
        };
        self.signals.push(format!("{from}: {text}"));
        let n = self.signals.len();
        if n > 5 {
            self.signals.drain(..n - 5);
        }
    }
}

/// The terminal page.
#[derive(Debug, Default)]
pub struct TerminalPage;

impl Plugin for TerminalPage {
    fn id(&self) -> PluginId {
        TERMINAL_PAGE
    }

    fn title(&self) -> String {
        "Terminal".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL, TERMINAL_PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        // A redirect: the demo program's input upper-cased.
        ctx.before(TERMINAL_INPUT, |app, args| {
            if args["terminal"] == "gallery.echo" && app.context().get(UPPER) == "true" {
                let mut a = args.clone();
                a["text"] = json!(args["text"].as_str().unwrap_or("").to_uppercase());
                Before::Redirect(a)
            } else {
                Before::Go
            }
        });
        // A veto: sessions stay open.
        ctx.before(TERMINAL_EXIT, |app, args| {
            if args["reason"] == "close" && app.context().get(KEEP) == "true" {
                Before::Veto("sessions are kept open".into())
            } else {
                Before::Go
            }
        });
        let page = Rc::new(RefCell::new(Page::default()));
        ctx.story(
            Story::new(StoryId::from_static("terminal"), "Terminal", move |ui, app| {
                terminal_page(ui, app, &mut page.borrow_mut())
            })
            .description("A terminal: ANSI colours from the theme, scrollback, selection and copy, links (Ctrl+click), every key to the program (Shift+Escape leaves); a console source for logs, a demo program whose input a hook rewrites, and a shell in a pseudo-terminal on the desktop.")
            .group("Components", 80),
        )?;
        Ok(())
    }
}

fn flag(ui: &mut Ui, app: &AppHandle, key: &str, label: &str) {
    let mut on = app.context().get(key) == "true";
    if ui.checkbox(&mut on, label).changed() {
        app.set_context(key, if on { "true" } else { "false" });
    }
}

fn terminal_page(ui: &mut Ui, app: &AppHandle, p: &mut Page) {
    ui.label("Click a terminal to type in it: every key goes to its program, Escape and Tab included; Shift+Escape leaves. Drag to select, Ctrl+Shift+C copies, the wheel scrolls back, Ctrl+click opens a link.");
    section(ui, "Build log (a console source)");
    ui.horizontal_wrapped(|ui| {
        if ui.button("Write a line").clicked() {
            p.written += 1;
            p.console
                .line(&format!("\x1b[1;32m    Finished\x1b[0m step {}", p.written));
        }
        if ui.button("Finish with code 0").clicked() {
            p.console.finish(Some(0));
        }
        if ui.button("Clear").clicked() {
            p.log.clear();
        }
    });
    let out = Terminal::new()
        .label("Build log")
        .app(app)
        .height(230.0)
        .show(ui, &p.log);
    for s in &out.signals {
        p.signal("build log", s);
    }
    section(ui, "Echo (a demo program)");
    ui.horizontal_wrapped(|ui| {
        flag(
            ui,
            app,
            UPPER,
            "Upper-case the input (a redirect on terminal.input)",
        );
        flag(
            ui,
            app,
            KEEP,
            "Keep sessions open (a veto on terminal.exit)",
        );
        if ui.button("Close the session").clicked()
            && let Err(e) = p.echo.close(Some(app), "user")
        {
            p.signals.push(format!("echo: terminal.exit refused: {e}"));
        }
        if ui.button("Restart").clicked() {
            let _ = p.echo.restart(Some(app), "user");
        }
    });
    let out = Terminal::new()
        .label("Echo")
        .app(app)
        .height(110.0)
        .show(ui, &p.echo);
    for s in &out.signals {
        p.signal("echo", s);
    }
    shell(ui, app, p);
    section(ui, "Signals");
    if p.signals.is_empty() {
        ui.weak("Nothing yet.");
    }
    for l in &p.signals {
        ui.monospace(l);
    }
}

/// A shell in a pseudo-terminal, on demand (desktop only).
#[cfg(not(target_arch = "wasm32"))]
fn shell(ui: &mut Ui, app: &AppHandle, p: &mut Page) {
    use egui_pop::editors::terminal::PtySource;
    section(ui, "Shell (a pseudo-terminal)");
    match &p.shell {
        None => {
            if ui.button("Open a shell").clicked() {
                p.shell = Some(TerminalState::new("gallery.shell", PtySource::shell()));
            }
        }
        Some(t) => {
            let t = t.clone();
            let out = Terminal::new()
                .label("Shell")
                .app(app)
                .height(240.0)
                .show(ui, &t);
            for s in &out.signals {
                p.signal("shell", s);
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn shell(_ui: &mut Ui, _app: &AppHandle, _p: &mut Page) {}
