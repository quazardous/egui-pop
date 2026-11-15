//! A removed contributor's work in flight is said (ADR 0028): its
//! disconnection is a `plugin.disconnected` event and a log line, a run
//! of its command queued after it went is refused on the journal, a key
//! still bound to it warns, and the undo steps that went with it are
//! counted.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::{Mutex, Once};

use egui_kittest::Harness;
use egui_pop_api::host::Event;
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_core::lock;
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::ids::{CommandId, OwnerId, PluginId};
use egui_pop_model::values::{Change, Descriptor, Set, Value};
use serde_json::Value as Json;

const ACT: CommandId = CommandId::from_static("doomed.act");
const LEVEL: &str = "doomed/level";
const MINE: &str = "user/n";

/// The log lines written, for every test of this binary.
static LINES: Mutex<Vec<(log::Level, String)>> = Mutex::new(Vec::new());

struct Capture;

impl log::Log for Capture {
    fn enabled(&self, m: &log::Metadata<'_>) -> bool {
        m.level() <= log::Level::Info
    }

    fn log(&self, r: &log::Record<'_>) {
        if self.enabled(r.metadata()) {
            LINES
                .lock()
                .unwrap()
                .push((r.level(), r.args().to_string()));
        }
    }

    fn flush(&self) {}
}

fn capture() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        log::set_logger(&Capture).unwrap();
        log::set_max_level(log::LevelFilter::Info);
    });
}

fn logged(level: log::Level, part: &str) -> bool {
    LINES
        .lock()
        .unwrap()
        .iter()
        .any(|(l, m)| *l == level && m.contains(part))
}

/// A plugin with a command and a value; `fail_after` connections, its
/// connection fails (after it registered them).
struct Doomed {
    id: &'static str,
    ran: Rc<Cell<u32>>,
    connects: Rc<Cell<u32>>,
    fail_after: u32,
}

impl Plugin for Doomed {
    fn id(&self) -> PluginId {
        PluginId::new(self.id).unwrap()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let ran = self.ran.clone();
        ctx.command(CommandSpec::new(ACT, "Act", "Doomed"), move |_| {
            ran.set(ran.get() + 1);
            Ok(Json::Null)
        })?;
        lock(&ctx.app().store())
            .declare(ctx.owner(), LEVEL, Descriptor::int("Level", 0, 99, 0))
            .unwrap();
        self.connects.set(self.connects.get() + 1);
        if self.connects.get() > self.fail_after {
            return Err(PluginError::Failed("it fails now".into()));
        }
        Ok(())
    }
}

fn doomed(id: &'static str, fail_after: u32) -> (Doomed, Rc<Cell<u32>>) {
    let ran = Rc::new(Cell::new(0));
    let p = Doomed {
        id,
        ran: ran.clone(),
        connects: Rc::default(),
        fail_after,
    };
    (p, ran)
}

fn app(p: Doomed) -> App {
    let mut app = App::builder()
        .plugin(p)
        .build(&egui::Context::default())
        .unwrap();
    app.logic(&egui::Context::default());
    app
}

fn settle(app: &mut App) {
    app.logic(&egui::Context::default());
    app.logic(&egui::Context::default());
}

fn event<'a>(events: &'a [Event], name: &str) -> &'a Event {
    events
        .iter()
        .find(|e| e.name == name)
        .unwrap_or_else(|| panic!("no `{name}` in {events:#?}"))
}

fn set(app: &App, path: &str, v: i64) {
    lock(&app.handle().store())
        .set(Set::new(path, Change::To(Value::Int(v)), Origin::User))
        .unwrap();
}

