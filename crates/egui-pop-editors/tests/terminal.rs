//! The terminal with egui_kittest: keys routed to the program (Escape
//! and Tab included, Shift+Escape leaving), copy, the steps an
//! application hooks, the commands an agent runs, and a real program in
//! a pseudo-terminal (feature `pty`, on Unix).

use std::sync::{Arc, Mutex};

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::steps::Before;
use egui_pop_api::{App, AppHandle, Ctx, Plugin, PluginError};
use egui_pop_editors::terminal::steps::{
    CMD_CLEAR, CMD_CLOSE, CMD_READ, CMD_SEND, TERMINAL_EXIT, TERMINAL_INPUT, TERMINAL_SPAWN,
};
use egui_pop_editors::terminal::{
    CONSOLE_PENDING, Cell, Console, InputKind, Sink, Spawn, Terminal, TerminalPlugin,
    TerminalSignal, TerminalSource, TerminalState,
};
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::PluginId;
use serde_json::{Value as Json, json};

/// A program that echoes what it is sent, and keeps it.
#[derive(Clone, Default)]
struct Echo {
    received: Arc<Mutex<Vec<u8>>>,
    sink: Arc<Mutex<Option<Sink>>>,
}

impl Echo {
    fn received(&self) -> String {
        String::from_utf8_lossy(&self.received.lock().unwrap()).into_owned()
    }
}

impl TerminalSource for Echo {
    fn name(&self) -> String {
        "echo".into()
    }

    fn writable(&self) -> bool {
        true
    }

    fn start(&mut self, _spawn: &Spawn, sink: Sink) -> Result<Json, String> {
        sink.output(b"$ ");
        *self.sink.lock().unwrap() = Some(sink);
        Ok(json!({}))
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.received.lock().unwrap().extend_from_slice(bytes);
        let printable: Vec<u8> = bytes
            .iter()
            .flat_map(|b| {
                if *b == b'\r' {
                    vec![b'\r', b'\n']
                } else {
                    vec![*b]
                }
            })
            .filter(|b| *b >= b' ' || *b == b'\r' || *b == b'\n')
            .collect();
        if let Some(s) = self.sink.lock().unwrap().as_ref() {
            s.output(&printable);
        }
        Ok(())
    }
}

struct Alone {
    state: TerminalState,
    signals: Vec<TerminalSignal>,
}

fn alone(state: TerminalState) -> Harness<'static, Alone> {
    Harness::builder()
        .with_size(egui::vec2(600.0, 300.0))
        .build_ui_state(
            |ui, a: &mut Alone| {
                egui_pop_core::focus::install(ui.ctx());
                let out = Terminal::new()
                    .label("Shell")
                    .height(200.0)
                    .show(ui, &a.state);
                a.signals.extend(out.signals);
                let _ = ui.button("After");
            },
            Alone {
                state,
                signals: Vec::new(),
            },
        )
}

#[test]
fn every_key_goes_to_the_program_and_shift_escape_leaves() {
    let echo = Echo::default();
    let mut h = alone(TerminalState::new("shell", echo.clone()));
    h.run();
    assert!(h.state().state.is_running());
    assert!(h.state().signals.contains(&TerminalSignal::Started));
    h.get_by_label("Shell").click();
    h.run();
    h.event(egui::Event::Text("ls".into()));
    h.key_press(Key::Enter);
    h.key_press(Key::Escape);
    h.key_press(Key::Tab);
    h.key_press(Key::ArrowUp);
    h.key_press_modifiers(Modifiers::CTRL, Key::D);
    h.run();
    assert_eq!(echo.received(), "ls\r\x1b\t\x1b[A\x04");
    assert!(h.state().state.text().starts_with("$ ls"));
    // Still focused after Escape and Tab: Shift+Escape leaves.
    h.key_press_modifiers(Modifiers::SHIFT, Key::Escape);
    h.run();
    assert!(h.state().signals.contains(&TerminalSignal::Left));
    h.event(egui::Event::Text("x".into()));
    h.run();
    assert!(!echo.received().contains('x'));
}

