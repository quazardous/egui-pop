//! The window lifecycle on the bus (ADR 0029): the window points and
//! their listeners in a declared, reviewed order; the registry as the one
//! list of windows (`windows.list`); and a part that cares about a
//! window closing listening to `window.close` at a declared place,
//! finding what runs before it done.

use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use egui::ViewportId;
use egui_pop_api::admin::{self, APP_WINDOW, AdminPlugin, CMD_OPEN};
use egui_pop_api::bus::egui_pop_hooks::{ListenOpts, Point as _};
use egui_pop_api::bus::{
    self, FOCUS, REGISTRY, Reason, WINDOW_POINTS, WindowClose, WindowKind, WindowRequest,
};
use egui_pop_api::windows::{WindowId, WindowSpec, WindowsPlugin};
use egui_pop_api::{App, AppHandle, DefaultPlugins};
use egui_pop_test::windows::MultiWindow;
use serde_json::json;

fn multi() -> MultiWindow {
    MultiWindow::new(|ctx| {
        App::builder()
            .plugins(DefaultPlugins)
            .plugin(WindowsPlugin)
            .plugin(AdminPlugin)
            .build(ctx)
            .unwrap()
    })
}

fn listed(app: &AppHandle) -> Vec<(String, WindowKind, String)> {
    app.windows_list()
        .into_iter()
        .map(|w| (w.id, w.kind, w.owner))
        .collect()
}

/// The reviewed order of the window points' listeners: a change to it is
/// a change to this file, read in review.
#[test]
fn the_window_points_run_in_their_reviewed_order() {
    let m = multi();
    let dump = m.app().bus_order();
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/bus/window-points.txt");
    if std::env::var_os("UPDATE_BUS_ORDER").is_some() {
        fs::write(&path, &dump).unwrap();
    }
    let reviewed = fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        dump,
        reviewed,
        "the window points' order changed: review it, then UPDATE_BUS_ORDER=1 writes {}",
        path.display()
    );
    // `window.close`, as the ADR orders it.
    let close: Vec<String> = m
        .app()
        .bus()
        .listeners(WindowClose::ID)
        .unwrap()
        .into_iter()
        .map(|l| l.id)
        .collect();
    assert_eq!(
        close,
        [
            "window.children",
            "workbench.window",
            "admin.window",
            "window.focus",
            "window.registry",
            "window.place",
            "window.invariants"
        ]
    );
}

#[test]
fn no_window_listener_orders_itself_against_a_missing_one() {
    let m = multi();
    assert_eq!(
        bus::missing_targets(&m.app().bus(), &WINDOW_POINTS),
        Vec::<String>::new()
    );
    // One that does is found.
    m.app()
        .bus()
        .listen::<WindowClose>("test.ghost", ListenOpts::new().after("nobody"), |_, _| {
            Ok(())
        })
        .unwrap();
    assert_eq!(
        bus::missing_targets(&m.app().bus(), &WINDOW_POINTS),
        ["window.close: test.ghost -> nobody"]
    );
    // A second listener last on a point is refused.
    assert!(
        m.app()
            .bus()
            .listen::<WindowClose>("test.last", ListenOpts::new().last(), |_, _| Ok(()))
            .is_err()
    );
}

#[test]
fn the_registry_lists_every_window_with_its_kind_and_owner() {
    let mut m = multi();
    assert_eq!(
        listed(m.app()),
        [("main".into(), WindowKind::Main, "app".into())]
    );
    m.app().run(CMD_OPEN, json!({ "mode": "window" }));
    m.root_pass();
    m.root_pass();
    m.app()
        .open_window(WindowSpec::new("import", "Import"), |ui, _| {
            ui.label("cards");
        });
    assert_eq!(
        listed(m.app()),
        [
            ("main".into(), WindowKind::Main, "app".into()),
            (APP_WINDOW.into(), WindowKind::App, admin::OWNER.into()),
            ("import".into(), WindowKind::App, "app".into()),
        ]
    );
    let opened = m
        .app()
        .events_since(Default::default())
        .into_iter()
        .filter(|e| e.name == "window.opened")
        .count();
    assert_eq!(opened, 2);
    m.app().close_window(&WindowId::new("import"));
    let closed: Vec<_> = m
        .app()
        .events_since(Default::default())
        .into_iter()
        .filter(|e| e.name == "window.closed")
        .map(|e| e.data)
        .collect();
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0]["window"], "import");
    assert_eq!(closed[0]["reason"], "command");
    assert_eq!(listed(m.app()).len(), 2);
    // Unknown: refused, said.
    assert!(!m.app().request_window_close(WindowRequest::new(
        "nowhere",
        WindowKind::App,
        "test",
        Reason::Command
    )));
    assert!(
        m.app()
            .events_since(Default::default())
            .iter()
            .any(|e| e.name == "window.refused" && e.data["window"] == "nowhere")
    );
}

/// A part listening to `window.close` after the owner and before the
/// registry finds the Settings app closed and the window still held:
/// the same call, in the declared order, whoever asked.
#[test]
fn a_listener_placed_after_the_owner_finds_its_state_closed() {
    let mut m = multi();
    let seen: Arc<Mutex<Vec<(bool, bool)>>> = Arc::default();
    let app = m.app().clone();
    let weak_admin = app.service(&admin::ADMIN).unwrap();
    let registry_had = {
        let seen = seen.clone();
        move |req: &WindowRequest, _: &mut Vec<String>| {
            if req.window == APP_WINDOW {
                seen.lock()
                    .unwrap()
                    .push((weak_admin.is_open(), req.reason == Reason::User));
            }
            Ok(())
        }
    };
    app.bus()
        .listen::<WindowClose>(
            "test.after-owner",
            ListenOpts::new()
                .owner("test")
                .after(admin::WINDOW_LISTENER)
                .before(FOCUS)
                .before(REGISTRY),
            registry_had,
        )
        .unwrap();
    m.app().run(CMD_OPEN, json!({ "mode": "window" }));
    m.root_pass();
    m.root_pass();
    let v = m.app().window_viewport(&WindowId::new(APP_WINDOW)).unwrap();
    m.focus(v);
    m.window_pass(v);
    assert_eq!(m.app().active_window(), v);
    m.request_close(v);
    m.window_pass(v);
    assert_eq!(*seen.lock().unwrap(), [(false, true)]);
    // The focus went back to the main window, in the same call.
    assert_eq!(m.app().active_window(), ViewportId::ROOT);
}
