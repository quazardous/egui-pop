//! Each application window's own shell (ADR 0025): its toasts, its
//! dialogs (blocking it alone, `modalOpen` per window), its palette, the
//! agent's calls naming a window, the window quick pick, and each
//! window's geometry kept under its own id when one moves into the main
//! window. Driven headless, each window's pass on its own
//! (`egui_pop_test::windows::MultiWindow`).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use egui::{Key, Modifiers, ViewportCommand, ViewportId};
use egui_pop_api::dialogs::{self, Confirm, DialogsPlugin};
use egui_pop_api::palette::{self, PalettePlugin};
use egui_pop_api::toasts::{self, Toast, ToastsPlugin};
use egui_pop_api::windows::{CMD_PICK, WindowSpec, WindowsPlugin};
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::{CommandId, PluginId};
use egui_pop_test::windows::MultiWindow;
use serde_json::{Value as Json, json};

const ASK: CommandId = CommandId::from_static("demo.ask");
const FREE: CommandId = CommandId::from_static("demo.free");
const ONLY_B: CommandId = CommandId::from_static("demo.only_b");
const DONE: CommandId = CommandId::from_static("demo.done");

/// The commands that ran, by id.
type Ran = Rc<RefCell<Vec<String>>>;

struct Demo(Ran);

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let (r1, r2, r3) = (self.0.clone(), self.0.clone(), self.0.clone());
        ctx.command(CommandSpec::new(ASK, "Ask", "Demo").key("Ctrl+D"), |c| {
            let d = c.app.service(&dialogs::DIALOGS).ok_or("no dialogs")?;
            Ok(json!(d.open(Confirm::new("Go on?", "Go on", DONE))))
        })?
        .command(
            CommandSpec::new(FREE, "Free", "Demo")
                .key("Ctrl+F")
                .when("!modalOpen"),
            move |_| {
                r1.borrow_mut().push("free".into());
                Ok(Json::Null)
            },
        )?
        .command(
            CommandSpec::new(ONLY_B, "Only B", "Demo").when("appWindow == 'b'"),
            move |_| {
                r2.borrow_mut().push("only_b".into());
                Ok(Json::Null)
            },
        )?
        .command(CommandSpec::new(DONE, "Done", "Demo"), move |_| {
            r3.borrow_mut().push("done".into());
            Ok(Json::Null)
        })?;
        Ok(())
    }
}

fn multi_with(persist: Option<std::path::PathBuf>) -> (MultiWindow, Ran) {
    let ran = Ran::default();
    let r = ran.clone();
    let m = MultiWindow::new(move |ctx| {
        let mut b = App::builder()
            .name("Demo")
            .plugin(DialogsPlugin)
            .plugin(ToastsPlugin)
            .plugin(PalettePlugin)
            .plugin(WindowsPlugin)
            .plugin(Demo(r));
        if let Some(p) = persist {
            b = b.persist(p);
        }
        b.build(ctx).unwrap()
    });
    (m, ran)
}

fn multi() -> (MultiWindow, Ran) {
    multi_with(None)
}

/// A window whose content raises a toast when asked (`Rc` state).
fn open(m: &MultiWindow, id: &str, toast: Option<Rc<Cell<bool>>>) -> ViewportId {
    let name = id.to_uppercase();
    m.app()
        .open_window(WindowSpec::new(id, name.clone()), move |ui, win| {
            if let Some(t) = &toast
                && t.replace(false)
            {
                let toasts = win.app().service(&toasts::TOASTS).unwrap();
                toasts.notify(Toast::info(&format!("from {name}")));
            }
            ui.label(name.as_str());
        });
    m.app()
        .window_viewport(&id.into())
        .unwrap_or(ViewportId::ROOT)
}

/// Opens A and B, runs a first pass of each.
fn two(m: &mut MultiWindow, toast_b: Option<Rc<Cell<bool>>>) -> (ViewportId, ViewportId) {
    open(m, "a", None);
    open(m, "b", toast_b);
    m.root_pass();
    let va = m.app().window_viewport(&"a".into()).unwrap();
    let vb = m.app().window_viewport(&"b".into()).unwrap();
    m.window_pass(va);
    m.window_pass(vb);
    (va, vb)
}

/// Presses a key in a window, then runs the main window's pass (where the
/// command runs) and the window's again.
fn key_in(m: &mut MultiWindow, v: ViewportId, key: Key, mods: Modifiers) {
    m.focus(v);
    m.press(v, key, mods);
    if v == ViewportId::ROOT {
        m.root_pass();
    } else {
        m.window_pass(v);
        m.root_pass();
        m.window_pass(v);
    }
}

