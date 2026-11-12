//! Application windows (`AppHandle::open_window`): deferred viewports
//! declared from the main window's pass, their `Rc` content reached
//! through the thread-local shim, titles from the content, closing, the
//! main window's close moving another window into it, keys dispatched in
//! a window's own context, geometry kept per window.
//!
//! kittest has one window and embeds the others, so these tests drive an
//! egui context with real viewports, as eframe does without a GPU: the
//! main window's pass, then each deferred window's callback in a pass of
//! its own, with its own input (`egui_pop_test::windows::MultiWindow`).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use egui::{Key, Modifiers, ViewportCommand, ViewportId};
use egui_pop_api::windows::{WindowId, WindowSpec};
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::ids::{CommandId, PluginId};
use egui_pop_test::windows::MultiWindow;
use serde_json::Value as Json;

const HERE: CommandId = CommandId::from_static("demo.here");
const ONLY_B: CommandId = CommandId::from_static("demo.only_b");

/// Where the commands ran: the window worked in last, as each saw it.
type Ran = Rc<RefCell<Vec<(String, String)>>>;

struct Demo(Ran);

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let ran = self.0.clone();
        let ran_b = self.0.clone();
        ctx.command(
            CommandSpec::new(HERE, "Here", "Demo").key("Ctrl+G"),
            move |c| {
                ran.borrow_mut()
                    .push(("here".into(), c.app.active_app_window().to_string()));
                Ok(Json::Null)
            },
        )?
        .command(
            CommandSpec::new(ONLY_B, "Only B", "Demo")
                .key("Ctrl+B")
                .when("appWindow == 'b'"),
            move |c| {
                ran_b
                    .borrow_mut()
                    .push(("only_b".into(), c.app.active_app_window().to_string()));
                Ok(Json::Null)
            },
        )?;
        Ok(())
    }
}

fn multi() -> (MultiWindow, Ran) {
    let ran = Ran::default();
    let m = MultiWindow::new(|ctx| App::builder().plugin(Demo(ran.clone())).build(ctx).unwrap());
    (m, ran)
}

/// Opens a window whose content counts its passes and names itself after
/// the count: `Rc` state, no `Send`.
fn open_counting(m: &MultiWindow, id: &str) -> Rc<Cell<u32>> {
    let count = Rc::new(Cell::new(0));
    let c = count.clone();
    let name = id.to_uppercase();
    m.app().open_window(
        WindowSpec::new(id, format!("{name} (opening)")).size([300.0, 200.0]),
        move |ui, win| {
            c.set(c.get() + 1);
            win.set_title(format!("{name} — pass {}", c.get()));
            ui.label(format!("window {name}"));
        },
    );
    count
}

#[test]
fn windows_are_deferred_viewports_drawn_by_their_own_pass() {
    let (mut m, _) = multi();
    let a = open_counting(&m, "a");
    let b = open_counting(&m, "b");
    m.root_pass();
    let va = m.app().window_viewport(&"a".into()).unwrap();
    let vb = m.app().window_viewport(&"b".into()).unwrap();
    assert_eq!(m.declared(), vec![va, vb].into_iter().collect());
    // The main window's pass does not draw them: each draws on its own.
    assert_eq!((a.get(), b.get()), (0, 0));
    m.window_pass(va);
    m.window_pass(va);
    assert_eq!((a.get(), b.get()), (2, 0));
    m.window_pass(vb);
    assert_eq!(b.get(), 1);
    // The title comes from the content, sent to the system at once.
    assert!(
        m.commands(va)
            .contains(&ViewportCommand::Title("A — pass 2".into()))
    );
    let titles: Vec<String> = m.app().windows().into_iter().map(|w| w.title).collect();
    assert_eq!(titles[1..], ["A — pass 2", "B — pass 1"]);
    assert!(m.app().windows()[0].main);
}

#[test]
fn opening_an_open_window_again_raises_it() {
    let (mut m, _) = multi();
    open_counting(&m, "a");
    m.root_pass();
    let again = open_counting(&m, "a");
    m.root_pass();
    let va = m.app().window_viewport(&"a".into()).unwrap();
    m.window_pass(va);
    // The first content stays; the window is asked to the front.
    assert_eq!(again.get(), 0);
    assert!(m.commands(va).contains(&ViewportCommand::Focus));
    assert_eq!(m.app().windows().len(), 2);
}

