//! The bus is the application's (ADR 0030): given to the builder,
//! egui-pop declares its points on it, registers its listeners on it in
//! their reviewed order, and asks its requests on it; it constructs no
//! bus of its own.

use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use egui_pop_api::admin::AdminPlugin;
use egui_pop_api::bus::bus_contract::{
    Altered, AnyInput, AnyListener, AnyValue, Bus, CloneFn, Collected, FailureSink, FreshFn,
    ListenOpts, ListenerInfo, Point as _, PointInfo, PointKey, PointKind, PointSpec, Refused,
};
use egui_pop_api::bus::egui_pop_hooks::Kernel;
use egui_pop_api::bus::{self, CONTRACT, WINDOW_POINTS, WindowClose};
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
    let declared: Vec<String> = CONTRACT
        .iter()
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
    // Every listener of `window.close` it runs was registered through it.
    let mut registered = recording.calls("listen window.close ");
    let mut running: Vec<String> = recording
        .listeners(WindowClose::ID)
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
            .contains(&"window.open".to_string())
    );
    app.close_window(&WindowId::new("import"));
    assert!(
        recording
            .calls("alter ")
            .contains(&"window.close".to_string())
    );
    assert_eq!(app.windows_list().len(), 1);
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
            point: "window.open"
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