#[test]
fn each_window_has_its_own_toasts() {
    let (mut m, _) = multi();
    let raise = Rc::new(Cell::new(false));
    let (_, vb) = two(&mut m, Some(raise.clone()));
    let toasts = m.app().service(&toasts::TOASTS).unwrap();
    // One from the main window (outside any window: the active one, the
    // main window), one from B's content.
    m.focus(ViewportId::ROOT);
    m.root_pass();
    toasts.notify(Toast::info("from main"));
    raise.set(true);
    m.window_pass(vb);
    m.root_pass();
    m.window_pass(vb);
    let shown = toasts.shown();
    let by = |text: &str| shown.iter().find(|t| t.toast.text == text).unwrap().clone();
    assert_eq!(by("from main").window.as_deref(), Some("main"));
    assert_eq!(by("from B").window.as_deref(), Some("b"));
    // Each drawn in its own window: both on screen at once.
    assert!(by("from main").since.is_some());
    assert!(by("from B").since.is_some());
    // Each window stacks its own: B's toasts do not take the main
    // window's places.
    for i in 0..toasts::SHOWN {
        toasts.notify_in(vb, Toast::info(&format!("b{i}")));
    }
    toasts.notify(Toast::info("main 2"));
    m.root_pass();
    m.window_pass(vb);
    let shown = toasts.shown();
    let main2 = shown.iter().find(|t| t.toast.text == "main 2").unwrap();
    assert!(main2.since.is_some(), "the main window has room of its own");
    let b_on_screen = shown
        .iter()
        .filter(|t| t.window.as_deref() == Some("b") && t.since.is_some())
        .count();
    assert_eq!(b_on_screen, toasts::SHOWN);
}

#[test]
fn a_dialog_blocks_only_the_window_it_was_opened_from() {
    let (mut m, ran) = multi();
    let (va, vb) = two(&mut m, None);
    // Ctrl+D in B opens a dialog there.
    key_in(&mut m, vb, Key::D, Modifiers::COMMAND);
    let d = m.app().service(&dialogs::DIALOGS).unwrap();
    assert_eq!(d.len(), 1);
    assert!(d.current_in(vb).is_some());
    assert!(d.current_in(ViewportId::ROOT).is_none());
    // `!modalOpen` refuses in B, runs in A and in the main window.
    key_in(&mut m, vb, Key::F, Modifiers::COMMAND);
    assert!(ran.borrow().is_empty(), "refused in B: {:?}", ran.borrow());
    key_in(&mut m, va, Key::F, Modifiers::COMMAND);
    key_in(&mut m, ViewportId::ROOT, Key::F, Modifiers::COMMAND);
    assert_eq!(*ran.borrow(), ["free", "free"]);
    // A dialog in the main window shows at the same time as B's.
    key_in(&mut m, ViewportId::ROOT, Key::D, Modifiers::COMMAND);
    m.root_pass();
    assert_eq!(d.len(), 2);
    assert!(d.current_in(ViewportId::ROOT).is_some());
    assert!(egui_pop_core::overlay::modal_windows(m.ctx()).contains(&vb));
    assert!(egui_pop_core::overlay::modal_windows(m.ctx()).contains(&ViewportId::ROOT));
    // An agent answers B's, naming B: the main window's stays.
    let f = m.app().debug_call_in(
        Some("b"),
        "dialogs",
        "answer",
        &json!({ "answer": "accept" }),
        Origin::User,
    );
    assert!(f.ok, "{}", f.message);
    m.window_pass(vb);
    m.root_pass();
    assert!(d.current_in(vb).is_none());
    assert!(d.current_in(ViewportId::ROOT).is_some());
    assert_eq!(ran.borrow().last().map(String::as_str), Some("done"));
    // The collector lists each window's dialog.
    let snap = m.app().debug_snapshot();
    let windows = snap["dialogs"]["windows"].as_array().unwrap();
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0]["window"], "main");
}

#[test]
fn a_closed_window_s_dialog_goes_to_the_main_window() {
    let (mut m, _) = multi();
    let (_, vb) = two(&mut m, None);
    key_in(&mut m, vb, Key::D, Modifiers::COMMAND);
    let d = m.app().service(&dialogs::DIALOGS).unwrap();
    assert!(d.current_in(vb).is_some());
    m.request_close(vb);
    m.window_pass(vb);
    m.root_pass();
    m.root_pass();
    assert!(d.current_in(ViewportId::ROOT).is_some());
}

#[test]
fn the_palette_opens_in_the_window_it_is_asked_in() {
    let (mut m, _) = multi();
    let (_, vb) = two(&mut m, None);
    key_in(&mut m, vb, Key::P, Modifiers::COMMAND | Modifiers::SHIFT);
    assert!(palette::is_open_in(m.app(), vb));
    assert!(!palette::is_open_in(m.app(), ViewportId::ROOT));
    let p = m.app().service(&palette::PALETTE).unwrap();
    assert_eq!(p.window_in(vb), Some(vb));
    // It offers B's commands: its context is B's.
    let cx = p.context_in(vb).unwrap();
    assert_eq!(cx.get("appWindow"), "b");
    assert!(m.app().applies(&ONLY_B, &cx));
    // The main window opens its own, B's staying open.
    key_in(
        &mut m,
        ViewportId::ROOT,
        Key::P,
        Modifiers::COMMAND | Modifiers::SHIFT,
    );
    assert!(palette::is_open_in(m.app(), ViewportId::ROOT));
    assert!(palette::is_open_in(m.app(), vb));
    assert!(
        !m.app()
            .applies(&ONLY_B, &p.context_in(ViewportId::ROOT).unwrap())
    );
    // Escape in B closes B's only.
    key_in(&mut m, vb, Key::Escape, Modifiers::NONE);
    assert!(!palette::is_open_in(m.app(), vb));
    assert!(palette::is_open_in(m.app(), ViewportId::ROOT));
}

