//! The bus is the application's (ADR 0030): given to the builder,
//! egui-pop declares its points on it, registers its listeners on it in
//! their reviewed order, and asks its requests on it; it constructs no
//! bus of its own.

use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use egui_pop_api::admin::AdminPlugin;
use egui_pop_api::bus::bus_contract::{
    self, Altered, AnyInput, AnyListener, AnyLocalListener, AnyValue, Bus, CloneFn, Collected,
    FailureSink, FreshFn, ListenOpts, ListenerInfo, PendingRun, PendingWaker, Point, PointDecl,
    PointId, PointInfo, PointKey, PointKind, PointSpec, Refused,
};
use egui_pop_api::bus::bus_contract_crossing::BusCrossing;
use egui_pop_api::bus::bus_contract_journal::{
    BusJournal, JOURNALED, Journal, JournalEntry, Since,
};
use egui_pop_api::bus::egui_pop_hooks::Kernel;
use egui_pop_api::bus::{
    self, CONTRACT, PLACE, REGISTRY, Reason, WINDOW_POINTS, WindowClose, WindowKind, WindowRequest,
};
use egui_pop_api::windows::{WindowId, WindowSpec, WindowsPlugin};
use egui_pop_api::{App, DefaultPlugins};
use egui_pop_test::windows::MultiWindow;

/// A bus written by the application: it records every call, then hands
/// it to the default kernel to do the work.
#[derive(Default)]
struct Recording {
    calls: Mutex<Vec<String>>,
    kernel: Kernel,
}

impl Recording {
    fn record(&self, call: String) {
        self.calls.lock().unwrap().push(call);
    }

    fn calls(&self, prefix: &str) -> Vec<String> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter_map(|c| c.strip_prefix(prefix).map(str::to_owned))
            .collect()
    }
}

impl Bus for Recording {
    fn declare_point(&self, point: PointKey, spec: PointSpec) -> Result<(), Refused> {
        self.record(format!("declare {} {}", point.id, point.kind.as_str()));
        self.kernel.declare_point(point, spec)
    }

    fn listen_any(
        &self,
        point: PointKey,
        id: String,
        opts: ListenOpts,
        run: AnyListener,
    ) -> Result<(), Refused> {
        self.record(format!("listen {} {id}", point.id));
        self.kernel.listen_any(point, id, opts, run)
    }

    fn listen_local_any(
        &self,
        point: PointKey,
        id: String,
        opts: ListenOpts,
        run: AnyLocalListener,
    ) -> Result<(), Refused> {
        self.record(format!("listen local {} {id}", point.id));
        self.kernel.listen_local_any(point, id, opts, run)
    }

    fn run_pending(&self) -> Vec<PendingRun> {
        self.kernel.run_pending()
    }

    fn add_pending_waker(&self, wake: PendingWaker) {
        Bus::add_pending_waker(&self.kernel, wake);
    }

    fn unlisten(&self, point: &str, id: &str) -> bool {
        self.record(format!("unlisten {point} {id}"));
        self.kernel.unlisten(point, id)
    }

    fn forget(&self, owner: &str) -> usize {
        self.record(format!("forget {owner}"));
        self.kernel.forget(owner)
    }

    fn alter_any(
        &self,
        point: PointKey,
        input: AnyInput,
        value: AnyValue,
        clone: CloneFn,
    ) -> Altered<AnyValue> {
        self.record(format!("alter {}", point.id));
        Bus::alter_any(&self.kernel, point, input, value, clone)
    }

    fn collect_any(&self, point: PointKey, input: AnyInput, fresh: FreshFn) -> Collected<AnyValue> {
        self.record(format!("collect {}", point.id));
        Bus::collect_any(&self.kernel, point, input, fresh)
    }

    fn add_failure_sink(&self, sink: FailureSink) {
        Bus::add_failure_sink(&self.kernel, sink);
    }

    fn points(&self) -> Vec<PointInfo> {
        self.kernel.points()
    }

