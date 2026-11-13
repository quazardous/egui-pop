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

/// An application drawing its own frame (`FramePlugin::own`), with the
/// window menu, and window A open and worked in, 420 by 260 at (300, 120).
fn own_frame_with_a() -> (MultiWindow, ViewportId) {
    use egui_pop_api::frame::FramePlugin;
    use egui_pop_api::windows::WindowsPlugin;
    let mut m = MultiWindow::new(|ctx| {
        App::builder()
            .name("Demo")
            .plugin(FramePlugin::own())
            .plugin(WindowsPlugin)
            .build(ctx)
            .unwrap()
    });
    open_counting(&m, "a");
    m.root_pass();
    let va = m.app().window_viewport(&"a".into()).unwrap();
    m.set_inner_size(va, [420.0, 260.0]);
    m.set_outer_position(va, [300.0, 120.0]);
    m.focus(va);
    m.window_pass(va);
    (m, va)
}

/// The centre of a window button the last pass published, in points.
fn button_at(m: &MultiWindow, viewport: ViewportId, close: bool) -> egui::Pos2 {
    let regions = egui_pop_core::frame::HitTable::of(m.ctx())
        .get(viewport)
        .expect("the window publishes its frame's regions");
    let r = if close {
        regions.close
    } else {
        regions.maximize
    }
    .expect("the button is drawn");
    let ppp = m.ctx().pixels_per_point();
    egui::pos2(
        (r.x as f32 + r.w as f32 / 2.0) / ppp,
        (r.y as f32 + r.h as f32 / 2.0) / ppp,
    )
}

#[test]
fn an_application_window_draws_the_main_windows_title_bar() {
    let (mut m, va) = own_frame_with_a();
    // Built without the system's decorations: the application draws them.
    m.window_pass(va);
    let names = m.names(va);
    for name in ["Minimize", "Maximize", "Close"] {
        assert!(names.iter().any(|n| n == name), "{name} in {names:?}");
    }
    assert!(names.iter().any(|n| n.starts_with("A — ")), "{names:?}");
    // Its close button closes it, through the system's close request its
    // content may refuse.
    let at = button_at(&m, va, true);
    m.click(va, at);
    assert!(m.commands(va).contains(&ViewportCommand::Close));
}

#[test]
fn a_window_moved_into_the_main_window_keeps_its_geometry_and_a_title_bar() {
    let (mut m, _) = own_frame_with_a();
    m.request_close(ViewportId::ROOT);
    m.root_pass();
    // The main window takes A's size and place, rather than A's content
    // showing where the main window was.
    let sent = m.commands(ViewportId::ROOT);
    assert!(sent.contains(&ViewportCommand::CancelClose));
    assert!(
        sent.contains(&ViewportCommand::InnerSize(egui::vec2(420.0, 260.0))),
        "{sent:?}"
    );
    assert!(
        sent.contains(&ViewportCommand::OuterPosition(egui::pos2(300.0, 120.0))),
        "{sent:?}"
    );
    m.root_pass();
    // The shell's title bar left with the shell: A's content has one of
    // its own, with A's title and the window's buttons.
    let names = m.names(ViewportId::ROOT);
    for name in ["Minimize", "Maximize", "Close"] {
        assert!(names.iter().any(|n| n == name), "{name} in {names:?}");
    }
    assert!(names.iter().any(|n| n.starts_with("A — ")), "{names:?}");
    // Its buttons work on the main window.
    let at = button_at(&m, ViewportId::ROOT, false);
    m.click(ViewportId::ROOT, at);
    assert!(
        m.commands(ViewportId::ROOT)
            .contains(&ViewportCommand::Maximized(true))
    );
    let at = button_at(&m, ViewportId::ROOT, true);
    m.click(ViewportId::ROOT, at);
    assert!(
        m.commands(ViewportId::ROOT)
            .contains(&ViewportCommand::Close)
    );
}

#[test]
fn raising_the_main_window_brings_its_shell_title_bar_and_geometry_back() {
    use egui_pop_api::windows::CMD_SWITCH;
    let (mut m, _) = own_frame_with_a();
    m.request_close(ViewportId::ROOT);
    m.root_pass();
    m.root_pass();
    m.app()
        .run(CMD_SWITCH, serde_json::json!({ "window": "main" }));
    m.root_pass();
    // The main window's own size comes back (MultiWindow's first size).
    let sent = m.commands(ViewportId::ROOT);
    assert!(
        sent.contains(&ViewportCommand::InnerSize(egui::vec2(640.0, 480.0))),
        "{sent:?}"
    );
    m.root_pass();
    // The shell's bar again: the application's name at its start, and no
    // longer A's title.
    let names = m.names(ViewportId::ROOT);
    assert!(names.iter().any(|n| n == "Demo"), "{names:?}");
    assert!(names.iter().any(|n| n == "Close"), "{names:?}");
    assert!(!names.iter().any(|n| n.starts_with("A — ")), "{names:?}");
}

