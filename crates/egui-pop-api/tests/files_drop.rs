//! Files dropped from another program, on the bus: every window's pass
//! (the main window, an application window, a detached panel's window)
//! reads the files dropped on it once, into a `files.drop` request naming
//! that window. A listener taking them makes it `files.dropped`; with
//! nobody taking them it is `files.refused` (ADR 0003), never lost.
//! Nothing but the bus's files module reads egui's dropped files: the
//! file browser takes a drop through it, said by its listener.

use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use bus_contract::ListenOpts;
use egui::ViewportId;
use egui_pop_api::bus::{self, FILES_BROWSER, FILES_POINTS, FilesDrop};
use egui_pop_api::files::browser::{AppDropTarget, FileBrowser, FsSource};
use egui_pop_api::files::fs::MemoryFs;
use egui_pop_api::host::Event;
use egui_pop_api::windows::WindowSpec;
use egui_pop_api::workbench;
use egui_pop_api::{App, AppHandle, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_model::ids::{PanelId, PluginId};
use egui_pop_model::panels::{Area, PanelSpec};
use egui_pop_test::windows::MultiWindow;

const LOG: PanelId = PanelId::from_static("demo.log");

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

fn multi() -> MultiWindow {
    let mut m = MultiWindow::new(|ctx| {
        App::builder()
            .name("Demo")
            .plugins(DefaultPlugins)
            .plugin(Demo)
            .build(ctx)
            .unwrap()
    });
    m.root_pass();
    m
}

fn events(app: &AppHandle, name: &str) -> Vec<Event> {
    app.events_since(Default::default())
        .into_iter()
        .filter(|e| e.name == name)
        .collect()
}

/// The windows the events of `name` name, in order.
fn windows_of(app: &AppHandle, name: &str) -> Vec<String> {
    events(app, name)
        .iter()
        .map(|e| e.data["window"].as_str().unwrap_or_default().to_string())
        .collect()
}

/// A detached log panel and an application window, each a viewport of
/// its own.
fn two_windows(m: &mut MultiWindow) -> (ViewportId, ViewportId) {
    assert!(m.app().detach_panel(&LOG).unwrap().ok());
    m.app()
        .open_window(WindowSpec::new("import", "Import"), |ui, _| {
            ui.label("import body");
        });
    m.root_pass();
    m.root_pass();
    let panel = workbench::window_id(&LOG);
    assert!(m.shown_immediate().contains(&panel));
    let import = m.app().window_viewport(&"import".into()).unwrap();
    m.window_pass(import);
    (panel, import)
}

#[test]
fn files_dropped_on_any_window_nobody_takes_are_refused_naming_that_window() {
    let mut m = multi();
    let (panel, import) = two_windows(&mut m);

    // On the detached panel's window: its pass runs inside the main one's.
    m.drop_files(panel, &["/home/me/a.cr3"]);
    m.root_pass();
    // On the application window.
    m.drop_files(import, &["/home/me/b.cr3", "/home/me/c.cr3"]);
    m.window_pass(import);
    // On the main window.
    m.drop_files(ViewportId::ROOT, &["/home/me/d.cr3"]);
    m.root_pass();
    // Passes without a drop say nothing more.
    m.root_pass();
    m.window_pass(import);

    let app = m.app();
    assert_eq!(
        windows_of(app, "files.refused"),
        ["panel:demo.log", "import", "main"]
    );
    let refused = events(app, "files.refused");
    assert_eq!(
        refused[1].data["paths"],
        serde_json::json!(["/home/me/b.cr3", "/home/me/c.cr3"])
    );
    assert!(events(app, "files.dropped").is_empty());
}

/// The files dropped a listener of the application took: the window and
/// the paths, each drop once.
type Taken = Arc<Mutex<Vec<(String, Vec<String>)>>>;

/// The application's import, a listener of `files.drop` after the file
/// browser's: it takes what no browser took.
fn listen_import(app: &AppHandle) -> Taken {
    let taken = Taken::default();
    let t = taken.clone();
    app.bus()
        .listen::<FilesDrop>(
            "demo.import",
            ListenOpts::new().owner("demo").after(FILES_BROWSER),
            move |drop, did| {
                if !did.is_empty() {
                    return Ok(());
                }
                // The application asking, for a listener keeping no handle.
                assert!(bus::asking().is_some());
                let paths = drop.paths.iter().map(|p| p.display().to_string()).collect();
                t.lock().unwrap().push((drop.window.clone(), paths));
                did.push(format!("demo.import: {} files", drop.paths.len()));
                Ok(())
            },
        )
        .unwrap();
    taken
}

#[test]
fn files_dropped_on_a_detached_window_and_an_app_window_are_dropped_once_each_naming_it() {
    let mut m = multi();
    let taken = listen_import(m.app());
    let (panel, import) = two_windows(&mut m);

    m.drop_files(panel, &["/home/me/a.cr3"]);
    m.root_pass();
    m.drop_files(import, &["/home/me/b.cr3"]);
    m.window_pass(import);
    m.root_pass();
    m.window_pass(import);

    let app = m.app();
    assert_eq!(
        windows_of(app, "files.dropped"),
        ["panel:demo.log", "import"]
    );
    let dropped = events(app, "files.dropped");
    assert_eq!(
        dropped[0].data["paths"],
        serde_json::json!(["/home/me/a.cr3"])
    );
    assert_eq!(
        dropped[1].data["did"],
        serde_json::json!(["demo.import: 1 files"])
    );
    assert!(events(app, "files.refused").is_empty());
    assert_eq!(
        *taken.lock().unwrap(),
        [
            (
                "panel:demo.log".to_string(),
                vec!["/home/me/a.cr3".to_string()]
            ),
            ("import".to_string(), vec!["/home/me/b.cr3".to_string()]),
        ]
    );
}

#[test]
fn files_dropped_on_a_window_not_open_are_refused() {
    let m = multi();
    listen_import(m.app());
    let did = m
        .app()
        .request_files_drop(bus::FileDrop::new("gone", vec!["/home/me/a.cr3".into()]));
    assert!(did.is_empty());
    let refused = events(m.app(), "files.refused");
    assert_eq!(refused.len(), 1);
    assert_eq!(refused[0].data["reason"], "no open window of this id");
}

#[test]
fn a_file_browser_takes_files_dropped_on_its_target_said_by_its_listener() {
    let mut m = multi();
    let taken = listen_import(m.app());
    let fs = MemoryFs::new().dir("/r").file("/r/IMG_001.png", vec![0; 3]);
    let mut browser = FileBrowser::new("drop-browser", Arc::new(FsSource::new(Arc::new(fs))), "/r")
        .with_drops(|_| true)
        .with_drop_targets(vec![AppDropTarget::new(
            "catalogue",
            "Add to the catalogue",
        )]);
    let got = Arc::new(Mutex::new(Vec::new()));
    let g = got.clone();
    m.app()
        .open_window(WindowSpec::new("browse", "Browse"), move |ui, _| {
            let out = browser.show(ui);
            g.lock().unwrap().extend(out.dropped);
        });
    m.root_pass();
    let v = m.app().window_viewport(&"browse".into()).unwrap();
    for _ in 0..3 {
        m.window_pass(v);
    }
    let (zone, _) = m
        .node(v, "Add to the catalogue")
        .expect("the drop target is drawn");
    m.queue(v, egui::Event::PointerMoved(zone.center()));
    m.hover_files(v, &["/home/me/raw.cr3"]);
    m.window_pass(v);
    m.drop_files(v, &["/home/me/raw.cr3"]);
    m.window_pass(v);

    assert_eq!(got.lock().unwrap().len(), 1);
    let dropped = events(m.app(), "files.dropped");
    assert_eq!(dropped.len(), 1);
    assert_eq!(dropped[0].data["window"], "browse");
    assert_eq!(
        dropped[0].data["did"],
        serde_json::json!(["files.browser: a file browser took them for `catalogue`"])
    );
    // Taken by the browser: the application's import, after it, left it.
    assert!(taken.lock().unwrap().is_empty());
}

/// The reviewed order of the files point's listeners.
#[test]
fn the_files_point_runs_in_its_reviewed_order() {
    let m = multi();
    let dump = m.app().bus_order_of(&FILES_POINTS);
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/bus/files-points.txt");
    if std::env::var_os("UPDATE_BUS_ORDER").is_some() {
        fs::write(&path, &dump).unwrap();
    }
    let reviewed = fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        dump,
        reviewed,
        "review it, then UPDATE_BUS_ORDER=1 writes {}",
        path.display()
    );
    assert_eq!(
        bus::missing_targets(&m.app().bus(), &FILES_POINTS),
        Vec::<String>::new()
    );
}

/// Only the bus's files module reads egui's hovered and dropped files:
/// every window's pass goes through it, and a widget reads what it read.
#[test]
fn only_the_bus_files_module_reads_the_files_dropped() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let allowed = Path::new("egui-pop-api/src/bus/files.rs");
    let mut found = Vec::new();
    let mut dirs: Vec<_> = fs::read_dir(&crates)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path().join("src"))
        .filter(|p| p.is_dir())
        .collect();
    while let Some(dir) = dirs.pop() {
        for entry in fs::read_dir(&dir).unwrap().filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                dirs.push(path);
                continue;
            }
            if path.extension().is_none_or(|e| e != "rs") || path.ends_with(allowed) {
                continue;
            }
            let text = fs::read_to_string(&path).unwrap();
            for (n, line) in text.lines().enumerate() {
                if line.contains(".dropped_files") || line.contains(".hovered_files") {
                    found.push(format!("{}:{}: {}", path.display(), n + 1, line.trim()));
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "read outside the bus's files module: {found:#?}"
    );
}