#[test]
fn a_close_request_closes_the_window_and_keeps_the_others() {
    let (mut m, _) = multi();
    open_counting(&m, "a");
    open_counting(&m, "b");
    m.root_pass();
    let vb = m.app().window_viewport(&"b".into()).unwrap();
    let mark = m.app().last_event();
    m.request_close(vb);
    m.window_pass(vb);
    m.root_pass();
    assert_eq!(m.declared().len(), 1);
    let ids: Vec<String> = m.app().windows().iter().map(|w| w.id.to_string()).collect();
    assert_eq!(ids, ["main", "a"]);
    let events = m.app().events_since(mark);
    assert!(
        events
            .iter()
            .any(|e| e.name == "window.closed" && e.data["window"] == "b")
    );
}

#[test]
fn a_content_may_keep_its_window_open() {
    let (mut m, _) = multi();
    let asked = Rc::new(Cell::new(false));
    let a = asked.clone();
    m.app()
        .open_window(WindowSpec::new("doc", "Unsaved"), move |_, win| {
            if win.close_requested() {
                a.set(true);
                win.keep_open();
            }
        });
    m.root_pass();
    let v = m.app().window_viewport(&"doc".into()).unwrap();
    m.request_close(v);
    m.window_pass(v);
    m.root_pass();
    assert!(asked.get());
    assert_eq!(m.declared().len(), 1);
}

#[test]
fn closing_the_main_window_moves_the_last_worked_in_window_into_it() {
    let (mut m, _) = multi();
    let a = open_counting(&m, "a");
    let b = open_counting(&m, "b");
    m.root_pass();
    let va = m.app().window_viewport(&"a".into()).unwrap();
    let vb = m.app().window_viewport(&"b".into()).unwrap();
    m.focus(vb);
    m.window_pass(vb);
    m.focus(va);
    m.window_pass(va);
    let before = (a.get(), b.get());
    m.request_close(ViewportId::ROOT);
    m.root_pass();
    // Nothing quits: the close is cancelled.
    assert!(
        m.commands(ViewportId::ROOT)
            .contains(&ViewportCommand::CancelClose)
    );
    // A, worked in last, is now drawn in the main window, with its title.
    m.root_pass();
    assert!(a.get() > before.0);
    assert!(
        m.commands(ViewportId::ROOT)
            .iter()
            .any(|c| matches!(c, ViewportCommand::Title(t) if t.starts_with("A — ")))
    );
    let windows = m.app().windows();
    assert_eq!(windows[0].id, WindowId::new("a"));
    assert!(windows[0].main);
    assert_eq!(windows.len(), 2);
    assert_eq!(m.declared(), [vb].into_iter().collect());
    assert_eq!(b.get(), before.1);
    // Closing the main window with no other open quits as usual.
    m.request_close(vb);
    m.window_pass(vb);
    m.request_close(ViewportId::ROOT);
    m.root_pass();
    assert!(
        !m.commands(ViewportId::ROOT)
            .contains(&ViewportCommand::CancelClose)
    );
}

#[test]
fn keys_are_dispatched_in_their_window_with_its_context() {
    let (mut m, ran) = multi();
    open_counting(&m, "a");
    open_counting(&m, "b");
    m.root_pass();
    let va = m.app().window_viewport(&"a".into()).unwrap();
    let vb = m.app().window_viewport(&"b".into()).unwrap();
    m.focus(vb);
    m.press(vb, Key::G, Modifiers::COMMAND);
    m.window_pass(vb);
    // The command runs before the main window's next pass.
    m.root_pass();
    assert_eq!(*ran.borrow(), [("here".to_string(), "b".to_string())]);
    ran.borrow_mut().clear();
    // `when: "appWindow == 'b'"` holds in B, not in A.
    m.focus(va);
    m.press(va, Key::B, Modifiers::COMMAND);
    m.window_pass(va);
    m.root_pass();
    assert!(ran.borrow().is_empty());
    m.focus(vb);
    m.press(vb, Key::B, Modifiers::COMMAND);
    m.window_pass(vb);
    m.root_pass();
    assert_eq!(*ran.borrow(), [("only_b".to_string(), "b".to_string())]);
}

