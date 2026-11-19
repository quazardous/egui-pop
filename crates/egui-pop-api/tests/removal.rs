//! A removed contributor's work in flight is said (ADR 0028): its
//! disconnection is a `plugin.disconnected` event and a log line, a run
//! of its command queued after it went is refused on the journal, and
//! the undo steps that went with it are counted. A key the user bound to
//! its command is dormant: it falls through to the next binding and
//! comes back with the plugin. Its running jobs are cancelled by the
//! host, or said orphaned. What it left on screen is closed or kept dormant: its dialog
//! is cancelled with the reason, a toast's action naming its command is
//! greyed, its panel's place is kept and given back, its components,
//! hosts and mounts go.

use egui_pop_model::jobs::JobEventKind;
use std::cell::Cell;
use std::rc::Rc;
use std::sync::{Mutex, Once};

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_api::dialogs::{Answer, Confirm, DIALOGS, DialogsPlugin, Info};
use egui_pop_api::host::Event;
use egui_pop_api::jobs::{JOBS, JobsPlugin};
use egui_pop_api::jobs_centre;
use egui_pop_api::mounts;
use egui_pop_api::toasts::{TOASTS, Toast, ToastsPlugin};
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_core::lock;
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::ids::{CommandId, OwnerId, PanelId, PluginId};
use egui_pop_model::jobs::{JobState, Request as JobRequest};
use egui_pop_model::keymap::{Action, DORMANT, Layer, Verdict};
use egui_pop_model::layout::{DormantPlace, Layout, Node, SavedLayout, schema};
use egui_pop_model::mount::{HostOffer, Mountable};
use egui_pop_model::panels::{Area, PanelSpec};
use egui_pop_model::values::{Change, Descriptor, Set, Value};
use egui_pop_test::wait::{Deadline, settle as settle_within};
use serde_json::{Value as Json, json};

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

const FIND: CommandId = CommandId::from_static("base.find");

/// A plugin whose command has the keys the user gives another: what the
/// key falls through to.
struct Base {
    ran: Rc<Cell<u32>>,
}

impl Plugin for Base {
    fn id(&self) -> PluginId {
        PluginId::from_static("base")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let ran = self.ran.clone();
        ctx.command(CommandSpec::new(FIND, "Find", "Base"), move |_| {
            ran.set(ran.get() + 1);
            Ok(Json::Null)
        })?;
        ctx.keybinding("Ctrl+K", FIND, "", Layer::Default)?;
        Ok(())
    }
}

