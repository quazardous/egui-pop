//! The active window on the bus (ADR 0029, `pop.window.activate`): the window
//! the person works in is one truth, held by the registry. A press in a
//! window asks it active (`user`), and `window.activated` is said once
//! per change; `windows.list` marks it; a dialog opened from outside any
//! window comes up over it; a window closing while active hands the
//! activation back to the window worked in before it (`pop.window.focus`),
//! whatever its kind. A command asking a window to the front raises it
//! through the bus's `pop.window.raise`, the one place `ViewportCommand::Focus`
//! is sent.

use std::path::Path;

use egui::{ViewportCommand, ViewportId, pos2};
use egui_pop_api::bus::{RAISE, Reason, WindowKind, WindowRequest};
use egui_pop_api::dialogs::{self, DIALOGS, Info};
use egui_pop_api::host::Event;
use egui_pop_api::windows::{WindowId, WindowSpec};
use egui_pop_api::workbench;
use egui_pop_api::{App, AppHandle, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_model::ids::{PanelId, PluginId};
use egui_pop_model::panels::{Area, PanelSpec};
use egui_pop_test::windows::MultiWindow;

const LOG: PanelId = PanelId::from_static("demo.log");
const LOG_WINDOW: &str = "panel:demo.log";
const EDITOR: &str = "editor";

struct Demo;

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.panel(PanelSpec::new(LOG, "Log", Area::Bottom), |ui, _| {
            ui.label("log body");
        })?;
        Ok(())
    }
}

/// The main window, the log detached into its own window, and an
/// application window: their viewports.
fn three_windows() -> (MultiWindow, ViewportId, ViewportId) {
    let mut m = MultiWindow::new(|ctx| {
        App::builder()
            .name("Demo")
            .plugins(DefaultPlugins)
            .plugin(Demo)
            .build(ctx)
            .unwrap()
    });
    m.root_pass();
    assert!(m.app().detach_panel(&LOG).unwrap().ok());
    m.app()
        .open_window(WindowSpec::new(EDITOR, "Editor"), |ui, _| {
            ui.label("editor body");
        });
    m.root_pass();
    m.root_pass();
    let panel = workbench::window_id(&LOG);
    assert!(m.shown_immediate().contains(&panel));
    let editor = m.app().window_viewport(&WindowId::new(EDITOR)).unwrap();
    m.window_pass(editor);
    (m, panel, editor)
}

fn activated(app: &AppHandle) -> Vec<Event> {
    app.events_since(Default::default())
        .into_iter()
        .filter(|e| e.name == "window.activated")
        .collect()
}

/// The windows `windows.list` marks active.
fn listed_active(app: &AppHandle) -> Vec<String> {
    app.windows_list()
        .into_iter()
        .filter(|w| w.active)
        .map(|w| w.id)
        .collect()
}

/// A press somewhere in a window's body.
fn press(m: &mut MultiWindow, v: ViewportId) {
    m.click(v, pos2(320.0, 300.0));
}

/// Opens an information dialog from outside any window's pass and lets
/// it come up: the window its popup belongs to.
fn dialog_parent(m: &mut MultiWindow, editor: ViewportId) -> Option<String> {
    let id = m
        .app()
        .service(&DIALOGS)
        .unwrap()
        .open(Info::new("Done", "3 photos imported"));
    m.root_pass();
    m.window_pass(editor);
    m.root_pass();
    let popup = format!("{}{id}", dialogs::POPUP_PREFIX);
    m.app()
        .windows_list()
        .into_iter()
        .find(|w| w.id == popup)
        .and_then(|w| w.parent)
}

/// The acceptance test: a press in each window activates it once, said
/// once; the list agrees; a dialog comes up over the window worked in
/// last; the detached window closing activates the one before it.
#[test]
fn a_press_in_each_window_activates_it_once_and_a_close_hands_it_back() {
    let (mut m, panel, editor) = three_windows();
    assert_eq!(m.app().active_window_id(), "main");
    assert!(activated(m.app()).is_empty(), "nothing pressed yet");

    press(&mut m, panel);
    assert_eq!(m.app().active_window_id(), LOG_WINDOW);
    assert_eq!(m.app().active_window(), panel);
    assert_eq!(listed_active(m.app()), [LOG_WINDOW]);

    press(&mut m, editor);
    assert_eq!(m.app().active_window_id(), EDITOR);
    assert_eq!(m.app().active_window(), editor);
    assert_eq!(listed_active(m.app()), [EDITOR]);

    press(&mut m, ViewportId::ROOT);
    assert_eq!(m.app().active_window_id(), "main");
    assert_eq!(m.app().active_window(), ViewportId::ROOT);
    assert_eq!(listed_active(m.app()), ["main"]);

    let said = activated(m.app());
    let windows: Vec<&str> = said
        .iter()
        .map(|e| e.data["window"].as_str().unwrap())
        .collect();
    assert_eq!(windows, [LOG_WINDOW, EDITOR, "main"], "one event each");
    assert!(said.iter().all(|e| e.data["reason"] == "user"), "{said:?}");
    assert_eq!(said[1].data["from"], LOG_WINDOW);
    // The same window pressed again says nothing new.
    press(&mut m, ViewportId::ROOT);
    assert_eq!(activated(m.app()).len(), 3);

    // The detached window worked in, then closed by its title bar: the
    // main window, worked in before it, is active again, in that call.
    press(&mut m, panel);
    assert_eq!(m.app().active_window_id(), LOG_WINDOW);
    m.request_close(panel);
    m.root_pass();
    assert_eq!(m.app().active_window_id(), "main");
    assert_eq!(m.app().active_window(), ViewportId::ROOT);
    assert_eq!(listed_active(m.app()), ["main"]);
    let last = activated(m.app()).pop().unwrap();
    assert_eq!(last.data["window"], "main");
    assert_eq!(last.data["from"], LOG_WINDOW);
    assert_eq!(last.data["reason"], "system");

    // A dialog opened from outside any window comes up over the window
    // worked in last: the editor, then the main window. (A modal dialog
    // in the main window's group blocks its detached windows, so the
    // dialogs come last.)
    press(&mut m, editor);
    assert_eq!(dialog_parent(&mut m, editor).as_deref(), Some(EDITOR));
    press(&mut m, ViewportId::ROOT);
    assert_eq!(dialog_parent(&mut m, editor).as_deref(), Some("main"));
    assert!(m.app().broken_invariants().is_empty());
}

