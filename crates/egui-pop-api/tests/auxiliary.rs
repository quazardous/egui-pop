//! Auxiliary windows: a panel detached into a window of its own, docked
//! back, its window's closing a step a plugin may veto; panels that may
//! not leave say so.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::steps::{Before, Outcome};
use egui_pop_api::workbench::{self, Change, STEP_CLOSE_WINDOW};
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_model::ids::{PanelId, PluginId};
use egui_pop_model::panels::{Area, PanelSpec};

const LOG: PanelId = PanelId::from_static("demo.log");
const PINNED: PanelId = PanelId::from_static("demo.pinned");

struct Demo;

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.panel(PanelSpec::new(LOG, "Log", Area::Bottom), |ui, _| {
            ui.label("log body");
        })?
        .panel(
            PanelSpec::new(PINNED, "Pinned", Area::EndSide).detachable(false),
            |ui, _| {
                ui.label("pinned body");
            },
        )?;
        ctx.before(STEP_CLOSE_WINDOW, |app, _| {
            if app.context().get("busy") == "true" {
                Before::Veto("the log is busy".into())
            } else {
                Before::Go
            }
        });
        Ok(())
    }
}

fn harness() -> Harness<'static, App> {
    let app = App::builder()
        .plugin(Demo)
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 600.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h
}

#[test]
fn a_panel_detaches_into_its_own_window_and_docks_back() {
    let mut h = harness();
    let app = h.state().handle().clone();
    assert!(app.detach_panel(&LOG).unwrap().ok());
    h.run();
    let layout = app.layout();
    assert_eq!(layout.area_of(&LOG), None);
    assert_eq!(layout.auxiliary.len(), 1);
    assert_eq!(layout.auxiliary[0].panel, LOG);
    // Drawn in its window (kittest embeds it in the main one).
    assert!(h.query_by_label("log body").is_some());
    // The window's closing is vetoed while busy, then docks it back.
    app.set_context("busy", "true");
    let out = app.change_workbench(Change::CloseWindow(LOG)).unwrap();
    assert_eq!(out.vetoed(), Some("the log is busy"));
    assert_eq!(app.layout().auxiliary.len(), 1);
    app.set_context("busy", "false");
    assert!(app.change_workbench(Change::CloseWindow(LOG)).unwrap().ok());
    h.run();
    assert_eq!(app.layout().area_of(&LOG), Some(Area::Bottom));
    assert!(app.layout().auxiliary.is_empty());
    assert!(h.query_by_label("log body").is_some());
}

#[test]
fn a_panel_that_may_not_leave_says_so() {
    let h = harness();
    let out = h.state().handle().detach_panel(&PINNED).unwrap();
    assert!(
        matches!(out, Outcome::Done(Err(ref e)) if e.contains("cannot be detached")),
        "{out:?}"
    );
    assert_eq!(
        h.state().handle().layout().area_of(&PINNED),
        Some(Area::EndSide)
    );
}

#[test]
fn a_saved_window_place_is_repaired_before_it_opens() {
    let mut h = harness();
    let app = h.state().handle().clone();
    app.detach_panel(&LOG);
    // A place far off every screen and a size past what can be drawn.
    let mut layout = app.layout();
    layout.auxiliary[0].rect = [1.0e7, 1.0e7, 20_000.0, 10.0];
    app.set_layout(layout);
    h.run();
    assert!(h.query_by_label("log body").is_some());
    assert_eq!(workbench::window_id(&LOG), workbench::window_id(&LOG));
}

#[test]
fn a_detached_window_is_titled_with_the_application_name_not_the_main_title() {
    let app = App::builder()
        .name("Demo")
        .plugin(Demo)
        .build(&egui::Context::default())
        .unwrap();
    assert_eq!(app.handle().name(), "Demo");
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 600.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    // The main window's title names a document as well.
    h.input_mut()
        .viewports
        .entry(egui::ViewportId::ROOT)
        .or_default()
        .title = Some("notes.txt — Demo".into());
    h.run();
    let app = h.state().handle().clone();
    assert!(app.detach_panel(&LOG).unwrap().ok());
    h.run();
    // kittest embeds the window in the main one, titled as its own.
    assert!(h.query_by_label("Log — Demo").is_some());
    assert!(h.query_by_label_contains("notes.txt").is_none());
}