#[test]
fn a_user_key_bound_to_a_gone_command_is_dormant_falls_through_and_comes_back() {
    capture();
    let (p, ran) = doomed("keyed", u32::MAX);
    let found = Rc::new(Cell::new(0));
    let app = App::builder()
        .plugin(Base { ran: found.clone() })
        .plugin(p)
        .build(&egui::Context::default())
        .unwrap();
    let h = app.handle().clone();
    h.rebind(ACT, Some("Ctrl+K".into()));
    let mut harness = Harness::builder()
        .with_size(egui::vec2(600.0, 400.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    harness.run();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::K);
    harness.run();
    assert_eq!(
        (ran.get(), found.get()),
        (1, 0),
        "the user's key wins while its command is there"
    );
    assert!(h.dormant_keys().is_empty());

    h.set_plugin_enabled(PluginId::from_static("keyed"), false);
    harness.run();
    let before = h.last_event();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::K);
    harness.run();
    // The key falls through to the next binding of it; nothing is
    // refused, the gone command is not asked for.
    assert_eq!((ran.get(), found.get()), (1, 1));
    let events = h.events_since(before);
    assert!(
        !events.iter().any(|e| e.name == "command.refused"),
        "{events:#?}"
    );
    // The shortcuts editor's model: the binding, dormant, with the
    // plugin; the command shows no key, and the key makes no conflict.
    let dormant = h.dormant_keys();
    assert_eq!(dormant.len(), 1, "{dormant:?}");
    assert_eq!(dormant[0].binding.keys.to_string(), "Ctrl+K");
    assert_eq!(dormant[0].binding.action, Action::Run(ACT));
    assert_eq!(dormant[0].binding.layer, Layer::User);
    assert_eq!(dormant[0].plugin, PluginId::from_static("keyed"));
    assert_eq!(dormant[0].plugin_title, "keyed");
    assert!(h.keys_for(&ACT).is_empty());
    assert_eq!(h.keys_for(&FIND), ["Ctrl+K"]);
    assert!(h.key_conflicts().is_empty());
    // It is still the user's, saved with their keymap.
    assert!(
        h.export_keymap(Layer::User)
            .bindings
            .iter()
            .any(|b| b.keys == "Ctrl+K" && b.command.as_deref() == Some("doomed.act"))
    );
    // The key trace says it.
    let traces = h.key_traces();
    assert!(
        traces.iter().any(|t| t.candidates.iter().any(|c| c.verdict
            == Verdict::Lost {
                to: "base.find".into(),
                because: DORMANT
            })),
        "{traces:#?}"
    );
    assert!(logged(
        log::Level::Info,
        "keys: `Ctrl+K` (User layer) is dormant: its command `doomed.act` went with plugin `keyed`"
    ));

    // Back: the user's key runs its command again.
    h.set_plugin_enabled(PluginId::from_static("keyed"), true);
    harness.run();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::K);
    harness.run();
    assert_eq!((ran.get(), found.get()), (2, 1));
    assert!(h.dormant_keys().is_empty());
    assert_eq!(h.keys_for(&ACT), ["Ctrl+K"]);
}

/// The job commands the test's host ran: request, job.
type Asked = Rc<std::cell::RefCell<Vec<(JobRequest, String)>>>;

/// An application with the jobs plugin, whose host cancels a job when
/// asked (it publishes `job.cancelled`), and the plugin `worker`.
fn with_jobs() -> (App, Asked) {
    let asked = Asked::default();
    let seen = asked.clone();
    let (p, _) = doomed("worker", u32::MAX);
    let mut app = App::builder()
        .plugin(JobsPlugin::default().run_locally(move |app, request, id| {
            seen.borrow_mut().push((request, id.to_string()));
            if request == JobRequest::Cancel {
                app.publish_job("host", JobEventKind::Cancelled, json!({ "id": id }));
            }
            Ok(Json::Null)
        }))
        .plugin(p)
        .build(&egui::Context::default())
        .unwrap();
    app.logic(&egui::Context::default());
    (app, asked)
}

