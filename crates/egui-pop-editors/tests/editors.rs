//! The editors with egui_kittest: typing, the keyboard, Escape leaving,
//! search and replace, undo, the gutter, folding, 100 000 lines, the
//! steps and commands an application and an agent reach, code assist.

use std::time::Instant;

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::steps::Before;
use egui_pop_api::{App, AppHandle, Ctx, Plugin, PluginError};
use egui_pop_editors::assist::{
    AssistProvider, CodeAction, CompletionItem, Hover, Reply, Request, Signature, SignatureHelp,
    Suggestion,
};
use egui_pop_editors::buffer::{Change, Pos};
use egui_pop_editors::markers::{Diagnostic, Severity};
use egui_pop_editors::selection::Selection;
use egui_pop_editors::steps::{
    CMD_FIND, CMD_READ, CMD_REPLACE, CMD_SAVE, CMD_WRITE, EDIT_CHANGE, EDIT_SAVE, EditorsPlugin,
};
use egui_pop_editors::{CodeEditor, EditorState, Signal, TextSurface};
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::PluginId;
use serde_json::json;

/// An editor alone, no application.
struct Alone {
    state: EditorState,
    code: bool,
    budget: usize,
    signals: Vec<Signal>,
}

fn alone(state: EditorState, code: bool) -> Harness<'static, Alone> {
    Harness::builder()
        .with_size(egui::vec2(600.0, 400.0))
        .build_ui_state(
            |ui, a: &mut Alone| {
                egui_pop_core::focus::install(ui.ctx());
                let out = if a.code {
                    CodeEditor::new()
                        .label("Code")
                        .highlight_budget(a.budget)
                        .show(ui, &a.state)
                } else {
                    TextSurface::new().label("Notes").show(ui, &a.state)
                };
                a.signals.extend(out.signals);
                let _ = ui.button("After");
            },
            Alone {
                state,
                code,
                budget: 100,
                signals: Vec::new(),
            },
        )
}

fn click_editor<S>(h: &mut Harness<'_, S>, label: &str) {
    h.get_by_label(label).click();
    h.run();
}

#[test]
fn typing_moving_and_escape_leaving() {
    let mut h = alone(EditorState::new("notes", "hello"), false);
    h.run();
    click_editor(&mut h, "Notes");
    h.key_press(Key::End);
    h.event(egui::Event::Text(" world".into()));
    h.run();
    assert_eq!(h.state().state.text(), "hello world");
    h.key_press(Key::Enter);
    h.event(egui::Event::Text("x".into()));
    h.run();
    assert_eq!(h.state().state.text(), "hello world\nx");
    // Shift+Home selects to the line's start; typing replaces it.
    h.key_press_modifiers(Modifiers::SHIFT, Key::Home);
    h.event(egui::Event::Text("y".into()));
    h.run();
    assert_eq!(h.state().state.text(), "hello world\ny");
    // Undo, redo.
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run();
    assert_eq!(h.state().state.text(), "hello world\nx");
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    h.run();
    assert_eq!(h.state().state.text(), "hello world\ny");
    // Plain keys stay with the editor (Tab inserts), Escape leaves it.
    h.key_press(Key::Tab);
    h.run();
    assert_eq!(h.state().state.text(), "hello world\ny    ");
    assert!(h.ctx.text_edit_focused(), "a text input for egui-pop");
    h.key_press(Key::Escape);
    h.run();
    assert!(h.ctx.memory(|m| m.focused()).is_none(), "Escape left it");
    // The text did not change.
    assert_eq!(h.state().state.text(), "hello world\ny    ");
    assert!(
        h.state()
            .signals
            .iter()
            .any(|s| matches!(s, Signal::Changed { kind, .. } if kind == "undo"))
    );
}

/// The text the last frame put on the clipboard, if any.
fn copied<S>(h: &Harness<'_, S>) -> Option<String> {
    h.output()
        .platform_output
        .commands
        .iter()
        .find_map(|c| match c {
            egui::OutputCommand::CopyText(text) => Some(text.clone()),
            _ => None,
        })
}