/// A window gaining the system's focus asks once (`system`), not at every
/// pass while it keeps it.
#[test]
fn a_window_gaining_the_focus_is_activated_once() {
    let (mut m, _, editor) = three_windows();
    m.focus(editor);
    m.window_pass(editor);
    m.window_pass(editor);
    m.root_pass();
    let said = activated(m.app());
    assert_eq!(said.len(), 1, "{said:?}");
    assert_eq!(said[0].data["window"], EDITOR);
    assert_eq!(said[0].data["reason"], "system");
    m.focus(ViewportId::ROOT);
    m.root_pass();
    m.root_pass();
    assert_eq!(m.app().active_window_id(), "main");
    assert_eq!(activated(m.app()).len(), 2);
}

/// An application window closing while active hands the activation to
/// the window worked in before it: the detached panel's, not the main.
#[test]
fn an_application_window_closing_activates_the_one_worked_in_before() {
    let (mut m, panel, editor) = three_windows();
    press(&mut m, panel);
    press(&mut m, editor);
    assert!(m.app().request_window_close(WindowRequest::new(
        EDITOR,
        WindowKind::App,
        "app",
        Reason::Command,
    )));
    assert_eq!(m.app().active_window_id(), LOG_WINDOW);
    assert_eq!(m.app().active_window(), panel);
}

/// A command asking a window to the front activates it and raises it,
/// through `pop.window.raise`; a window not open is refused.
#[test]
fn a_command_raises_the_window_through_the_bus() {
    let (mut m, _, editor) = three_windows();
    m.app().raise_window(&WindowId::new(EDITOR));
    assert_eq!(m.app().active_window_id(), EDITOR);
    let said = activated(m.app());
    assert_eq!(said.len(), 1);
    assert_eq!(said[0].data["reason"], "command");
    let did: Vec<String> = serde_json::from_value(said[0].data["did"].clone()).unwrap();
    assert!(did.iter().any(|d| d.starts_with(RAISE)), "{did:?}");
    m.root_pass();
    m.window_pass(editor);
    assert!(m.commands(editor).contains(&ViewportCommand::Focus));

    assert!(!m.app().request_window_activate(WindowRequest::new(
        "nowhere",
        WindowKind::App,
        "test",
        Reason::Command,
    )));
    assert!(
        m.app()
            .events_since(Default::default())
            .iter()
            .any(|e| e.name == "window.refused"
                && e.data["window"] == "nowhere"
                && e.data["request"] == "pop.window.activate")
    );
}

/// No crate sends `ViewportCommand::Focus` but the bus's `pop.window.raise`:
/// the active window is asked, never set behind the registry's back.
/// `egui-pop-core`'s overlay, used without an application (it has no
/// bus), is the one other place, and asks through a callback when an
/// application gives one.
#[test]
fn no_raw_focus_outside_the_bus() {
    const ALLOWED: &[&str] = &[
        "crates/egui-pop-api/src/bus.rs",
        "crates/egui-pop-api/src/bus/",
        // L0, without an application: `block_window_among`'s default.
        "crates/egui-pop-core/src/overlay.rs",
    ];
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut found = Vec::new();
    let mut dirs: Vec<_> = std::fs::read_dir(root.join("crates"))
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path().join("src"))
        .filter(|p| p.is_dir())
        .collect();
    while let Some(dir) = dirs.pop() {
        for e in std::fs::read_dir(&dir).unwrap().filter_map(Result::ok) {
            let path = e.path();
            if path.is_dir() {
                dirs.push(path);
                continue;
            }
            if path.extension().is_none_or(|x| x != "rs") {
                continue;
            }
            let rel = path
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            if ALLOWED.iter().any(|a| rel.starts_with(a)) {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            for (n, line) in text.lines().enumerate() {
                let code = line.split("//").next().unwrap_or("");
                if code.contains("ViewportCommand::Focus") {
                    found.push(format!("{rel}:{}: {}", n + 1, line.trim()));
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "raw `ViewportCommand::Focus` outside the bus: ask `pop.window.activate` (reason `command`) instead\n{}",
        found.join("\n")
    );
}
