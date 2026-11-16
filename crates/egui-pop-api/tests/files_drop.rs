//! Files dropped from another program, on the bus: every window's pass
//! (the main window, an application window, a detached panel's window)
//! reads the files dropped on it once, into a `files.drop` request naming
//! that window. A listener taking them makes it `files.dropped`; with
//! nobody taking them it is `files.refused` (ADR 0003), never lost.

use egui::ViewportId;
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