#[test]
fn a_gone_plugins_running_job_is_cancelled_and_one_it_cannot_cancel_is_orphaned() {
    capture();
    let (mut app, asked) = with_jobs();
    let h = app.handle().clone();
    let start = |source: &str, data: Json| {
        h.publish_job(source, JobEventKind::Started, data);
    };
    // Run by the host for the plugin: one it can cancel, one it cannot.
    start(
        "host",
        json!({ "id": "export", "title": "Export", "owner": "worker" }),
    );
    start(
        "host",
        json!({ "id": "render", "title": "Render", "owner": "worker", "cancellable": false }),
    );
    // Started by its command.
    start(
        "host",
        json!({ "id": "sync", "title": "Sync", "command": "doomed.act" }),
    );
    // One it published itself, naming no owner; and someone else's.
    start("worker", json!({ "id": "scan", "title": "Scan" }));
    start(
        "host",
        json!({ "id": "other", "title": "Other", "owner": "someone" }),
    );
    settle(&mut app);
    let jobs = h.service(&JOBS).unwrap();
    assert_eq!(jobs.job("export").unwrap().owner.as_deref(), Some("worker"));
    // Started since the last pass: it counts too.
    start(
        "host",
        json!({ "id": "late", "title": "Late", "owner": "worker" }),
    );

    let before = h.last_event();
    h.set_plugin_enabled(PluginId::from_static("worker"), false);
    settle(&mut app);
    let events = h.events_since(before);
    let reason = "its plugin `worker` is disconnected";

    // Counted on the disconnection.
    let gone = event(&events, "plugin.disconnected");
    assert_eq!(gone.data["removed"]["jobs_cancelled"], 3);
    assert_eq!(gone.data["removed"]["jobs_orphaned"], 2);
    // The host was asked to cancel those naming the plugin, by the
    // framework, with the reason; it did.
    assert_eq!(
        *asked.borrow(),
        [
            (JobRequest::Cancel, "export".to_string()),
            (JobRequest::Cancel, "sync".to_string()),
            (JobRequest::Cancel, "late".to_string()),
        ]
    );
    let dispatched: Vec<&Event> = events
        .iter()
        .filter(|e| e.name == "command.dispatched" && e.data["command"] == "job.cancel")
        .collect();
    assert_eq!(dispatched.len(), 3);
    assert_eq!(dispatched[0].source, "script:egui_pop");
    assert_eq!(dispatched[0].data["args"]["id"], "export");
    assert_eq!(dispatched[0].data["args"]["reason"], reason);
    for id in ["export", "sync", "late"] {
        let job = jobs.job(id).unwrap();
        assert_eq!(job.state, JobState::Cancelled, "{id}");
        assert_eq!(job.reason.as_deref(), Some(reason), "{id}");
        assert_eq!(job.orphaned, None, "{id}");
    }
    // The others go on, said orphaned on the journal and shown as such.
    let orphaned: Vec<&Event> = events.iter().filter(|e| e.name == "job.orphaned").collect();
    assert_eq!(orphaned.len(), 2);
    assert_eq!(orphaned[0].source, "egui_pop");
    assert_eq!(orphaned[0].data["id"], "render");
    assert_eq!(orphaned[0].data["plugin"], "worker");
    assert_eq!(orphaned[0].data["reason"], reason);
    assert_eq!(orphaned[1].data["id"], "scan");
    for id in ["render", "scan"] {
        let job = jobs.job(id).unwrap();
        assert_eq!(job.state, JobState::Running, "{id}");
        assert_eq!(job.orphaned.as_deref(), Some("worker"), "{id}");
        let said = jobs_centre::state_text(&egui::Context::default(), &job);
        assert!(said.contains("worker") && said.contains("is off"), "{said}");
    }
    let other = jobs.job("other").unwrap();
    assert_eq!((other.state, other.orphaned), (JobState::Running, None));
    assert!(logged(
        log::Level::Info,
        "job `export`: the host is asked to cancel it: its plugin `worker` is disconnected"
    ));
    assert!(logged(
        log::Level::Info,
        "job `render` is orphaned: its plugin `worker` is disconnected"
    ));

    // An orphaned job can be forgotten while it goes on; the plugin
    // back, the other is its own again.
    assert!(jobs.dismiss("scan"));
    assert!(!jobs.dismiss("other"));
    let before = h.last_event();
    h.set_plugin_enabled(PluginId::from_static("worker"), true);
    settle(&mut app);
    let events = h.events_since(before);
    let adopted = event(&events, "job.adopted");
    assert_eq!(adopted.data["plugin"], "worker");
    assert_eq!(adopted.data["jobs"], json!(["render"]));
    assert_eq!(jobs.job("render").unwrap().orphaned, None);
}