#[test]
fn ctrl_shift_c_without_a_selection_does_not_interrupt() {
    let echo = Echo::default();
    let mut h = alone(TerminalState::new("shell", echo.clone()));
    h.run();
    h.get_by_label("Shell").click();
    h.run();
    // Under winit, Ctrl+Shift+C arrives as a copy event with Shift held.
    h.event_modifiers(egui::Event::Copy, Modifiers::CTRL | Modifiers::SHIFT);
    h.run();
    assert_eq!(echo.received(), "", "Ctrl+Shift+C only copies");
    // Ctrl+C with nothing selected still interrupts.
    h.event_modifiers(egui::Event::Copy, Modifiers::CTRL);
    h.run();
    assert_eq!(echo.received(), "\x03");
}

#[test]
fn a_bracketed_paste_cannot_end_its_bracket() {
    let echo = Echo::default();
    let t = TerminalState::new("shell", echo.clone());
    t.spawn(None, "user").unwrap();
    // The program asks for bracketed paste.
    assert!(
        echo.sink
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .output(b"\x1b[?2004h")
    );
    // The pasted text tries to close the bracket and type a command;
    // removing one marker must not join the rest into another.
    t.send(
        None,
        "safe\x1b[201~rm -rf ~\r\x1b[20\x1b[201~0~x",
        InputKind::Paste,
        "user",
    )
    .unwrap();
    assert_eq!(
        echo.received(),
        "\x1b[200~saferm -rf ~\rx\x1b[201~",
        "only the terminal's own markers bracket the paste"
    );
}

#[test]
fn a_console_is_read_only_and_copies_its_selection() {
    let log = Console::new();
    log.line("\x1b[31merror\x1b[0m: something broke");
    log.line("see https://example.org/help");
    let mut h = alone(TerminalState::new("log", log.clone()));
    h.run();
    let t = h.state().state.clone();
    assert!(!t.is_writable());
    assert_eq!(t.lines(0, 1), vec!["error: something broke"]);
    h.get_by_label("Shell").click();
    h.run();
    t.select(Cell::new(0, 0), Cell::new(0, 5));
    h.key_press_modifiers(Modifiers::CTRL | Modifiers::SHIFT, Key::C);
    h.run();
    assert!(
        h.state()
            .signals
            .contains(&TerminalSignal::Copied { chars: 5 })
    );
    log.finish(Some(0));
    h.run();
    h.run();
    assert!(
        h.state()
            .signals
            .contains(&TerminalSignal::Ended { code: Some(0) })
    );
    assert!(t.text().ends_with("Process exited with code 0"));
}

/// Hooks an application puts on the terminal's steps.
struct Hooks;

impl Plugin for Hooks {
    fn id(&self) -> PluginId {
        PluginId::from_static("terminal_hooks")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.before(TERMINAL_SPAWN, |_, args| {
            if args["terminal"] == "forbidden" {
                Before::Veto("no shell here".into())
            } else {
                Before::Go
            }
        });
        // Agents' input is upper-cased.
        ctx.before(TERMINAL_INPUT, |_, args| {
            if args["kind"] == "agent" {
                let mut a = args.clone();
                a["text"] = json!(args["text"].as_str().unwrap_or("").to_uppercase());
                Before::Redirect(a)
            } else {
                Before::Go
            }
        });
        ctx.before(TERMINAL_EXIT, |_, args| {
            if args["terminal"] == "kept" && args["reason"] == "close" {
                Before::Veto("a job is running".into())
            } else {
                Before::Go
            }
        });
        Ok(())
    }
}

struct WithApp {
    app: Option<App>,
    terminals: Vec<TerminalState>,
    signals: Vec<TerminalSignal>,
}

impl WithApp {
    fn handle(&self) -> AppHandle {
        self.app.as_ref().unwrap().handle().clone()
    }

    fn run(
        &mut self,
        command: &egui_pop_model::ids::CommandId,
        args: Json,
    ) -> Result<Json, String> {
        self.app
            .as_mut()
            .unwrap()
            .run(command, args, Origin::Agent("t".into()))
            .now()
            .expect("answered now")
    }
}