    fn listeners(&self, point: &str) -> Option<Vec<ListenerInfo>> {
        self.kernel.listeners(point)
    }
}

fn multi(bus: Arc<dyn Bus>) -> MultiWindow {
    MultiWindow::new(move |ctx| {
        App::builder()
            .plugins(DefaultPlugins)
            .plugin(WindowsPlugin)
            .plugin(AdminPlugin)
            .bus(bus.clone())
            .build(ctx)
            .unwrap()
    })
}

#[test]
fn an_application_bus_receives_egui_pops_points_listeners_and_requests() {
    let recording = Arc::new(Recording::default());
    let m = multi(recording.clone());
    let app = m.app().clone();

    // The points, from egui-pop's contract table, in its order, with
    // their kinds.
    // Then egui_stack's points, declared when the workbench gives its
    // dock space the application's bus.
    let declared: Vec<String> = CONTRACT
        .iter()
        .chain(egui_stack::bus::POINTS)
        .map(|d| format!("{} {}", d.id, d.kind.as_str()))
        .collect();
    assert_eq!(recording.calls("declare "), declared);
    assert!(CONTRACT.iter().any(|d| d.kind == PointKind::Event));

    // The listeners went on the application's bus, nowhere else: the
    // reviewed order file reads the same through it.
    assert!(Arc::ptr_eq(
        &(app.bus() as Arc<dyn Bus>),
        &(recording.clone() as Arc<dyn Bus>)
    ));
    let reviewed = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/bus/window-points.txt"),
    )
    .unwrap();
    assert_eq!(app.bus_order(), reviewed);
    assert_eq!(bus::order_dump(&*recording, &WINDOW_POINTS), reviewed);
    // Every listener of `pop.window.close` it runs was registered through it.
    let mut registered = recording.calls("listen pop.window.close ");
    let mut running: Vec<String> = recording
        .listeners(&WindowClose::ID)
        .unwrap()
        .into_iter()
        .map(|l| l.id)
        .collect();
    registered.sort();
    running.sort();
    assert_eq!(registered, running);

    // A request is asked on it.
    app.open_window(WindowSpec::new("import", "Import"), |ui, _| {
        ui.label("cards");
    });
    assert!(
        recording
            .calls("alter ")
            .contains(&"pop.window.open".to_string())
    );
    app.close_window(&WindowId::new("import"));
    assert!(
        recording
            .calls("alter ")
            .contains(&"pop.window.close".to_string())
    );
    assert_eq!(app.windows_list().len(), 1);
}

/// The bus-contract naming rule: several libraries share one bus, so
/// every point egui-pop declares and every listener it registers is under
/// its `pop.` prefix (egui_stack's own are under `stack.`).
#[test]
fn every_point_and_listener_of_egui_pop_has_its_prefix() {
    for d in CONTRACT {
        assert!(d.id.starts_with("pop."), "point `{}` lacks `pop.`", d.id);
    }
    for p in bus::POINTS {
        assert!(p.starts_with("pop."), "point `{p}` lacks `pop.`");
    }
    let recording = Arc::new(Recording::default());
    let _m = multi(recording.clone());
    let listens = recording.calls("listen ");
    assert!(listens.len() > 20, "{listens:#?}");
    for call in &listens {
        let call = call.strip_prefix("local ").unwrap_or(call);
        let (point, id) = call.split_once(' ').unwrap();
        assert!(
            id.starts_with("pop.") || id.starts_with("stack."),
            "listener `{id}` on `{point}` lacks `pop.`"
        );
    }
}

#[test]
fn a_second_application_on_one_bus_is_refused() {
    let shared: Arc<dyn Bus> = Arc::new(Kernel::new());
    let first = multi(shared.clone());
    let second = App::builder()
        .plugins(DefaultPlugins)
        .bus(shared.clone())
        .build(&egui::Context::default());
    assert!(matches!(
        second,
        Err(egui_pop_api::BuildError::Bus(Refused::AlreadyDeclared {
            point: "pop.window.open"
        }))
    ));
    // The first keeps its listeners.
    assert_eq!(
        first.app().bus_order(),
        fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/bus/window-points.txt")
        )
        .unwrap()
    );
}