/// A job left orphaned by its removed plugin goes on, owned by nobody:
/// a wait for the application to settle does not wait on it (it may
/// never end), and the reads say it apart.
#[test]
fn a_wait_for_idle_returns_with_an_orphaned_job_going_on() {
    capture();
    let (p, _) = doomed("worker", u32::MAX);
    let mut harness = egui_pop_test::app(
        App::builder()
            .plugin(JobsPlugin::default().run_locally(|_, _, _| Ok(Json::Null)))
            .plugin(p),
        egui::vec2(400.0, 300.0),
    );
    harness.run_ok();
    let h = harness.state().handle().clone();
    h.publish_job(
        "host",
        JobEventKind::Started,
        json!({ "id": "render", "title": "Render", "owner": "worker", "cancellable": false }),
    );
    // A pass: the job is followed (the jobs plugin folds the journal).
    harness.step();
    let short = Deadline {
        frames: 10,
        time: std::time::Duration::from_secs(2),
    };
    // Owned, it is waited on.
    assert!(settle_within(&mut harness, short).is_err());

    h.set_plugin_enabled(PluginId::from_static("worker"), false);
    let jobs = h.service(&JOBS).unwrap();
    let r = settle_within(&mut harness, Deadline::default());
    assert!(r.is_ok(), "{r:?}");
    assert_eq!(jobs.job("render").unwrap().state, JobState::Running);
    assert_eq!(
        jobs.job("render").unwrap().orphaned.as_deref(),
        Some("worker")
    );
    // Said apart: the model, the handle, the jobs collector and the
    // bridge's wait answer count it.
    assert!(jobs.idle());
    assert_eq!(jobs.orphans().len(), 1);
    assert_eq!(h.orphaned_jobs(), 1);
    let snap = h.debug_snapshot();
    assert_eq!(snap["jobs"]["idle"], true, "{snap}");
    assert_eq!(snap["jobs"]["orphaned"], 1, "{snap}");
    let f = h.debug_call("bridge", "wait", &json!({ "settled": true }));
    assert!(f.ok, "{}", f.message);
    assert_eq!(f.data["met"], true);
    assert_eq!(f.data["orphaned"], 1);

    // Its plugin back, it is owned and waited on again.
    h.set_plugin_enabled(PluginId::from_static("worker"), true);
    harness.run_ok();
    assert_eq!(jobs.job("render").unwrap().orphaned, None);
    assert!(!jobs.idle());
    assert_eq!(h.orphaned_jobs(), 0);
}

const OPEN: CommandId = CommandId::from_static("owned.open");
const DO: CommandId = CommandId::from_static("owned.do");
const LOG: PanelId = PanelId::from_static("owned.log");
const HOST: &str = "owned.host";
const WIDGET: &str = "owned.widget";

/// A plugin with a panel, a component and a host; its `owned.open`
/// mounts a component in its host, opens a dialog on its `owned.do` and
/// shows a toast offering it.
struct Owned;

impl Plugin for Owned {
    fn id(&self) -> PluginId {
        PluginId::from_static("owned")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.command(CommandSpec::new(DO, "Do", "Owned"), |_| Ok(Json::Null))?;
        ctx.command(CommandSpec::new(OPEN, "Open", "Owned"), |call| {
            call.app.mount(mounts::PROGRESS, HOST)?;
            call.app
                .service(&DIALOGS)
                .ok_or("no dialogs")?
                .open(Confirm::new("Do it?", "Do", DO));
            call.app.service(&TOASTS).ok_or("no toasts")?.notify(
                Toast::info("Owned note")
                    .sticky()
                    .action("Do again", DO, Json::Null),
            );
            Ok(Json::Null)
        })?;
        ctx.panel(PanelSpec::new(LOG, "Owned log", Area::Bottom), |ui, _| {
            ui.label("log");
        })?;
        ctx.mountable(Mountable::new(WIDGET, "Owned widget"));
        ctx.host_offer(HostOffer::new(HOST, "O", "Owned host"));
        Ok(())
    }
}

