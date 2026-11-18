//! The shell's windows on the bus (ADR 0029, slice 4): opening an
//! application window is its `pop.window.open`; the main window's close,
//! whatever asked it, is its `pop.window.close`, which moves another window
//! into it as a `pop.window.reparent` when one is open, and closes every other
//! window by its own `pop.window.close` when it does close; quitting is that
//! close with the reason `quit`. The registry is the one list throughout.

use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;

use egui::{ViewportCommand, ViewportId};
use egui_pop_api::admin::{self, APP_WINDOW, AdminPlugin, CMD_OPEN};
use egui_pop_api::bus::{Reason, STEP_WINDOW_CLOSE, WindowKind};
use egui_pop_api::host::Event;
use egui_pop_api::steps::Before;
use egui_pop_api::windows::{WindowId, WindowSpec, WindowsPlugin};
use egui_pop_api::{App, AppHandle, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_model::ids::PluginId;
use egui_pop_test::windows::MultiWindow;
use serde_json::json;

/// Vetoes the main window's close while `.0` says so (unsaved work).
struct Unsaved(Rc<Cell<bool>>);

impl Plugin for Unsaved {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo.unsaved")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let unsaved = self.0.clone();
        ctx.before(STEP_WINDOW_CLOSE, move |_, args| {
            if args["kind"] == "main" && unsaved.get() {
                Before::Veto("unsaved work".into())
            } else {
                Before::Go
            }
        });
        Ok(())
    }
}

fn multi_with(unsaved: Rc<Cell<bool>>) -> MultiWindow {
    MultiWindow::new(move |ctx| {
        App::builder()
            .name("Demo")
            .plugins(DefaultPlugins)
            .plugin(WindowsPlugin)
            .plugin(AdminPlugin)
            .plugin(Unsaved(unsaved.clone()))
            .build(ctx)
            .unwrap()
    })
}

fn multi() -> MultiWindow {
    multi_with(Rc::default())
}

fn open(m: &mut MultiWindow, id: &str) -> ViewportId {
    m.app()
        .open_window(WindowSpec::new(id, id.to_uppercase()), |ui, _| {
            ui.label("content");
        });
    m.root_pass();
    let v = m.app().window_viewport(&WindowId::new(id)).unwrap();
    m.focus(v);
    m.window_pass(v);
    v
}

fn events(app: &AppHandle, name: &str) -> Vec<Event> {
    app.events_since(Default::default())
        .into_iter()
        .filter(|e| e.name == name)
        .collect()
}

fn listed(app: &AppHandle) -> Vec<(String, WindowKind, String)> {
    app.windows_list()
        .into_iter()
        .map(|w| (w.id, w.kind, w.owner))
        .collect()
}

fn did(data: &serde_json::Value) -> Vec<String> {
    data.as_array()
        .into_iter()
        .flatten()
        .filter_map(|d| d.as_str().map(String::from))
        .collect()
}

/// The listeners of a `pop.window.reparent`, in their order: the registry,
/// the place (when the move left places to keep: moving in), the focus,
/// then the invariants.
fn assert_reparent_order(did: &[String], registry: &str, place: bool) {
    let mut want = vec![registry.to_string()];
    if place {
        want.push("pop.window.place: each keeps its place".into());
    }
    want.push("pop.window.focus: the main window is active".into());
    assert_eq!(did.len(), want.len() + 1, "{did:?}");
    assert_eq!(did[..want.len()], want[..]);
    assert!(
        did[want.len()].starts_with("pop.window.invariants: "),
        "{did:?}"
    );
}