/// A point the application declares on the bus it gives egui-pop.
struct HealthReport;

impl Point for HealthReport {
    const ID: PointId = PointId::new("health.report");
    type Input = ();
    type Value = Vec<String>;
    const KIND: PointKind = PointKind::Collect;
}

#[test]
fn the_debug_bus_collector_lists_every_point_on_the_bus() {
    let shared: Arc<dyn Bus> = Arc::new(Kernel::new());
    shared
        .declare::<HealthReport>(PointSpec::new("The application's health checks."))
        .unwrap();
    // Registered out of order: `app.disks` asks to run after `app.library`.
    shared
        .listen::<HealthReport>(
            "app.disks",
            ListenOpts::new().after("app.library"),
            |_, _| Ok(()),
        )
        .unwrap();
    shared
        .listen::<HealthReport>("app.library", ListenOpts::new(), |_, _| Ok(()))
        .unwrap();
    shared
        .listen::<HealthReport>(
            "app.backups",
            ListenOpts::new().after("app.nobody"),
            |_, _| Ok(()),
        )
        .unwrap();
    let m = multi(shared.clone());
    let snapshot = m.app().debug_snapshot()["bus"].clone();
    let points = snapshot["points"].as_array().unwrap();

    // Every point on the bus shows, egui-pop's and the application's.
    let ids: Vec<&str> = points
        .iter()
        .map(|p| p["point"].as_str().unwrap())
        .collect();
    let on_bus: Vec<&str> = shared.points().iter().map(|p| p.id).collect();
    assert_eq!(ids, on_bus);
    for decl in CONTRACT {
        let p = points
            .iter()
            .find(|p| p["point"] == decl.id.as_str())
            .unwrap();
        assert_eq!(p["by"], "egui_pop", "{}", decl.id);
        assert_eq!(p["kind"], decl.kind.as_str());
    }
    let health = points
        .iter()
        .find(|p| p["point"] == "health.report")
        .unwrap();
    assert_eq!(health["by"], "application");
    assert_eq!(health["kind"], "collect");
    assert_eq!(health["declared"], true);
    assert_eq!(health["description"], "The application's health checks.");
    // Its listeners, in the order they run.
    let listeners: Vec<&str> = health["listeners"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["id"].as_str().unwrap())
        .collect();
    let running: Vec<String> = shared
        .listeners(&HealthReport::ID)
        .unwrap()
        .into_iter()
        .map(|l| l.id)
        .collect();
    assert_eq!(listeners, running);
    let library = listeners.iter().position(|l| *l == "app.library").unwrap();
    let disks = listeners.iter().position(|l| *l == "app.disks").unwrap();
    assert!(library < disks, "{listeners:?}");
    // The missing target is said, and the order text names the point.
    let missing: Vec<&str> = snapshot["missing_targets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t.as_str().unwrap())
        .filter(|t| t.starts_with("health."))
        .collect();
    assert_eq!(missing, ["health.report: app.backups -> app.nobody"]);
    assert!(
        snapshot["order"]
            .as_str()
            .unwrap()
            .contains("health.report\n"),
    );
}

/// A local listener (state that is not `Send`, as an interface's is)
/// on a window point takes its place in the declared order: run inline
/// when the request is asked on the interface's thread; queued when it
/// is asked from a worker, and run at the next pass, before its
/// `On::Pass` handlers.
#[test]
fn a_local_listener_on_a_window_point_runs_in_order_on_the_interface_thread() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let shared: Arc<dyn Bus> = Arc::new(Kernel::new());
    let mut m = multi(shared.clone());
    let app = m.app().clone();
    // Each request heard: its window and what the listeners before did.
    type Heard = Rc<RefCell<Vec<(String, Vec<String>)>>>;
    let heard: Heard = Rc::default();
    let mine = heard.clone();
    shared
        .listen_local::<WindowClose>(
            "app.panel",
            ListenOpts::new().owner("app").after(REGISTRY).before(PLACE),
            move |req, did| {
                mine.borrow_mut().push((req.window.clone(), did.clone()));
                did.push("app.panel: heard".into());
                Ok(())
            },
        )
        .unwrap();
    let order: Vec<(String, bool)> = shared
        .listeners(&WindowClose::ID)
        .unwrap()
        .into_iter()
        .map(|l| (l.id, l.local))
        .collect();
    let at = order.iter().position(|(id, _)| id == "app.panel").unwrap();
    assert!(order[at].1, "shown as local");
    assert_eq!(order[at - 1].0, REGISTRY);
    assert_eq!(order[at + 1].0, PLACE);

    // Asked on the interface's thread: in its place, in the same call.
    app.open_window(WindowSpec::new("import", "Import"), |ui, _| {
        ui.label("cards");
    });
    m.root_pass();
    app.close_window(&WindowId::new("import"));
    {
        let heard = heard.borrow();
        assert_eq!(heard.len(), 1);
        assert_eq!(heard[0].0, "import");
        let did = &heard[0].1;
        assert!(did.iter().any(|d| d.starts_with(REGISTRY)), "{did:?}");
        assert!(!did.iter().any(|d| d.starts_with(PLACE)), "{did:?}");
    }

    // Asked from a worker: the others run there, this one waits for the
    // interface's next pass.
    let worker = shared.clone();
    let out = std::thread::spawn(move || {
        worker.alter::<WindowClose>(
            WindowRequest::new("nowhere", WindowKind::App, "worker", Reason::Command),
            Vec::new(),
        )
    })
    .join()
    .unwrap();
    assert_eq!(out.deferred, ["app.panel"]);
    assert_eq!(heard.borrow().len(), 1, "not run on the worker");
    m.root_pass();
    let heard = heard.borrow();
    assert_eq!(heard.len(), 2, "run at the next pass");
    assert_eq!(heard[1].0, "nowhere");
    assert!(app.run_pending().is_empty(), "run once");
}