#[test]
fn a_plugin_disabled_says_so_and_a_run_queued_after_it_is_refused() {
    capture();
    let (p, ran) = doomed("doomed", u32::MAX);
    let mut app = app(p);
    let h = app.handle().clone();
    let store = h.store();
    lock(&store)
        .declare(&OwnerId::USER, MINE, Descriptor::int("N", 0, 99, 0))
        .unwrap();
    // A step of its own value, one mixing it with the user's, one of
    // the user's alone.
    set(&app, LEVEL, 1);
    {
        let mut s = lock(&store);
        let g = s.begin_gesture(Origin::User, "both");
        s.set(Set::new(LEVEL, Change::To(Value::Int(2)), Origin::User).gesture(g))
            .unwrap();
        s.set(Set::new(MINE, Change::To(Value::Int(2)), Origin::User).gesture(g))
            .unwrap();
        s.end(g);
    }
    set(&app, MINE, 3);
    assert_eq!(lock(&store).undo_steps().len(), 3);

    let before = h.last_event();
    h.set_plugin_enabled(PluginId::from_static("doomed"), false);
    h.run_as(ACT, Json::Null, Origin::User);
    settle(&mut app);

    let events = h.events_since(before);
    let gone = event(&events, "plugin.disconnected");
    assert_eq!(gone.source, "egui_pop");
    assert_eq!(gone.data["plugin"], "doomed");
    assert_eq!(gone.data["owner"], "doomed");
    assert_eq!(gone.data["reason"], "disabled");
    assert_eq!(gone.data["removed"]["commands"], 1);
    assert_eq!(gone.data["removed"]["undo_steps"], 2);
    assert_eq!(gone.data["removed"]["mixed_undo_steps"], 1);
    assert_eq!(gone.data["undo"][0]["history"], "app");
    let refused = event(&events, "command.refused");
    assert_eq!(refused.source, "user");
    assert_eq!(refused.data["command"], "doomed.act");
    assert_eq!(
        refused.data["reason"],
        "its plugin `doomed` is disconnected"
    );
    assert_eq!(ran.get(), 0);
    // The user's step alone is left.
    assert_eq!(lock(&store).undo_steps().len(), 1);
    assert!(logged(
        log::Level::Info,
        "plugin `doomed` disconnected (disabled): 1 command(s)"
    ));
    assert!(logged(
        log::Level::Info,
        "2 undo step(s) dropped with `doomed` (1 also changing other values)"
    ));

    // Back: its command runs again, and a run of a command never
    // declared is refused as such.
    h.set_plugin_enabled(PluginId::from_static("doomed"), true);
    settle(&mut app);
    h.run_as(ACT, Json::Null, Origin::User);
    let before = h.last_event();
    h.run_as(
        CommandId::from_static("never.was"),
        Json::Null,
        Origin::User,
    );
    settle(&mut app);
    assert_eq!(ran.get(), 1);
    let events = h.events_since(before);
    let refused = event(&events, "command.refused");
    assert_eq!(refused.data["command"], "never.was");
    assert!(
        !refused.data["reason"]
            .as_str()
            .unwrap()
            .contains("disconnected")
    );
}

#[test]
fn a_plugin_failing_to_connect_says_so() {
    capture();
    let (p, _) = doomed("flaky", 1);
    let mut app = app(p);
    let h = app.handle().clone();
    h.set_plugin_enabled(PluginId::from_static("flaky"), false);
    settle(&mut app);
    let before = h.last_event();
    h.set_plugin_enabled(PluginId::from_static("flaky"), true);
    h.run_as(ACT, Json::Null, Origin::User);
    settle(&mut app);
    let events = h.events_since(before);
    let gone = event(&events, "plugin.disconnected");
    assert_eq!(gone.data["plugin"], "flaky");
    assert_eq!(gone.data["reason"], "failed connect");
    assert_eq!(gone.data["removed"]["commands"], 1);
    let refused = event(&events, "command.refused");
    assert_eq!(refused.data["reason"], "its plugin `flaky` is disconnected");
    assert!(logged(
        log::Level::Info,
        "plugin `flaky` disconnected (failed connect)"
    ));
}

#[test]
fn a_key_still_bound_to_a_gone_command_warns_and_is_refused() {
    capture();
    let (p, ran) = doomed("keyed", u32::MAX);
    let app = app(p);
    let h = app.handle().clone();
    h.rebind(ACT, Some("Ctrl+K".into()));
    let mut harness = Harness::builder()
        .with_size(egui::vec2(600.0, 400.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    harness.run();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::K);
    harness.run();
    assert_eq!(ran.get(), 1, "the key runs it while it is there");

    h.set_plugin_enabled(PluginId::from_static("keyed"), false);
    harness.run();
    let before = h.last_event();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::K);
    harness.run();
    assert_eq!(ran.get(), 1);
    let events = h.events_since(before);
    let refused = event(&events, "command.refused");
    assert_eq!(refused.source, "user");
    assert_eq!(refused.data["command"], "doomed.act");
    assert_eq!(refused.data["reason"], "its plugin `keyed` is disconnected");
    assert!(logged(
        log::Level::Warn,
        "its plugin `keyed` is disconnected"
    ));
}