const MAIN_ONLY: CommandId = CommandId::from_static("demo.main_only");

/// Commands in the main window's File menu: one for any window, one for
/// the main window only.
struct FileMenu(Ran);

impl Plugin for FileMenu {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo.file")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        use egui_pop_api::app::MENU_FILE;
        let ran = self.0.clone();
        ctx.command(
            CommandSpec::new(HERE, "Here", "Demo").menu(MENU_FILE, "1_demo", 0),
            move |c| {
                ran.borrow_mut()
                    .push(("here".into(), c.app.active_app_window().to_string()));
                Ok(Json::Null)
            },
        )?
        .command(
            CommandSpec::new(MAIN_ONLY, "Main only", "Demo")
                .when("appWindow == 'main'")
                .menu(MENU_FILE, "1_demo", 1),
            |_| Ok(Json::Null),
        )?;
        Ok(())
    }
}

/// Clicks the node a window's last pass named `name`.
fn click_named(m: &mut MultiWindow, viewport: ViewportId, name: &str) {
    let (rect, _) = m
        .node(viewport, name)
        .unwrap_or_else(|| panic!("no `{name}` in {:?}", m.names(viewport)));
    m.click(viewport, rect.center());
}

#[test]
fn a_window_with_the_shell_bars_has_the_main_title_bar_start_and_menus_run_in_it() {
    use egui_pop_api::frame::FramePlugin;
    use egui_pop_api::windows::WindowsPlugin;
    let ran = Ran::default();
    let file = ran.clone();
    let mut m = MultiWindow::new(move |ctx| {
        App::builder()
            .name("Demo")
            .plugin(FramePlugin::own())
            .plugin(WindowsPlugin)
            .plugin(FileMenu(file))
            .build(ctx)
            .unwrap()
    });
    m.app().open_window(
        WindowSpec::new("b", "B")
            .size([640.0, 480.0])
            .shell_bars(true),
        |ui, _| {
            ui.label("window B");
        },
    );
    open_counting(&m, "a");
    m.root_pass();
    let vb = m.app().window_viewport(&"b".into()).unwrap();
    let va = m.app().window_viewport(&"a".into()).unwrap();
    m.focus(vb);
    m.window_pass(vb);
    m.window_pass(va);
    // B: the title bar's start (the window menu's button, the
    // application's name) and the main menu, as the main window has.
    let names = m.names(vb);
    for name in ["Demo", "File", "Window", "Close"] {
        assert!(names.iter().any(|n| n == name), "{name} in {names:?}");
    }
    // A, without them: its title and buttons only.
    let names = m.names(va);
    assert!(!names.iter().any(|n| n == "File"), "{names:?}");
    assert!(!names.iter().any(|n| n == "Demo"), "{names:?}");
    // File in B: a command for any window, and the main window's greyed.
    click_named(&mut m, vb, "File");
    m.window_pass(vb);
    let (_, disabled) = m.node(vb, "Here").expect("Here in B's File menu");
    assert!(!disabled);
    let (main_only, disabled) = m.node(vb, "Main only").expect("Main only listed");
    assert!(disabled, "the main window's command is greyed in B");
    // Its tooltip says where it runs.
    m.queue(vb, egui::Event::PointerMoved(main_only.center()));
    let mut said = false;
    for _ in 0..90 {
        m.window_pass(vb);
        if m.names(vb).iter().any(|n| n == "Only in the Demo window") {
            said = true;
            break;
        }
    }
    assert!(said, "{:?}", m.names(vb));
    // A command run from B's menu runs in B's context.
    click_named(&mut m, vb, "Here");
    m.root_pass();
    assert_eq!(*ran.borrow(), [("here".to_owned(), "b".to_owned())]);
}

/// The modal of an in-app picker, as the files plugin names it.
fn picker_modal(id: u64) -> egui::Id {
    egui::Id::unique(("egui_pop.files", id))
}