/// With a bus that offers timings, the debug collector shows each
/// point's budget and passes and each listener's runs, and `slowest`
/// names who costs what; a pass over its point's budget is said once as
/// `bus.overBudget` (the next ones within a minute counted, not said),
/// its results kept.
#[test]
fn timings_show_in_the_collector_and_a_pass_over_budget_is_said() {
    use std::time::Duration;
    let shared: Arc<dyn Bus> = Arc::new(Kernel::new());
    shared
        .declare::<HealthReport>(
            PointSpec::new("The application's health checks.").budget(Duration::ZERO),
        )
        .unwrap();
    shared
        .listen::<HealthReport>("app.slow", ListenOpts::new(), |_, v| {
            std::thread::sleep(Duration::from_millis(2));
            v.push("slow".into());
            Ok(())
        })
        .unwrap();
    shared
        .listen::<HealthReport>("app.quick", ListenOpts::new(), |_, _| Ok(()))
        .unwrap();
    let mut m = multi(shared.clone());
    for _ in 0..3 {
        let out = shared.collect::<HealthReport>(());
        assert_eq!(out.parts.len(), 2, "a budget drops nothing");
    }
    m.root_pass();

    let said: Vec<_> = m
        .app()
        .events_since(Default::default())
        .into_iter()
        .filter(|e| e.name == "bus.overBudget" && e.data["point"] == "health.report")
        .collect();
    assert_eq!(said.len(), 1, "once a minute per point");
    let data = &said[0].data;
    assert_eq!(data["budget_us"], 0);
    assert_eq!(data["slowest"], "app.slow");
    assert!(data["took_us"].as_u64().unwrap() >= 2000);
    assert_eq!(data["listeners"].as_array().unwrap().len(), 2);
    assert_eq!(data["unsaid"], 0);

    let snapshot = m.app().debug_snapshot()["bus"].clone();
    let health = snapshot["points"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["point"] == "health.report")
        .unwrap()
        .clone();
    assert_eq!(health["budget_us"], 0);
    assert_eq!(health["timing"]["count"], 3);
    assert_eq!(health["timing"]["over_budget"], 3);
    let slow = &health["listeners"][0];
    assert_eq!(slow["id"], "app.slow");
    assert_eq!(slow["timing"]["count"], 3);
    assert!(slow["timing"]["max_us"].as_u64().unwrap() >= 2000);
    assert_eq!(slow["local"], false);
    assert_eq!(slow["threaded"], false);
    assert_eq!(slow["deadline_ms"], 1000);
    let slowest = snapshot["slowest"].as_array().unwrap();
    assert_eq!(slowest[0]["point"], "health.report");
    assert_eq!(slowest[0]["id"], "app.slow");
}