#[test]
fn opening_an_application_window_is_a_window_open_with_its_owner() {
    let mut m = multi();
    open(&mut m, "a");
    let opened = events(m.app(), "window.opened");
    assert_eq!(opened.len(), 1);
    assert_eq!(opened[0].data["owner"], "app");
    let did = did(&opened[0].data["did"]);
    assert_eq!(did[0], "pop.window.registry: holds it", "{did:?}");
    assert!(did[1].starts_with("pop.window.invariants: "), "{did:?}");
    // An owner of its own: the registry holds it under that owner.
    m.app().request_window_open(
        WindowSpec::new("b", "B"),
        "demo.import",
        Reason::Command,
        |ui, _| {
            ui.label("cards");
        },
    );
    assert_eq!(
        listed(m.app()),
        [
            ("main".into(), WindowKind::Main, "app".into()),
            ("a".into(), WindowKind::App, "app".into()),
            ("b".into(), WindowKind::App, "demo.import".into()),
        ]
    );
}

#[test]
fn closing_the_main_window_with_a_window_open_moves_it_in_as_a_reparent() {
    let mut m = multi();
    let va = open(&mut m, "a");
    let vb = open(&mut m, "b");
    m.focus(va);
    m.window_pass(va);
    m.request_close(ViewportId::ROOT);
    m.root_pass();
    assert!(
        m.commands(ViewportId::ROOT)
            .contains(&ViewportCommand::CancelClose)
    );
    // One `pop.window.close` of the main window, answered by a move.
    let close = events(m.app(), "window.close");
    assert_eq!(close.len(), 1);
    assert_eq!(close[0].data["args"]["kind"], "main");
    assert_eq!(close[0].data["result"]["moved_in"], "a");
    let reparent = events(m.app(), "window.reparent");
    assert_eq!(reparent.len(), 1);
    assert_eq!(reparent[0].data["args"]["window"], "a");
    assert_eq!(reparent[0].data["args"]["parent"], "main");
    assert_reparent_order(
        &did(&reparent[0].data["result"]["did"]),
        "pop.window.registry: `a` drawn in the main window",
        true,
    );
    // Nothing closed; A's own window left the registry, the main window
    // stays, and the backend agrees.
    assert!(
        events(m.app(), "window.closed")
            .iter()
            .all(|e| e.data.get("reason").is_none())
    );
    assert_eq!(
        listed(m.app()),
        [
            ("main".into(), WindowKind::Main, "app".into()),
            ("b".into(), WindowKind::App, "app".into()),
        ]
    );
    assert_eq!(m.app().active_window(), ViewportId::ROOT);
    m.root_pass();
    assert_eq!(m.declared(), [vb].into_iter().collect());
    assert_eq!(m.app().windows()[0].id, WindowId::new("a"));
}

#[test]
fn raising_the_main_window_reparents_the_content_back_to_its_own_window() {
    let mut m = multi();
    let va = open(&mut m, "a");
    m.request_close(ViewportId::ROOT);
    m.root_pass();
    m.app().raise_window(&WindowId::main());
    let reparent = events(m.app(), "window.reparent");
    assert_eq!(reparent.len(), 2, "back in the call that asked");
    assert!(reparent[1].data["args"].get("parent").is_none());
    assert_reparent_order(
        &did(&reparent[1].data["result"]["did"]),
        "pop.window.registry: `a` in a window of its own",
        false,
    );
    assert_eq!(
        listed(m.app()),
        [
            ("main".into(), WindowKind::Main, "app".into()),
            ("a".into(), WindowKind::App, "app".into()),
        ]
    );
    m.root_pass();
    assert_eq!(m.declared(), [va].into_iter().collect());
    assert_eq!(m.app().windows()[0].id, WindowId::main());
}