#[test]
fn a_refused_cut_leaves_the_clipboard_alone() {
    let mut h = alone(EditorState::new("notes", "keep me").read_only(true), false);
    h.run();
    click_editor(&mut h, "Notes");
    // Copy reaches the clipboard even when read-only...
    h.event(egui::Event::Copy);
    h.step();
    assert_eq!(copied(&h).as_deref(), Some("keep me\n"));
    // ...but a cut whose delete is refused copies nothing.
    h.event(egui::Event::Cut);
    h.step();
    assert_eq!(copied(&h), None, "a refused cut must not copy");
    h.run();
    assert_eq!(h.state().state.text(), "keep me");
    // Accepted, the cut deletes and copies.
    let _ = h.state().state.clone().read_only(false);
    h.event(egui::Event::Cut);
    h.step();
    assert_eq!(copied(&h).as_deref(), Some("keep me\n"));
    h.run();
    assert_eq!(h.state().state.text(), "");
}

#[test]
fn search_and_replace_from_the_bar() {
    let mut h = alone(EditorState::new("notes", "one two one\nthree one"), false);
    h.run();
    click_editor(&mut h, "Notes");
    h.key_press_modifiers(Modifiers::COMMAND, Key::H);
    h.run();
    h.get_by_label("Find").type_text("one");
    h.run();
    assert!(h.query_by_label("1 of 3").is_some());
    h.get_by_label("Replace with").focus();
    h.run();
    h.get_by_label("Replace with").type_text("1");
    h.run();
    h.get_by_label("Replace all").click();
    h.run();
    assert_eq!(
        h.state().state.text(),
        "1 two 1\nthree 1",
        "{:?} {}",
        h.state().signals,
        h.state().state.describe()
    );
    // One undo step for all of them.
    h.state().state.undo(None, "test").unwrap();
    assert_eq!(h.state().state.text(), "one two one\nthree one");
    h.get_by_label("Close the search").click();
    h.run();
    assert!(
        h.query_by_label("Find").is_none(),
        "{}",
        h.state().state.describe()
    );
}

#[test]
fn the_gutter_folds_markers_and_virtualisation() {
    let mut text =
        String::from("fn main() {\n    let a = 1;\n    if a > 0 {\n        go();\n    }\n}\n");
    for i in 0..100_000 {
        text.push_str(&format!("let x{i} = {i};\n"));
    }
    let state = EditorState::new("big.rs", &text).language("rs");
    state
        .set_diagnostics(
            None,
            vec![Diagnostic::new(1, Severity::Warning, "unused variable `a`").columns(8, 9)],
        )
        .unwrap();
    state.toggle_breakpoint(3);
    let mut h = alone(state, true);
    let t = Instant::now();
    h.run();
    let first = t.elapsed();
    let s = h.state().state.clone();
    assert_eq!(s.line_count(), 100_007);
    // A screenful laid out: the accessibility tree holds one editor, and
    // the highlighter parsed the lines in view only.
    let m = s.highlighter_name();
    assert_eq!(m, "syntect: Rust");
    // Jump to the end and type: each pass stays a screenful's work.
    click_editor(&mut h, "Code");
    h.key_press_modifiers(Modifiers::COMMAND, Key::End);
    // The highlighter parses its way down within its budget per pass:
    // every pass stays short, the lines in view come in colour after.
    let mut typing = std::time::Duration::ZERO;
    for _ in 0..10 {
        let t = Instant::now();
        h.step();
        typing = typing.max(t.elapsed());
    }
    h.event(egui::Event::Text("// end".into()));
    let t = Instant::now();
    h.step();
    typing = typing.max(t.elapsed());
    assert!(
        s.text().ends_with("// end"),
        "{:?} {:?}",
        s.selection(),
        &s.text()[..40]
    );
    assert!(
        typing.as_secs_f32() < 2.0 && first.as_secs_f32() < 5.0,
        "first {first:?}, typing {typing:?}"
    );
    // Folding the first function hides its body.
    assert!(s.fold(0));
    assert_eq!(s.folded(), vec![0]);
    h.step();
    // Diagnostics and breakpoints follow an inserted line.
    s.set_selection(Selection::caret(Pos::new(0, 0)));
    s.unfold(0);
    s.insert(
        None,
        "// top\n",
        egui_pop_editors::history::EditKind::Typing,
        "test",
    )
    .unwrap();
    assert_eq!(s.diagnostics()[0].line, 2);
    assert_eq!(s.breakpoints(), vec![4]);
    h.step();
}

/// Refuses edits that would add the word "forbidden"; saves into a
/// buffer of its own.
struct Guard;