/// The application's fact: a photo was imported, journaled by its host.
struct PhotoImported;
impl Point for PhotoImported {
    const ID: PointId = PointId::new("photo.imported");
    const KIND: PointKind = PointKind::Event;
    type Input = String;
    type Value = ();
}

/// A host's journal reduced to what it was given.
#[derive(Default)]
struct HostJournal {
    published: Mutex<Vec<&'static str>>,
}

impl Journal for HostJournal {
    fn last_seq(&self) -> u64 {
        self.published.lock().unwrap().len() as u64
    }

    fn since(&self, _: u64, _: usize) -> Since {
        Since::new(Vec::new(), self.last_seq())
    }

    fn wait(&self, _: &[&str], _: u64, _: std::time::Duration) -> Option<JournalEntry> {
        None
    }

    fn publish_any(
        &self,
        point: PointKey,
        _: AnyInput,
        _: FreshFn,
        _: Option<u64>,
    ) -> Result<u64, Refused> {
        let mut published = self.published.lock().unwrap();
        published.push(point.id);
        Ok(published.len() as u64)
    }

    fn flush(&self, _: std::time::Duration) -> bool {
        true
    }
}

/// A journal the host supplies to the kernel is the one the
/// application's bus offers: reachable from `AppHandle::bus`, its points
/// shown journaled by the `bus` collector.
#[test]
fn a_host_journal_given_to_the_kernel_is_reachable_from_the_apps_bus() {
    let host = Arc::new(HostJournal::default());
    let shared: Arc<dyn Bus> = Arc::new(Kernel::new().with_journal(host.clone()));
    assert!(
        bus_contract::declare_all(
            &*shared,
            &[PointDecl::of::<PhotoImported>("A photo was imported.").facets(&[JOURNALED])],
        )
        .is_empty()
    );
    let m = multi(shared);
    let bus = m.app().bus();
    let journal = bus.journal().expect("the host's journal");
    // egui-pop's journal is that journal (one per process): its own
    // events go there, under `pop.journal`.
    m.app()
        .publish("test", None, "test.said", serde_json::Value::Null);
    let before = journal.last_seq();
    assert!(before > 0);
    assert_eq!(m.app().last_event(), before);
    assert_eq!(
        journal.publish::<PhotoImported>("/a.raw".into()),
        Ok(before + 1)
    );
    {
        let published = host.published.lock().unwrap();
        assert_eq!(published.last(), Some(&"photo.imported"));
        assert!(
            published[..published.len() - 1]
                .iter()
                .all(|p| *p == "pop.journal")
        );
    }
    assert!(bus.relay().is_none());

    let snapshot = m.app().debug_snapshot()["bus"].clone();
    assert_eq!(snapshot["capabilities"]["journal"], true);
    assert_eq!(snapshot["capabilities"]["relay"], false);
    assert_eq!(snapshot["capabilities"]["defer"], true);
    assert_eq!(snapshot["journal_seq"], journal.last_seq());
    let photo = snapshot["points"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["point"] == "photo.imported")
        .unwrap()
        .clone();
    assert_eq!(photo["journaled"], true);
    assert_eq!(photo["relayable"], false);
    let close = snapshot["points"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["point"] == WindowClose::ID.as_str())
        .unwrap()
        .clone();
    assert_eq!(close["journaled"], false);
}