#[test]
fn what_a_plugin_left_on_screen_is_cancelled_greyed_kept_dormant_or_removed() {
    capture();
    let app = App::builder()
        .plugin(DialogsPlugin)
        .plugin(ToastsPlugin)
        .plugin(Owned)
        .build(&egui::Context::default())
        .unwrap();
    let h = app.handle().clone();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(900.0, 600.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    harness.run();
    // Its panel moved by the user to the end side bar.
    h.set_layout(Layout {
        areas: vec![(Area::EndSide, Node::tab(LOG))],
        ..Layout::default()
    });
    harness.run();
    assert_eq!(h.layout().area_of(&LOG), Some(Area::EndSide));
    h.run_as(OPEN, Json::Null, Origin::User);
    harness.run();
    // A mount of nobody's, which stays.
    h.mount(mounts::PROGRESS, mounts::PANEL).unwrap();
    let dialogs = h.service(&DIALOGS).unwrap();
    dialogs.open(Info::new("Unrelated", "Stays open."));
    assert_eq!(dialogs.len(), 2);
    assert_eq!(dialogs.current().unwrap().1.title(), "Do it?");
    assert!(h.mounts().iter().any(|m| m.host == HOST));
    assert!(h.mountables().iter().any(|m| m.id == WIDGET));

    let before = h.last_event();
    h.set_plugin_enabled(PluginId::from_static("owned"), false);
    harness.run();
    harness.run();
    let events = h.events_since(before);

    // Its dialog is cancelled, saying why; the other one shows.
    let cancelled = event(&events, "dialog.cancelled");
    assert_eq!(cancelled.source, "egui_pop");
    assert_eq!(cancelled.data["title"], "Do it?");
    assert_eq!(
        cancelled.data["reason"],
        "its plugin `owned` is disconnected"
    );
    assert!(!events.iter().any(|e| e.name == "command.refused"));
    assert_eq!(
        dialogs.len(),
        1,
        "answered: {:?}; events: {:?}",
        dialogs.answered(),
        events
            .iter()
            .map(|e| (e.name.clone(), e.data.clone()))
            .collect::<Vec<_>>()
    );
    assert_eq!(dialogs.current().unwrap().1.title(), "Unrelated");
    assert!(logged(
        log::Level::Info,
        "dialog `Do it?` cancelled: its plugin `owned` is disconnected"
    ));
    dialogs.answer(Answer::Cancel, Origin::User).unwrap();
    harness.run();
    harness.run();
    assert!(dialogs.is_empty());

    // Its components, hosts and mounts are gone, counted; others stay.
    let gone = event(&events, "plugin.disconnected");
    assert_eq!(gone.data["removed"]["components"], 1);
    assert_eq!(gone.data["removed"]["hosts"], 1);
    assert_eq!(gone.data["removed"]["mounts"], 1);
    assert!(!h.mountables().iter().any(|m| m.id == WIDGET));
    assert!(!h.host_offers().iter().any(|o| o.id == HOST));
    let mounted: Vec<(String, String)> = h
        .mounts()
        .into_iter()
        .map(|m| (m.component, m.host))
        .collect();
    assert_eq!(
        mounted,
        [(mounts::PROGRESS.to_string(), mounts::PANEL.to_string())]
    );

    // The toast's action is greyed, saying why.
    let toasts = h.service(&TOASTS).unwrap();
    assert_eq!(toasts.shown().len(), 1);
    let action = harness.get_by_label("Do again");
    assert!(action.accesskit_node().is_disabled());

    // Its panel's place is kept, dormant, in what is saved.
    let saved = h.saved_layout();
    assert!(!saved.layout.placed().contains(&&LOG));
    let kept = saved.dormant.iter().find(|d| d.panel == LOG);
    assert!(
        matches!(
            kept.map(|d| &d.place),
            Some(DormantPlace::Docked {
                area: Area::EndSide,
                ..
            })
        ),
        "{:?}",
        saved.dormant
    );
    let text = schema().save(&saved).unwrap();
    let read: SavedLayout = schema().load(&text).unwrap().value;
    assert_eq!(read, saved);
    assert!(logged(
        log::Level::Info,
        "layout: panel `owned.log` is not declared: its place is kept"
    ));

    // Back: the panel where it was, the toast's action alive again; the
    // dialog stays closed.
    h.set_plugin_enabled(PluginId::from_static("owned"), true);
    harness.run();
    harness.run();
    let saved = h.saved_layout();
    assert_eq!(saved.layout.area_of(&LOG), Some(Area::EndSide), "{saved:?}");
    assert!(saved.dormant.is_empty());
    assert!(
        !harness
            .get_by_label("Do again")
            .accesskit_node()
            .is_disabled()
    );
    assert!(dialogs.is_empty());
    assert!(h.mountables().iter().any(|m| m.id == WIDGET));
}