#[test]
fn quitting_closes_every_window_by_its_own_close_then_the_main_one() {
    let mut m = multi();
    open(&mut m, "a");
    m.app().run(CMD_OPEN, json!({ "mode": "window" }));
    m.root_pass();
    m.root_pass();
    assert!(admin::is_open(m.app()));
    m.app().quit();
    // Each window's own close, its owner's state with it: the Settings
    // app closed.
    assert!(!admin::is_open(m.app()));
    let closed: Vec<(String, String)> = events(m.app(), "window.closed")
        .into_iter()
        .map(|e| {
            (
                e.data["window"].as_str().unwrap_or("").to_string(),
                e.data["reason"].as_str().unwrap_or("").to_string(),
            )
        })
        .collect();
    assert_eq!(
        closed,
        [
            ("a".to_string(), "quit".to_string()),
            (APP_WINDOW.to_string(), "quit".to_string()),
            ("main".to_string(), "quit".to_string()),
        ]
    );
    assert!(listed(m.app()).is_empty(), "{:?}", listed(m.app()));
    let close = events(m.app(), "window.close");
    let main = close.last().unwrap();
    assert_eq!(
        did(&main.data["result"]["did"])[0],
        "pop.window.children: 2 windows closed"
    );
    // The system is asked, once; its close request is then let through.
    m.root_pass();
    assert!(
        m.commands(ViewportId::ROOT)
            .contains(&ViewportCommand::Close)
            || m.app().quitting()
    );
    m.request_close(ViewportId::ROOT);
    m.root_pass();
    assert!(
        !m.commands(ViewportId::ROOT)
            .contains(&ViewportCommand::CancelClose)
    );
    assert_eq!(events(m.app(), "window.close").len(), close.len());
    assert!(!m.app().quitting());
}

#[test]
fn a_vetoed_main_close_leaves_everything_as_it_was() {
    let unsaved = Rc::new(Cell::new(true));
    let mut m = multi_with(unsaved.clone());
    let va = open(&mut m, "a");
    let before = listed(m.app());
    // The system's close, with a window open: vetoed before any move.
    m.request_close(ViewportId::ROOT);
    m.root_pass();
    assert!(
        m.commands(ViewportId::ROOT)
            .contains(&ViewportCommand::CancelClose)
    );
    assert_eq!(listed(m.app()), before);
    assert!(events(m.app(), "window.reparent").is_empty());
    // Quitting: vetoed too, nothing closed, not quitting.
    m.app().quit();
    assert!(!m.app().quitting());
    assert_eq!(listed(m.app()), before);
    let refused = events(m.app(), "window.refused");
    assert_eq!(refused.len(), 2);
    assert!(
        refused
            .iter()
            .all(|e| e.data["vetoed_by"] == "demo.unsaved")
    );
    m.root_pass();
    assert_eq!(m.declared(), [va].into_iter().collect());
    // Saved: the close goes on, the window moves in.
    unsaved.set(false);
    m.request_close(ViewportId::ROOT);
    m.root_pass();
    assert_eq!(events(m.app(), "window.reparent").len(), 1);
}

#[test]
fn the_main_window_closed_alone_is_its_window_close_asked_once() {
    let mut m = multi();
    m.app().close_window(&WindowId::main());
    let closed = events(m.app(), "window.closed");
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0].data["window"], "main");
    assert_eq!(closed[0].data["reason"], "command");
    assert!(listed(m.app()).is_empty());
    // The system's close request that follows is let through, not asked
    // again.
    m.request_close(ViewportId::ROOT);
    m.root_pass();
    assert!(
        !m.commands(ViewportId::ROOT)
            .contains(&ViewportCommand::CancelClose)
    );
    assert_eq!(events(m.app(), "window.close").len(), 1);
}

/// No path of the shell opens, closes or moves a window but its requests
/// on the bus: the raw viewport commands of these paths are in the bus's
/// shell module only.
#[test]
fn the_shell_has_no_direct_window_path() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for file in ["windows.rs", "app.rs", "frame.rs"] {
        let text = std::fs::read_to_string(src.join(file)).unwrap();
        for raw in ["ViewportCommand::Close", "CancelClose", "take_quitting()"] {
            assert!(
                !text.contains(raw),
                "{file} uses `{raw}`: ask the bus (`pop.window.close`)"
            );
        }
        for call in ["move_into_root(", "move_out_of_root(", "take_going_home("] {
            let calls = text.matches(call).count();
            let defined = text.matches(&format!("fn {call}")).count();
            assert_eq!(
                calls, defined,
                "{file} calls `{call}`: ask the bus (`pop.window.reparent`)"
            );
        }
    }
}