#[test]
fn a_folder_chosen_from_an_application_window_is_asked_over_that_window() {
    use egui_pop_api::files::{self, FILES, FileRequest, FilesPlugin};

    let mut m = MultiWindow::new(|ctx| {
        App::builder()
            .plugin(FilesPlugin::in_app())
            .build(ctx)
            .unwrap()
    });
    let asked: Rc<RefCell<Option<egui_pop_api::host::Pending>>> = Rc::default();
    let a = asked.clone();
    let app = m.app().clone();
    m.app()
        .open_window(WindowSpec::new("import", "Import"), move |ui, _| {
            ui.label("import");
            if a.borrow().is_none() {
                let f = app.service(&FILES).unwrap();
                *a.borrow_mut() = Some(f.choose(&app, FileRequest::folder("Choose a folder…")));
            }
        });
    m.root_pass();
    let import = m.app().window_viewport(&"import".into()).unwrap();
    m.window_pass(import);
    let f = m.app().service(&FILES).unwrap();
    let id = f.open_ids()[0];
    // Asked in the import window's pass: that window owns the choice.
    assert_eq!(f.owner(id), Some(import));
    m.root_pass();
    m.window_pass(import);
    m.root_pass();
    // Its picker is drawn over the import window, not the main one, and
    // blocks it alone.
    assert_eq!(
        egui_pop_api::core::overlay::window_of(m.ctx(), picker_modal(id)),
        Some(import)
    );
    assert_eq!(
        egui_pop_api::core::overlay::modal_windows(m.ctx()),
        [import]
    );
    // Answered, it ends in that window's pass and its modal goes.
    f.deliver(id, Some(vec!["/tmp/photos".into()])).unwrap();
    m.root_pass();
    assert!(asked.borrow().as_ref().unwrap().take().is_none());
    m.window_pass(import);
    m.window_pass(import);
    let r = asked
        .borrow()
        .as_ref()
        .unwrap()
        .take()
        .expect("the choice ended");
    assert_eq!(
        files::chosen(&r).unwrap(),
        files::Chosen::Paths(vec!["/tmp/photos".into()])
    );
    assert_eq!(
        egui_pop_api::core::overlay::window_of(m.ctx(), picker_modal(id)),
        None
    );
}

#[test]
fn a_folder_chosen_from_the_main_window_is_asked_over_it() {
    use egui_pop_api::files::{FILES, FileRequest, FilesPlugin};

    let mut m = MultiWindow::new(|ctx| {
        App::builder()
            .plugin(FilesPlugin::in_app())
            .build(ctx)
            .unwrap()
    });
    open_counting(&m, "import");
    m.root_pass();
    let import = m.app().window_viewport(&"import".into()).unwrap();
    m.window_pass(import);
    let f = m.app().service(&FILES).unwrap();
    let _pending = f.choose(m.app(), FileRequest::folder("Choose a folder…"));
    let id = f.open_ids()[0];
    m.root_pass();
    m.window_pass(import);
    assert_eq!(f.owner(id), Some(ViewportId::ROOT));
    assert_eq!(
        egui_pop_api::core::overlay::window_of(m.ctx(), picker_modal(id)),
        Some(ViewportId::ROOT)
    );
}

#[test]
fn closing_the_window_a_folder_is_chosen_from_cancels_the_choice() {
    use egui_pop_api::files::{self, FILES, FileRequest, FilesPlugin};

    let mut m = MultiWindow::new(|ctx| {
        App::builder()
            .plugin(FilesPlugin::in_app())
            .build(ctx)
            .unwrap()
    });
    let asked: Rc<RefCell<Option<egui_pop_api::host::Pending>>> = Rc::default();
    let a = asked.clone();
    let app = m.app().clone();
    m.app()
        .open_window(WindowSpec::new("import", "Import"), move |ui, _| {
            ui.label("import");
            if a.borrow().is_none() {
                let f = app.service(&FILES).unwrap();
                *a.borrow_mut() = Some(f.choose(&app, FileRequest::folder("Choose a folder…")));
            }
        });
    m.root_pass();
    let import = m.app().window_viewport(&"import".into()).unwrap();
    m.window_pass(import);
    m.window_pass(import);
    let f = m.app().service(&FILES).unwrap();
    let id = f.open_ids()[0];
    m.request_close(import);
    m.window_pass(import);
    m.root_pass();
    m.root_pass();
    // Cancelled, not moved over the main window.
    assert!(f.open_ids().is_empty());
    assert_eq!(
        egui_pop_api::core::overlay::window_of(m.ctx(), picker_modal(id)),
        None
    );
    let r = asked
        .borrow()
        .as_ref()
        .unwrap()
        .take()
        .expect("the choice ended");
    assert_eq!(files::chosen(&r).unwrap(), files::Chosen::Cancelled);
}
