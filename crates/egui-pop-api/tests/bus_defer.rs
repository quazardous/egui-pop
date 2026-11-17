//! Requests asked for after the drawing, and every command run on the
//! bus.
//!
//! A panel cannot have a request heard while it draws with its state
//! borrowed: it asks it for after the drawing (`AppHandle::ask`, the
//! bus's `Defer` capability), and the application drains the asks once
//! per pass after the panels and windows are drawn, and before it quits.
//!
//! Every command run is asked on `command.run` first, a fail-closed
//! request: a listener refusing it, or failing, stops it before its
//! handler, and the journal says `command.refused` with who and why.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::bus::bus_contract::{self, HookError, ListenOpts, Point, PointDecl, PointKind};
use egui_pop_api::bus::{self, COMMAND_POINTS, CONTRACT, CommandAsk, CommandRun};
use egui_pop_api::{App, AppBuilder, Ctx, Plugin, PluginError};
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::ids::{CommandId, PanelId, PluginId};
use egui_pop_model::panels::{Area, PanelSpec};
use serde_json::Value as Json;

/// A request of the test's own: a line noted by whoever hears it.
#[derive(Debug)]
struct Note;
impl Point for Note {
    const KIND: PointKind = PointKind::Request;
    const ID: &'static str = "test.note";
    type Input = String;
    type Value = Vec<String>;
}

const PANEL: PanelId = PanelId::from_static("demo.asker");
const COUNT: CommandId = CommandId::from_static("demo.count");

type Log = Rc<RefCell<Vec<String>>>;

/// A panel that asks `test.note` while it draws with its state
/// borrowed, when `asking` is set; and a command counting its runs.
struct Demo {
    log: Log,
    state: Rc<RefCell<u32>>,
    asking: Rc<RefCell<bool>>,
    ran: Rc<RefCell<u32>>,
}

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let (log, state, asking) = (self.log.clone(), self.state.clone(), self.asking.clone());
        ctx.panel(
            PanelSpec::new(PANEL, "Asker", Area::EndSide),
            move |ui, app| {
                let mut drawing = state.borrow_mut();
                *drawing += 1;
                ui.label("asker body");
                if asking.replace(false) {
                    log.borrow_mut().push("panel drew".into());
                    assert!(
                        app.ask::<Note>("from the panel".into(), Vec::new())
                            .is_some()
                    );
                }
            },
        )?;
        let ran = self.ran.clone();
        ctx.command(
            CommandSpec::new(COUNT, "Count", "Demo").key("Ctrl+U"),
            move |_| {
                *ran.borrow_mut() += 1;
                Ok(Json::Null)
            },
        )?;
        Ok(())
    }
}

struct Fixture {
    log: Log,
    state: Rc<RefCell<u32>>,
    asking: Rc<RefCell<bool>>,
    ran: Rc<RefCell<u32>>,
}

fn fixture() -> (Fixture, Demo) {
    let f = Fixture {
        log: Rc::default(),
        state: Rc::default(),
        asking: Rc::default(),
        ran: Rc::default(),
    };
    let demo = Demo {
        log: f.log.clone(),
        state: f.state.clone(),
        asking: f.asking.clone(),
        ran: f.ran.clone(),
    };
    (f, demo)
}

fn build(b: AppBuilder) -> App {
    b.build(&egui::Context::default()).unwrap()
}

/// Declares `test.note` and listens to it on the interface's thread: the
/// listener notes what it heard, and whether the panel's state was free.
fn hear_notes(app: &App, f: &Fixture) {
    let bus = app.handle().bus();
    let refused = bus_contract::declare_all(&*bus, &[PointDecl::of::<Note>("A test note.")]);
    assert!(refused.is_empty(), "{refused:?}");
    let (log, state) = (f.log.clone(), f.state.clone());
    bus.listen_local::<Note>(
        "test.hear",
        ListenOpts::new().owner("test"),
        move |note, _| {
            let free = state.try_borrow_mut().is_ok();
            log.borrow_mut()
                .push(format!("heard {note} (state free: {free})"));
            Ok(())
        },
    )
    .unwrap();
}

#[test]
fn an_ask_made_in_a_panel_is_heard_after_the_panels_in_the_same_pass() {
    let (f, demo) = fixture();
    let app = build(App::builder().plugin(demo));
    hear_notes(&app, &f);
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 500.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.state().handle().set_panel_visible(&PANEL, true);
    h.run();
    assert!(*f.state.borrow() > 0, "the panel is drawn");

    *f.asking.borrow_mut() = true;
    h.step();
    assert_eq!(
        *f.log.borrow(),
        ["panel drew", "heard from the panel (state free: true)"],
        "heard in the pass that asked it, after the panel drew"
    );
    assert_eq!(h.state().handle().asks_waiting(), 0);
}

#[test]
fn an_ask_made_just_before_quit_is_heard() {
    let (f, demo) = fixture();
    let log = f.log.clone();
    let mut app = build(App::builder().plugin(demo).on_exit(move |app| {
        log.borrow_mut().push("on_exit".into());
        assert!(app.ask::<Note>("from on_exit".into(), Vec::new()).is_some());
    }));
    hear_notes(&app, &f);
    let handle = app.handle().clone();
    assert!(
        handle
            .ask::<Note>("before quit".into(), Vec::new())
            .is_some()
    );
    assert_eq!(handle.asks_waiting(), 1);
    let _ = app.exit();
    assert_eq!(
        *f.log.borrow(),
        [
            "heard before quit (state free: true)",
            "on_exit",
            "heard from on_exit (state free: true)",
        ]
    );
    assert_eq!(handle.asks_waiting(), 0, "nothing lost on the way out");
}