fn with_app(terminals: Vec<TerminalState>) -> Harness<'static, WithApp> {
    Harness::builder()
        .with_size(egui::vec2(600.0, 500.0))
        .build_ui_state(
            |ui, w: &mut WithApp| {
                if w.app.is_none() {
                    w.app = Some(
                        App::builder()
                            .plugin(TerminalPlugin)
                            .plugin(Hooks)
                            .build(ui.ctx())
                            .unwrap(),
                    );
                }
                let h = w.handle();
                for t in &w.terminals {
                    let out = Terminal::new()
                        .label(t.id())
                        .app(&h)
                        .height(100.0)
                        .show(ui, t);
                    w.signals.extend(out.signals);
                }
            },
            WithApp {
                app: None,
                terminals,
                signals: Vec::new(),
            },
        )
}

#[test]
fn steps_veto_redirect_and_the_commands_drive_the_terminal() {
    let echo = Echo::default();
    let kept = Echo::default();
    let mut h = with_app(vec![
        TerminalState::new("shell", echo.clone()),
        TerminalState::new("forbidden", Echo::default()),
        TerminalState::new("kept", kept),
    ]);
    h.run();
    let forbidden = h.state().terminals[1].clone();
    assert!(!forbidden.is_running());
    assert!(forbidden.text().contains("Not started: no shell here"));
    // An agent sends input: the redirect upper-cases it.
    let sent = h
        .state_mut()
        .run(
            &CMD_SEND,
            json!({ "terminal": "shell", "text": "make", "enter": true }),
        )
        .unwrap();
    assert_eq!(sent["bytes"], 5);
    assert_eq!(echo.received(), "MAKE\r");
    let read = h
        .state_mut()
        .run(&CMD_READ, json!({ "terminal": "shell" }))
        .unwrap();
    assert_eq!(read["text"], "$ MAKE");
    assert_eq!(read["running"], true);
    h.state_mut()
        .run(&CMD_CLEAR, json!({ "terminal": "shell" }))
        .unwrap();
    assert_eq!(h.state().terminals[0].text(), "");
    // Closing is vetoed for one, done for the other.
    let refused = h.state_mut().run(&CMD_CLOSE, json!({ "terminal": "kept" }));
    assert_eq!(refused, Err("a job is running".into()));
    h.state_mut()
        .run(&CMD_CLOSE, json!({ "terminal": "shell" }))
        .unwrap();
    assert!(!h.state().terminals[0].is_running());
    assert!(h.state().terminals[2].is_running());
    let snapshot = h.state().handle().debug_snapshot();
    let log = snapshot["terminal_io"]["log"].as_array().unwrap();
    assert!(log.iter().any(|e| e["step"] == "terminal.exit"), "{log:?}");
    assert_eq!(
        snapshot["terminal_sessions"]["terminals"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}

#[cfg(all(unix, feature = "pty"))]
#[test]
fn a_program_runs_in_a_pseudo_terminal_and_exits() {
    use egui_pop_editors::terminal::{CommandLine, PtySource};
    let t = TerminalState::new(
        "sh",
        PtySource::new(CommandLine::new("sh").arg("-c").arg("printf hello; exit 3")),
    )
    .size(10, 50);
    let mut h = alone(t.clone());
    for _ in 0..500 {
        let _ = h.run_ok();
        if t.has_ended() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(t.exit_code(), Some(3), "{}", t.text());
    assert!(t.text().contains("hello"), "{}", t.text());
}

#[test]
fn a_console_whose_terminal_is_not_drawn_keeps_its_latest_output_only() {
    // No terminal started it (its host never drew it); a worker writes
    // on: about twice what it keeps.
    let log = Console::new();
    let line = "x".repeat(99);
    let writer = log.clone();
    let l = line.clone();
    std::thread::spawn(move || {
        for _ in 0..20_000 {
            writer.line(&l);
        }
    })
    .join()
    .unwrap();
    // Each line is 101 bytes (`\r\n`); what went is counted.
    assert!(log.waiting() <= CONSOLE_PENDING, "{}", log.waiting());
    assert!(log.dropped() > 0);
    assert_eq!(log.waiting() as u64 + log.dropped(), 20_000 * 101);
    assert_eq!(log.dropped() % 101, 0, "dropped by whole lines");
    // Started at last, the terminal shows the latest lines (wide enough
    // that a line is one row: `text` has a row per line).
    let term = TerminalState::new("log", log.clone()).size(24, 120);
    term.spawn(None, "user").unwrap();
    let text = term.text();
    assert!(
        text.ends_with(&line),
        "{:?}",
        &text[text.len().saturating_sub(300)..]
    );
}