impl Plugin for Guard {
    fn id(&self) -> PluginId {
        PluginId::from_static("guard")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.before(EDIT_CHANGE, |_, args| {
            let adds = args["changes"].as_array().is_some_and(|c| {
                c.iter()
                    .any(|c| c["text"].as_str().is_some_and(|t| t.contains("forbidden")))
            });
            if adds {
                Before::Veto("not that word".into())
            } else {
                Before::Go
            }
        });
        ctx.replace(EDIT_SAVE, |_, args| {
            Ok(json!({ "written": args["editor"] }))
        });
        Ok(())
    }
}

struct WithApp {
    app: Option<App>,
    state: EditorState,
    signals: Vec<Signal>,
}

impl WithApp {
    fn handle(&self) -> AppHandle {
        self.app.as_ref().unwrap().handle().clone()
    }
}

fn with_app(state: EditorState) -> Harness<'static, WithApp> {
    Harness::builder()
        .with_size(egui::vec2(600.0, 400.0))
        .build_ui_state(
            |ui, w: &mut WithApp| {
                if w.app.is_none() {
                    w.app = Some(
                        App::builder()
                            .plugin(EditorsPlugin)
                            .plugin(Guard)
                            .build(ui.ctx())
                            .unwrap(),
                    );
                }
                let h = w.handle();
                let out = CodeEditor::new().label("Code").app(&h).show(ui, &w.state);
                w.signals.extend(out.signals);
            },
            WithApp {
                app: None,
                state,
                signals: Vec::new(),
            },
        )
}

#[test]
fn steps_veto_and_save_and_commands_drive_the_buffer() {
    let mut h = with_app(EditorState::new("main.rs", "let a = 1;"));
    h.run();
    click_editor(&mut h, "Code");
    h.key_press(Key::End);
    h.event(egui::Event::Text(" forbidden".into()));
    h.run();
    assert_eq!(h.state().state.text(), "let a = 1;");
    assert!(
        h.state()
            .signals
            .iter()
            .any(|s| matches!(s, Signal::Refused { reason, .. } if reason == "not that word"))
    );
    h.event(egui::Event::Text(" ok".into()));
    h.run();
    assert_eq!(h.state().state.text(), "let a = 1; ok");
    // Ctrl+S runs edit.save (replaced by the guard): the text is clean.
    assert!(h.state().state.is_dirty());
    // (The keymap carries Ctrl+S under App::show: the gallery's tests;
    // here the command it runs.)
    h.state_mut()
        .app
        .as_mut()
        .unwrap()
        .run(&CMD_SAVE, json!({ "editor": "main.rs" }), Origin::User)
        .now()
        .unwrap()
        .unwrap();
    h.run();
    assert!(!h.state().state.is_dirty());
    // An agent reads and writes through commands.
    let app = h.state_mut().app.as_mut().unwrap();
    let agent = || Origin::Agent("test".into());
    let read = app
        .run(&CMD_READ, json!({ "editor": "main.rs" }), agent())
        .now()
        .unwrap()
        .unwrap();
    assert_eq!(read["text"], "let a = 1; ok");
    let wrote = app.run(
        &CMD_WRITE,
        json!({ "editor": "main.rs", "from": { "line": 0, "col": 0 }, "to": { "line": 0, "col": 3 }, "text": "const" }),
        agent(),
    );
    assert!(wrote.now().unwrap().is_ok());
    let refused = app
        .run(
            &CMD_WRITE,
            json!({ "editor": "main.rs", "text": "forbidden" }),
            agent(),
        )
        .now()
        .unwrap();
    assert_eq!(refused, Err("not that word".into()));
    let found = app
        .run(
            &CMD_FIND,
            json!({ "editor": "main.rs", "query": "a" }),
            agent(),
        )
        .now()
        .unwrap()
        .unwrap();
    assert_eq!(found["matches"], 1);
    app.run(
        &CMD_REPLACE,
        json!({ "editor": "main.rs", "query": "ok", "replacement": "fine", "all": true }),
        agent(),
    )
    .now()
    .unwrap()
    .unwrap();
    h.run();
    assert_eq!(h.state().state.text(), "const a = 1; fine");
    // The trace says who changed what.
    let traces = h.state().handle().step_traces();
    let last = traces.iter().rev().find(|t| t.step == EDIT_CHANGE).unwrap();
    assert_eq!(last.args["kind"], "replace");
    assert_eq!(last.args["origin"], "agent:test");
    // The collectors are there.
    let snapshot = h.state().handle().debug_snapshot();
    for c in ["edit_changes", "edit_saves", "code_diagnostics"] {
        assert!(!snapshot[c].is_null(), "{c} in {snapshot}");
    }
    assert_eq!(
        snapshot["edit_saves"]["saves"][0]["result"]["written"],
        "main.rs"
    );
}