#[test]
fn command_run_is_declared_fail_closed() {
    let decl = CONTRACT
        .iter()
        .find(|d| d.id == CommandRun::ID)
        .expect("in egui-pop's contract");
    assert_eq!(decl.kind, PointKind::Request);
    let app = build(App::builder());
    let info = app
        .handle()
        .bus()
        .points()
        .into_iter()
        .find(|p| p.id == CommandRun::ID)
        .expect("declared on the bus");
    assert!(info.fail_closed);
}

#[test]
fn the_command_points_are_reviewed() {
    let app = build(App::builder());
    let dump = app.handle().bus_order_of(&COMMAND_POINTS);
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/bus/command-points.txt");
    if std::env::var_os("UPDATE_BUS_ORDER").is_some() {
        std::fs::write(&path, &dump).unwrap();
    }
    let reviewed = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        dump,
        reviewed,
        "review it, then UPDATE_BUS_ORDER=1 writes {}",
        path.display()
    );
    for p in COMMAND_POINTS {
        assert!(bus::POINTS.contains(&p), "{p} in the points");
    }
}

#[test]
fn a_command_runs_as_before_when_nobody_refuses() {
    let (f, demo) = fixture();
    let mut app = build(App::builder().plugin(demo));
    let seen: Arc<Mutex<Vec<String>>> = Arc::default();
    let mine = seen.clone();
    app.handle()
        .bus()
        .listen::<CommandRun>(
            "test.watch",
            ListenOpts::new().owner("test"),
            move |ask: &CommandAsk, _| {
                mine.lock().unwrap().push(ask.command.to_string());
                Ok(())
            },
        )
        .unwrap();
    let before = app.handle().last_event();
    assert!(
        app.run(&COUNT, Json::Null, Origin::User)
            .now()
            .unwrap()
            .is_ok()
    );
    assert_eq!(*f.ran.borrow(), 1);
    assert_eq!(*seen.lock().unwrap(), ["demo.count"]);
    let names: Vec<String> = app
        .handle()
        .events_since(before)
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert!(
        names.contains(&"command.dispatched".to_string()),
        "{names:?}"
    );
    assert!(names.contains(&"command.done".to_string()), "{names:?}");
    assert!(!names.contains(&"command.refused".to_string()), "{names:?}");
}

#[test]
fn a_refusing_listener_stops_a_palette_command_and_says_why() {
    let (f, demo) = fixture();
    let app = build(
        App::builder()
            .plugins(egui_pop_api::DefaultPlugins)
            .plugin(demo),
    );
    app.handle()
        .bus()
        .listen::<CommandRun>("test.policy", ListenOpts::new().owner("test"), |ask, _| {
            if ask.command == COUNT {
                return Err(HookError::refuse("an edit is not saved"));
            }
            Ok(())
        })
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(800.0, 500.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    let before = h.state().handle().last_event();
    h.key_press_modifiers(
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
        egui::Key::P,
    );
    h.run();
    assert!(
        egui_pop_api::palette::is_open(h.state().handle()),
        "the palette itself is not refused"
    );
    h.get_by_role(egui::accesskit::Role::TextInput)
        .type_text("count");
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    assert_eq!(*f.ran.borrow(), 0, "the handler did not run");
    let events = h.state().handle().events_since(before);
    let refused = events
        .iter()
        .find(|e| e.name == "command.refused" && e.data["command"] == "demo.count")
        .expect("the refusal is journaled");
    assert_eq!(refused.data["by"], "test.policy");
    assert_eq!(refused.data["failed"], false);
    assert_eq!(
        refused.data["reason"],
        "refused by `test.policy`: an edit is not saved"
    );
    assert!(
        !events
            .iter()
            .any(|e| e.name == "command.dispatched" && e.data["command"] == "demo.count"),
        "never dispatched to the host"
    );
}

#[test]
fn a_failing_listener_on_the_fail_closed_point_refuses_the_run() {
    let (f, demo) = fixture();
    let mut app = build(App::builder().plugin(demo));
    app.handle()
        .bus()
        .listen::<CommandRun>("test.broken", ListenOpts::new().owner("test"), |_, _| {
            Err("the policy table is unreadable".to_string().into())
        })
        .unwrap();
    let before = app.handle().last_event();
    let out = app.run(&COUNT, Json::Null, Origin::User).now().unwrap();
    assert_eq!(
        out.unwrap_err(),
        "refused, `test.broken` failed: the policy table is unreadable"
    );
    assert_eq!(*f.ran.borrow(), 0, "never run on a failed check");
    let refused = app
        .handle()
        .events_since(before)
        .into_iter()
        .find(|e| e.name == "command.refused")
        .expect("said");
    assert_eq!(refused.data["by"], "test.broken");
    assert_eq!(refused.data["failed"], true);
}