#[test]
fn geometry_is_kept_per_window_across_runs() {
    let dir = std::env::temp_dir().join(format!("egui-pop-app-windows-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    {
        let d = dir.clone();
        let mut m = MultiWindow::new(move |ctx| App::builder().persist(&d).build(ctx).unwrap());
        open_counting(&m, "a");
        m.root_pass();
        let va = m.app().window_viewport(&"a".into()).unwrap();
        m.set_inner_size(va, [420.0, 260.0]);
        m.window_pass(va);
        m.request_close(va);
        m.window_pass(va);
        m.app().flush_documents();
    }
    let d = dir.clone();
    let m = MultiWindow::new(move |ctx| App::builder().persist(&d).build(ctx).unwrap());
    let g = m.app().window_geometry(&"a".into()).expect("kept");
    assert_eq!(g.size, [420.0, 260.0]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_window_menu_lists_the_windows_by_title_and_switches() {
    use egui_pop_api::windows::{CMD_SWITCH, LIST_WINDOWS, WindowsPlugin};
    let mut m = MultiWindow::new(|ctx| {
        App::builder()
            .name("Demo")
            .plugin(WindowsPlugin)
            .build(ctx)
            .unwrap()
    });
    open_counting(&m, "a");
    m.root_pass();
    let va = m.app().window_viewport(&"a".into()).unwrap();
    m.focus(va);
    m.window_pass(va);
    let items = m.app().list_items(LIST_WINDOWS, &m.app().when_context());
    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    assert_eq!(labels, ["Demo", "A — pass 1"]);
    assert!(items[1].checked, "A, worked in last, is ticked");
    m.app()
        .run(CMD_SWITCH, serde_json::json!({ "window": "main" }));
    m.root_pass();
    assert!(
        m.commands(ViewportId::ROOT)
            .contains(&ViewportCommand::Focus)
    );
}

#[test]
fn raising_the_main_window_sends_a_moved_window_back_to_its_own() {
    use egui_pop_api::windows::{CMD_SWITCH, WindowsPlugin};
    let mut m = MultiWindow::new(|ctx| {
        App::builder()
            .name("Demo")
            .plugin(WindowsPlugin)
            .build(ctx)
            .unwrap()
    });
    let a = open_counting(&m, "a");
    m.root_pass();
    let va = m.app().window_viewport(&"a".into()).unwrap();
    m.focus(va);
    m.window_pass(va);
    m.request_close(ViewportId::ROOT);
    m.root_pass();
    m.root_pass();
    assert_eq!(m.app().windows()[0].id, WindowId::new("a"));
    assert!(m.declared().is_empty());
    let mark = m.app().last_event();
    // Window > Main window: `main` is still a window to switch to.
    m.app()
        .run(CMD_SWITCH, serde_json::json!({ "window": "main" }));
    m.root_pass();
    // The shell is back in the main window, under its own id and title.
    let windows = m.app().windows();
    assert_eq!(windows.len(), 2);
    assert_eq!(windows[0].id, WindowId::main());
    assert_eq!(windows[0].title, "Demo");
    assert!(windows[0].active, "the main window, asked to the front");
    assert!(
        m.commands(ViewportId::ROOT)
            .contains(&ViewportCommand::Title("Demo".into()))
    );
    // A is a window of its own again, with its title, drawn by its own
    // pass and no longer by the main window's.
    assert_eq!(windows[1].id, WindowId::new("a"));
    assert!(windows[1].title.starts_with("A — "));
    assert!(!windows[1].main);
    assert_eq!(m.app().window_viewport(&"a".into()), Some(va));
    assert_eq!(m.declared(), [va].into_iter().collect());
    let before = a.get();
    m.root_pass();
    assert_eq!(a.get(), before);
    m.window_pass(va);
    assert_eq!(a.get(), before + 1);
    let events = m.app().events_since(mark);
    assert!(
        events
            .iter()
            .any(|e| e.name == "window.moved" && e.data["window"] == "a" && e.data["to"] == "a")
    );
    assert!(
        events
            .iter()
            .any(|e| e.name == "window.opened" && e.data["window"] == "main")
    );
    // Its keys are dispatched in its own pass again.
    m.focus(va);
    m.window_pass(va);
    assert_eq!(m.app().active_app_window(), WindowId::new("a"));
}

#[test]
fn quitting_with_another_window_open_closes_everything() {
    let (mut m, _) = multi();
    open_counting(&m, "a");
    m.root_pass();
    let va = m.app().window_viewport(&"a".into()).unwrap();
    m.focus(va);
    m.window_pass(va);
    // `app.quit` run from window A closes the main window, not A.
    m.app().run(egui_pop_api::app::CMD_QUIT, Json::Null);
    m.root_pass();
    assert!(m.app().quitting());
    assert!(
        m.commands(ViewportId::ROOT)
            .contains(&ViewportCommand::Close)
    );
    assert!(!m.commands(va).contains(&ViewportCommand::Close));
    // The close is seen: nothing moves into the main window, the close
    // goes on and the application ends.
    m.request_close(ViewportId::ROOT);
    m.root_pass();
    assert!(
        !m.commands(ViewportId::ROOT)
            .contains(&ViewportCommand::CancelClose)
    );
    assert_eq!(m.app().windows()[0].id, WindowId::main());
    assert!(!m.app().quitting());
}