#[test]
fn every_step_is_declared() {
    let app = App::builder()
        .plugin(EditorsPlugin)
        .build(&egui::Context::default())
        .unwrap();
    let declared: Vec<String> = app
        .handle()
        .steps()
        .into_iter()
        .map(|s| s.spec.id.to_string())
        .collect();
    for s in [
        "edit.change",
        "edit.save",
        "code.diagnostics",
        "code.insert_completion",
        "code.accept_suggestion",
    ] {
        assert!(declared.iter().any(|d| d == s), "{s}");
    }
}

/// A provider answering from a thread, as a language server would.
struct Slow;

impl AssistProvider for Slow {
    fn name(&self) -> String {
        "slow".into()
    }

    fn suggests_inline(&self) -> bool {
        true
    }

    fn complete(&mut self, _: &Request, reply: Reply<Vec<CompletionItem>>) {
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(20));
            reply.deliver(vec![
                CompletionItem::new("println")
                    .insert("println!")
                    .detail("macro"),
                CompletionItem::new("print"),
            ]);
        });
    }

    fn signature_help(&mut self, _: &Request, reply: Reply<Option<SignatureHelp>>) {
        reply.deliver(Some(SignatureHelp {
            signatures: vec![Signature {
                label: "fn mix(a: f32, b: f32)".into(),
                parameters: vec![7..13, 15..21],
                documentation: String::new(),
            }],
            active: 0,
            parameter: 0,
        }));
    }

    fn hover(&mut self, _: &Request, reply: Reply<Option<Hover>>) {
        reply.deliver(Some(Hover {
            text: "a hover".into(),
            code: None,
            span: None,
        }));
    }

    fn code_actions(
        &mut self,
        _: &Request,
        _: egui_pop_editors::buffer::Span,
        reply: Reply<Vec<CodeAction>>,
    ) {
        reply.deliver(vec![CodeAction {
            title: "Add a comment".into(),
            changes: vec![Change::insert(Pos::new(0, 0), "// ")],
            preferred: true,
        }]);
    }

    fn inline_suggestion(&mut self, r: &Request, reply: Reply<Option<Suggestion>>) {
        let line = r.state.lines(r.at.line, r.at.line + 1);
        reply.deliver(line.ends_with("mix(").then(|| Suggestion {
            text: "0.5, 1.0)".into(),
        }));
    }
}

fn settle<S>(h: &mut Harness<'_, S>, done: impl Fn(&Harness<'_, S>) -> bool) {
    for _ in 0..200 {
        h.run();
        if done(h) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("never settled");
}

#[test]
fn assist_completes_suggests_and_acts_through_its_steps() {
    let state = EditorState::new("main.rs", "").assist(Box::new(Slow));
    let mut h = with_app(state);
    h.run();
    click_editor(&mut h, "Code");
    h.event(egui::Event::Text("pr".into()));
    // An answer from a thread, shown at a later pass.
    settle(&mut h, |h| h.query_by_label("println").is_some());
    h.key_press(Key::Enter);
    h.run();
    assert_eq!(h.state().state.text(), "println!");
    let events: Vec<String> = h
        .state()
        .handle()
        .events_since(0)
        .into_iter()
        .map(|e| e.name)
        .collect();
    for e in [
        "code.completion_requested",
        "code.completion_accepted",
        "code.cursor_moved",
        "code.insert_completion",
        "edit.change",
    ] {
        assert!(events.iter().any(|n| n == e), "{e} in {events:?}");
    }
    // Signature help on `(`, ghost text accepted with Tab.
    h.event(egui::Event::Text("; mix(".into()));
    h.run();
    h.run();
    assert!(h.query_by_label_contains("fn mix").is_some());
    h.key_press(Key::Tab);
    h.run();
    assert_eq!(h.state().state.text(), "println!; mix(0.5, 1.0)");
    // Code actions with Ctrl+. (here without the keymap: no App::show).
    let mut a = h.state_mut().app.take().unwrap();
    a.run(
        &egui_pop_editors::steps::CMD_ACTIONS,
        json!({ "editor": "main.rs" }),
        Origin::User,
    );
    h.state_mut().app = Some(a);
    h.run();
    assert!(h.query_by_label("Add a comment").is_some());
    h.key_press(Key::Enter);
    h.run();
    assert!(h.state().state.text().starts_with("// println!"));
}