#[test]
fn an_agent_names_the_window_a_call_is_for() {
    let (mut m, ran) = multi();
    let (_, vb) = two(&mut m, None);
    let run = |m: &MultiWindow, window: Option<&str>| {
        m.app().debug_call_in(
            window,
            "commands",
            "run",
            &json!({ "id": ONLY_B.as_str() }),
            Origin::User,
        )
    };
    assert!(run(&m, Some("b")).ok);
    m.root_pass();
    assert_eq!(*ran.borrow(), ["only_b"]);
    // In A, B's command does not apply.
    run(&m, Some("a"));
    m.root_pass();
    assert_eq!(ran.borrow().len(), 1);
    // Without a window: the active one (B, once worked in).
    m.focus(vb);
    m.window_pass(vb);
    run(&m, None);
    m.root_pass();
    assert_eq!(ran.borrow().len(), 2);
    let f = run(&m, Some("nowhere"));
    assert!(!f.ok);
    assert!(f.message.contains("nowhere"));
    // `describe` lists the windows by id and title.
    let caps = m.app().debug_capabilities();
    let ids: Vec<&str> = caps["windows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["main", "a", "b"]);
    assert_eq!(caps["windows"][2]["title"], "B");
    assert_eq!(caps["windows"][2]["active"], true);
    // A snapshot from B sees B's context.
    let snap = m.app().debug_snapshot_in(Some("b")).unwrap();
    let cx = snap["commands"]["context"].as_array().unwrap();
    assert!(
        cx.iter()
            .any(|k| k["key"] == "appWindow" && k["value"] == "b")
    );
}

#[test]
fn the_window_quick_pick_lists_the_windows_and_switches() {
    let (mut m, _) = multi();
    let (_, vb) = two(&mut m, None);
    m.focus(vb);
    m.window_pass(vb);
    let f = m.app().debug_call_in(
        Some("b"),
        "commands",
        "run",
        &json!({ "id": CMD_PICK.as_str() }),
        Origin::User,
    );
    assert!(f.ok, "{}", f.message);
    m.root_pass();
    m.window_pass(vb);
    let d = m.app().service(&dialogs::DIALOGS).unwrap();
    let Some((_, dialogs::Dialog::Choose(c))) = d.current_in(vb) else {
        panic!("a choice in B");
    };
    let labels: Vec<&str> = c.options.iter().map(|o| o.label.as_str()).collect();
    assert_eq!(labels, ["Demo", "A", "B"]);
    m.app().debug_call_in(
        Some("b"),
        "dialogs",
        "answer",
        &json!({ "answer": "choose", "index": 0 }),
        Origin::User,
    );
    m.window_pass(vb);
    m.root_pass();
    assert!(
        m.commands(ViewportId::ROOT)
            .contains(&ViewportCommand::Focus)
    );
}

#[test]
fn a_window_moved_into_the_main_one_keeps_its_own_geometry() {
    let dir = std::env::temp_dir().join(format!("egui-pop-window-scope-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    {
        let (mut m, _) = multi_with(Some(dir.clone()));
        open(&m, "a", None);
        m.set_inner_size(ViewportId::ROOT, [900.0, 700.0]);
        m.root_pass();
        let va = m.app().window_viewport(&"a".into()).unwrap();
        m.set_inner_size(va, [420.0, 260.0]);
        m.focus(va);
        m.window_pass(va);
        m.request_close(ViewportId::ROOT);
        m.root_pass();
        m.root_pass();
        assert_eq!(m.app().windows()[0].id.as_str(), "a");
        // Closing it now (it is the last) keeps the main window's size
        // under `main`, not under A.
        m.request_close(ViewportId::ROOT);
        m.root_pass();
        m.app().flush_documents();
    }
    let (m, _) = multi_with(Some(dir.clone()));
    assert_eq!(
        m.app().window_geometry(&"a".into()).unwrap().size,
        [420.0, 260.0]
    );
    let main = std::fs::read_to_string(dir.join("windows.json")).unwrap_or_default();
    let saved: Json = serde_json::from_str(&main).unwrap_or(Json::Null);
    assert_eq!(saved["main"]["size"], json!([900.0, 700.0]), "{main}");
    let _ = std::fs::remove_dir_all(&dir);
}
